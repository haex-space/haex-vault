//! Contact photos (spec 051 FR-005, research R10): at most 512 × 512 pixels and 200 KiB as JPEG, so
//! the address book does not weigh down the sync. Stored once per content in
//! `haex_contacts_binaries` (key: SHA-256 of the JPEG); a photo no contact uses any more is marked
//! and removed after seven days, so a contact of another device that has not arrived yet still
//! finds it (as the password manager does with its binaries).

use std::io::Cursor;

use haex_crdt::rusqlite::params;
use haex_crdt::CrdtTransaction;
use image::codecs::jpeg::JpegEncoder;
use image::ImageReader;

use crate::error::{HolziError, Result};
use crate::files::thumbnails::upright_within;
use crate::passwords::binaries::hash_bytes;
use crate::passwords::clock;
use crate::storage::query::Query;

/// The longest edge of a stored photo.
pub const EDGE: u32 = 512;

/// The most bytes of a stored photo.
pub const MAX_BYTES: usize = 200 * 1024;

/// Days an unused photo is kept before it is deleted.
pub const ORPHAN_GRACE_DAYS: i64 = 7;

/// JPEG qualities tried in turn until the photo fits [`MAX_BYTES`].
const QUALITIES: [u8; 5] = [85, 75, 65, 50, 35];

fn not_an_image() -> HolziError {
    HolziError::ContactsInvalid {
        reason: "not an image".to_owned(),
    }
}

/// The photo in `bytes` turned upright, shrunk to [`EDGE`] and encoded as a JPEG of at most
/// [`MAX_BYTES`]. Blocking: decoding and encoding take a moment.
pub fn shrink(bytes: &[u8]) -> Result<Vec<u8>> {
    let reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| not_an_image())?;
    let small = upright_within(reader, EDGE).map_err(|_| not_an_image())?;
    for quality in QUALITIES {
        let mut jpeg = Vec::new();
        JpegEncoder::new_with_quality(&mut Cursor::new(&mut jpeg), quality)
            .encode_image(&small)
            .map_err(|_| not_an_image())?;
        if jpeg.len() <= MAX_BYTES {
            return Ok(jpeg);
        }
    }
    Err(HolziError::ContactsInvalid {
        reason: "photo too large".to_owned(),
    })
}

/// Stores a shrunk photo (or finds it stored) and returns its hash; a stored photo that was
/// marked unused loses the mark.
pub fn store(tx: &mut CrdtTransaction<'_>, jpeg: &[u8]) -> Result<String> {
    let hash = hash_bytes(jpeg);
    let known = tx
        .query_row(
            "SELECT COUNT(*) FROM haex_contacts_binaries WHERE hash = ?1",
            params![hash],
            |r| r.get::<_, i64>(0),
        )?
        .unwrap_or(0)
        > 0;
    if known {
        tx.execute(
            "UPDATE haex_contacts_binaries SET orphaned_at = NULL \
             WHERE hash = ?1 AND orphaned_at IS NOT NULL",
            params![hash],
        )?;
    } else {
        tx.execute(
            "INSERT INTO haex_contacts_binaries (hash, data) VALUES (?1, ?2)",
            params![hash, jpeg],
        )?;
    }
    Ok(hash)
}

/// The JPEG of a stored photo.
pub fn data(q: &mut impl Query, hash: &str) -> Result<Vec<u8>> {
    q.query_row(
        "SELECT data FROM haex_contacts_binaries WHERE hash = ?1",
        params![hash],
        |r| r.get::<_, Vec<u8>>(0),
    )?
    .ok_or(HolziError::ContactsNotFound)
}

/// Marks a photo unused once no contact names it any more.
pub fn mark_if_unused(tx: &mut CrdtTransaction<'_>, hash: &str) -> Result<()> {
    if !is_used(tx, hash)? {
        tx.execute(
            "UPDATE haex_contacts_binaries SET orphaned_at = ?1 \
             WHERE hash = ?2 AND orphaned_at IS NULL",
            params![clock::now(), hash],
        )?;
    }
    Ok(())
}

fn is_used(q: &mut impl Query, hash: &str) -> Result<bool> {
    Ok(q.query_row(
        "SELECT COUNT(*) FROM haex_contacts WHERE photo = ?1",
        params![hash],
        |r| r.get::<_, i64>(0),
    )?
    .unwrap_or(0)
        > 0)
}

/// Cleans up photos once when the vault opens: an unused photo is marked, a used one loses its
/// mark, one unused for [`ORPHAN_GRACE_DAYS`] is deleted. Returns how many were deleted.
pub fn prune(tx: &mut CrdtTransaction<'_>, now_millis: i64) -> Result<u32> {
    let cutoff = clock::format_millis(now_millis - ORPHAN_GRACE_DAYS * 86_400_000);
    let now = clock::format_millis(now_millis);
    let rows: Vec<(String, Option<String>)> = tx.query_map(
        "SELECT hash, orphaned_at FROM haex_contacts_binaries",
        &[],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let mut deleted = 0;
    for (hash, orphaned_at) in rows {
        match (is_used(tx, &hash)?, orphaned_at) {
            (true, Some(_)) => {
                tx.execute(
                    "UPDATE haex_contacts_binaries SET orphaned_at = NULL WHERE hash = ?1",
                    params![hash],
                )?;
            }
            (false, None) => {
                tx.execute(
                    "UPDATE haex_contacts_binaries SET orphaned_at = ?1 WHERE hash = ?2",
                    params![now, hash],
                )?;
            }
            (false, Some(_)) => {
                let expired = tx
                    .query_row(
                        "SELECT COUNT(*) FROM haex_contacts_binaries \
                         WHERE hash = ?1 AND datetime(orphaned_at) <= datetime(?2)",
                        params![hash, cutoff],
                        |r| r.get::<_, i64>(0),
                    )?
                    .unwrap_or(0)
                    > 0;
                if expired {
                    tx.execute(
                        "DELETE FROM haex_contacts_binaries WHERE hash = ?1",
                        params![hash],
                    )?;
                    deleted += 1;
                }
            }
            (true, None) => {}
        }
    }
    Ok(deleted)
}

#[cfg(test)]
#[path = "photos_tests.rs"]
mod tests;
