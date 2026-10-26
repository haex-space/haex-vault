//! Attachments and the binary data behind them (spec 034, US5, FR-019..FR-022, research R4).
//!
//! The data lies as a BLOB in `haex_passwords_binaries`, unique by the SHA-256 of the raw bytes;
//! an entry (or a history state) links it under a file name. The size limit (25 MiB) is checked on
//! the file's metadata **before** the file is read where a size is known, the file name is stored as text (never a path),
//! and the data is read only by its own query, never in a list.
//!
//! Clean-up: when the last link to a binary goes, it is marked (`orphaned_at`) and removed only
//! after seven days, so a link of another device that has not arrived yet does not run into
//! nothing. A custom icon (`type = 'icon'`) has no link row; it counts as used while some entry,
//! folder, passkey or history state names `binary:<hash>`.

use std::io::Read;

use haex_crdt::rusqlite::params;
use haex_crdt::CrdtTransaction;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::model::AttachmentView;
use super::{clock, snapshots, ATTACHMENT_LIMIT_BYTES, ORPHAN_GRACE_DAYS};
use crate::error::{HolziError, Result};
use crate::files::picked::{self, Opener, PickedFile};
use crate::storage::query::Query;

fn invalid(reason: &str) -> HolziError {
    HolziError::InvalidInput {
        reason: reason.to_string(),
    }
}

/// The SHA-256 of the raw bytes as lowercase hex: the key of a binary.
pub fn hash_bytes(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// A file name as text, never as a path (FR-021): path separators and control characters become
/// `_`, at most 255 characters; an empty result becomes `file`.
pub fn sanitize_file_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c == '/' || c == '\\' || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .take(255)
        .collect();
    if cleaned.is_empty() {
        "file".to_string()
    } else {
        cleaned
    }
}

/// Reads a chosen file for an attachment: where the size is known it is checked first (above the
/// limit it is refused without reading a byte), otherwise reading stops one byte past the limit. An
/// empty file is `empty`, an unreadable one `unreadable`. Returns the name to show and the bytes.
/// Blocking: run it on a blocking thread.
pub fn read_attachment_file(opener: &impl Opener, file: &PickedFile) -> Result<(String, Vec<u8>)> {
    let source = picked::open_read(opener, file).map_err(unreadable)?;
    let known = source
        .metadata()
        .ok()
        .filter(std::fs::Metadata::is_file)
        .map(|metadata| metadata.len());
    if let Some(size) = known.filter(|size| *size > ATTACHMENT_LIMIT_BYTES) {
        return Err(HolziError::PasswordsAttachmentTooLarge {
            bytes: size,
            limit: ATTACHMENT_LIMIT_BYTES,
        });
    }
    let mut bytes = Vec::new();
    source
        .take(ATTACHMENT_LIMIT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid("unreadable"))?;
    let size = bytes.len() as u64;
    if size > ATTACHMENT_LIMIT_BYTES {
        return Err(HolziError::PasswordsAttachmentTooLarge {
            bytes: size,
            limit: ATTACHMENT_LIMIT_BYTES,
        });
    }
    if size == 0 {
        return Err(invalid("empty"));
    }
    Ok((picked::display_name(opener, file), bytes))
}

/// A chosen file that gives nothing back keeps the reason the window already knows.
fn unreadable(error: HolziError) -> HolziError {
    match error {
        HolziError::Unreadable => invalid("unreadable"),
        other => other,
    }
}

/// Writes bytes to the file chosen in the save dialog. Blocking.
pub fn save_to(opener: &impl Opener, file: &PickedFile, bytes: &[u8]) -> Result<()> {
    picked::write(opener, file, bytes).map_err(|error| match error {
        HolziError::InvalidInput { .. } | HolziError::NotEnoughSpace => error,
        _ => invalid("unwritable"),
    })
}

/// Attaches bytes to an entry under a file name: the binary row is created if its hash is new
/// (otherwise its grace mark ends), the link row is added, and the entry gets a new history state.
/// The size limit is checked here as well, for callers that already hold the bytes.
pub fn add_bytes(
    tx: &mut CrdtTransaction<'_>,
    item_id: &str,
    file_name: &str,
    data: &[u8],
) -> Result<AttachmentView> {
    if data.is_empty() {
        return Err(invalid("empty"));
    }
    let size = data.len() as u64;
    if size > ATTACHMENT_LIMIT_BYTES {
        return Err(HolziError::PasswordsAttachmentTooLarge {
            bytes: size,
            limit: ATTACHMENT_LIMIT_BYTES,
        });
    }
    let item_exists = tx
        .query_row(
            "SELECT COUNT(*) FROM haex_passwords_item_details WHERE id = ?1",
            params![item_id],
            |r| r.get::<_, i64>(0),
        )?
        .unwrap_or(0)
        > 0;
    if !item_exists {
        return Err(HolziError::PasswordsNotFound);
    }
    let hash = hash_bytes(data);
    ensure_binary(tx, &hash, data, "attachment")?;
    let view = link_attachment(tx, item_id, file_name, &hash, size)?;
    snapshots::take_snapshot(tx, item_id)?;
    Ok(view)
}

/// Makes sure the binary row of `hash` exists: a known one just ends its grace period, a new one is
/// inserted with the given type (`attachment` or `icon`). Returns whether a row was inserted, so a
/// caller that may have to undo its work knows what it created.
pub fn ensure_binary(
    tx: &mut CrdtTransaction<'_>,
    hash: &str,
    data: &[u8],
    kind: &str,
) -> Result<bool> {
    let known = tx
        .query_row(
            "SELECT COUNT(*) FROM haex_passwords_binaries WHERE hash = ?1",
            params![hash],
            |r| r.get::<_, i64>(0),
        )?
        .unwrap_or(0)
        > 0;
    if known {
        clear_orphan_mark(tx, hash)?;
        return Ok(false);
    }
    tx.execute(
        "INSERT INTO haex_passwords_binaries (hash, data, size, type, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![hash, data, data.len() as i64, kind, clock::now()],
    )?;
    Ok(true)
}

/// Links an entry to a binary under a (sanitised) file name, without a history state.
pub fn link_attachment(
    tx: &mut CrdtTransaction<'_>,
    item_id: &str,
    file_name: &str,
    hash: &str,
    size: u64,
) -> Result<AttachmentView> {
    let name = sanitize_file_name(file_name);
    let id = Uuid::new_v4().to_string();
    tx.execute(
        "INSERT INTO haex_passwords_item_binaries (id, item_id, binary_hash, file_name) \
         VALUES (?1, ?2, ?3, ?4)",
        params![id, item_id, hash, name],
    )?;
    Ok(AttachmentView {
        id,
        file_name: name,
        size,
        binary_hash: hash.to_string(),
    })
}

/// The link of an attachment: the entry, the binary and the name.
fn link_of(tx: &mut CrdtTransaction<'_>, attachment_id: &str) -> Result<(String, String)> {
    tx.query_row(
        "SELECT item_id, binary_hash FROM haex_passwords_item_binaries WHERE id = ?1",
        params![attachment_id],
        |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
    )?
    .ok_or(HolziError::PasswordsNotFound)
}

/// Renames an attachment on its entry only; returns the stored (sanitized) name.
pub fn rename_attachment(
    tx: &mut CrdtTransaction<'_>,
    attachment_id: &str,
    new_name: &str,
) -> Result<String> {
    let (item_id, _) = link_of(tx, attachment_id)?;
    let name = sanitize_file_name(new_name);
    tx.execute(
        "UPDATE haex_passwords_item_binaries SET file_name = ?1 WHERE id = ?2",
        params![name, attachment_id],
    )?;
    snapshots::take_snapshot(tx, &item_id)?;
    Ok(name)
}

/// Removes the link of an attachment; the binary stays (another entry or a history state may
/// still link it) and is cleaned up after the grace period once nothing links it.
pub fn remove_link(tx: &mut CrdtTransaction<'_>, attachment_id: &str) -> Result<()> {
    let (item_id, hash) = link_of(tx, attachment_id)?;
    tx.execute(
        "DELETE FROM haex_passwords_item_binaries WHERE id = ?1",
        params![attachment_id],
    )?;
    mark_if_unreferenced(tx, &hash)?;
    snapshots::take_snapshot(tx, &item_id)?;
    Ok(())
}

/// The name and the data of an attachment, by their own query (no list ever selects `data`).
pub fn attachment_data(q: &mut impl Query, attachment_id: &str) -> Result<(String, Vec<u8>)> {
    q.query_row(
        "SELECT ib.file_name, b.data FROM haex_passwords_item_binaries ib \
         JOIN haex_passwords_binaries b ON b.hash = ib.binary_hash WHERE ib.id = ?1",
        params![attachment_id],
        |r| Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?)),
    )?
    .ok_or(HolziError::PasswordsNotFound)
}

/// The data and the media type of an image attachment (`png`, `jpg`, `jpeg`, `gif`, `webp`), for
/// the thumbnail and the lightbox; anything else is `not_previewable` here. Text and PDF reach the
/// attachment view through `attachment_view` (spec 058).
pub fn preview_bytes(q: &mut impl Query, attachment_id: &str) -> Result<(Vec<u8>, &'static str)> {
    let (name, data) = attachment_data(q, attachment_id)?;
    let extension = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase());
    let mime = match extension.as_deref() {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        _ => return Err(invalid("not_previewable")),
    };
    Ok((data, mime))
}

/// The binary rows that something uses: a link of an entry or a state, or, for an icon, a name
/// (`binary:<hash>`) in an entry, folder, passkey or state.
fn is_used(tx: &mut CrdtTransaction<'_>, hash: &str, kind: &str) -> Result<bool> {
    let linked = tx
        .query_row(
            "SELECT (SELECT COUNT(*) FROM haex_passwords_item_binaries WHERE binary_hash = ?1) \
                  + (SELECT COUNT(*) FROM haex_passwords_snapshot_binaries WHERE binary_hash = ?1)",
            params![hash],
            |r| r.get::<_, i64>(0),
        )?
        .unwrap_or(0);
    if linked > 0 {
        return Ok(true);
    }
    if kind != "icon" {
        return Ok(false);
    }
    let name = format!("binary:{hash}");
    let named = tx
        .query_row(
            "SELECT (SELECT COUNT(*) FROM haex_passwords_item_details WHERE icon = ?1) \
                  + (SELECT COUNT(*) FROM haex_passwords_groups WHERE icon = ?1) \
                  + (SELECT COUNT(*) FROM haex_passwords_passkeys WHERE icon = ?1) \
                  + (SELECT COUNT(*) FROM haex_passwords_item_snapshots \
                     WHERE instr(snapshot_data, ?1) > 0)",
            params![name],
            |r| r.get::<_, i64>(0),
        )?
        .unwrap_or(0);
    Ok(named > 0)
}

/// Cleans up binary data (FR-022), once when the vault opens: a binary that nothing uses gets the
/// mark `orphaned_at = now` at the first detection (an old row that only now loses its last use
/// keeps the grace period too), a binary that something uses again loses the mark, and a binary
/// that has been unused for seven days is deleted. Returns how many rows were deleted.
// ponytail: the search for an icon's name in the JSON of every history state is a text scan
// (ceiling: tens of thousands of states at open; upgrade path: a reference table for icons).
pub fn prune_binaries(tx: &mut CrdtTransaction<'_>, now_millis: i64) -> Result<u32> {
    let cutoff = clock::format_millis(now_millis - ORPHAN_GRACE_DAYS * 86_400_000);
    let now = clock::format_millis(now_millis);
    let rows: Vec<(String, String, Option<String>)> = tx.query_map(
        "SELECT hash, COALESCE(type, 'attachment'), orphaned_at FROM haex_passwords_binaries",
        &[],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    let mut deleted = 0;
    for (hash, kind, orphaned_at) in rows {
        let used = is_used(tx, &hash, &kind)?;
        match (used, orphaned_at) {
            (true, Some(_)) => {
                tx.execute(
                    "UPDATE haex_passwords_binaries SET orphaned_at = NULL WHERE hash = ?1",
                    params![hash],
                )?;
            }
            (false, None) => {
                tx.execute(
                    "UPDATE haex_passwords_binaries SET orphaned_at = ?1 WHERE hash = ?2",
                    params![now, hash],
                )?;
            }
            (false, Some(_)) => {
                let expired = tx
                    .query_row(
                        "SELECT COUNT(*) FROM haex_passwords_binaries \
                         WHERE hash = ?1 AND datetime(orphaned_at) <= datetime(?2)",
                        params![hash, cutoff],
                        |r| r.get::<_, i64>(0),
                    )?
                    .unwrap_or(0)
                    > 0;
                if expired {
                    tx.execute(
                        "DELETE FROM haex_passwords_binaries WHERE hash = ?1",
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

/// Starts the grace period of a binary that no attachment and no history state links any more
/// (`orphaned_at = now`, only if it is not marked yet). A binary that is still linked is left
/// alone.
pub fn mark_if_unreferenced(tx: &mut CrdtTransaction<'_>, hash: &str) -> Result<()> {
    let linked = tx
        .query_row(
            "SELECT (SELECT COUNT(*) FROM haex_passwords_item_binaries WHERE binary_hash = ?1) \
                  + (SELECT COUNT(*) FROM haex_passwords_snapshot_binaries WHERE binary_hash = ?1)",
            params![hash],
            |r| r.get::<_, i64>(0),
        )?
        .unwrap_or(0);
    if linked == 0 {
        tx.execute(
            "UPDATE haex_passwords_binaries SET orphaned_at = ?1 \
             WHERE hash = ?2 AND orphaned_at IS NULL",
            params![clock::now(), hash],
        )?;
    }
    Ok(())
}

/// Ends the grace period of a binary that got a link (again).
pub fn clear_orphan_mark(tx: &mut CrdtTransaction<'_>, hash: &str) -> Result<()> {
    tx.execute(
        "UPDATE haex_passwords_binaries SET orphaned_at = NULL \
         WHERE hash = ?1 AND orphaned_at IS NOT NULL",
        params![hash],
    )?;
    Ok(())
}
