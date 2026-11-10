//! The user's own voices (spec 066 FR-049, FR-050, data-model.md "Eigene Stimmen", T145): a row
//! in `haex_tts_voices` per voice, its reference recording once per content in
//! `haex_tts_voices_binaries` (key: SHA-256 of the WAV). Both tables sync, so a voice made on one
//! device speaks on every device of the vault. Deleting a voice removes its row everywhere; its
//! recording is marked and removed after [`ORPHAN_GRACE_DAYS`], so a row of another device that
//! has not arrived yet still finds it (as the address book does with photos).

use ::uuid::Uuid;
use haex_crdt::rusqlite::params;
use haex_crdt::CrdtTransaction;

use crate::passwords::binaries::hash_bytes;
use crate::passwords::clock;
use crate::storage::query::Query;
use crate::storage::uuid;

/// Days an unused recording is kept before it is deleted.
pub const ORPHAN_GRACE_DAYS: i64 = 7;
/// The engine every own voice is made for (research R20): Pocket imitates a reference recording.
pub const ENGINE_POCKET: &str = "pocket";

/// One own voice as stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnVoice {
    pub id: Uuid,
    pub name: String,
    /// ISO 639-1, one of the six languages haex speaks (FR-052).
    pub language: String,
    pub engine: String,
    pub reference_hash: String,
    pub sample_rate: i64,
    /// Unix ms when the user confirmed that it is their voice or the speaker consented (FR-050).
    pub consent_confirmed_at: i64,
    pub created_at: i64,
}

/// What a new voice needs; `id`, the hash and `created_at` are set by [`insert`].
#[derive(Debug, Clone)]
pub struct NewVoice<'a> {
    pub name: &'a str,
    pub language: &'a str,
    /// The reference recording: a 16-bit mono WAV.
    pub wav: &'a [u8],
    pub sample_rate: u32,
    pub consent_confirmed_at: i64,
}

/// Stores a new voice and its recording (or finds the recording stored; a mark of an unused one is
/// lifted).
pub fn insert(
    tx: &mut CrdtTransaction<'_>,
    voice: NewVoice<'_>,
    now: i64,
) -> haex_crdt::Result<OwnVoice> {
    let hash = hash_bytes(voice.wav);
    let known = tx
        .query_row(
            "SELECT COUNT(*) FROM haex_tts_voices_binaries WHERE hash = ?1",
            params![hash],
            |r| r.get::<_, i64>(0),
        )?
        .unwrap_or(0)
        > 0;
    if known {
        tx.execute(
            "UPDATE haex_tts_voices_binaries SET orphaned_at = NULL \
             WHERE hash = ?1 AND orphaned_at IS NOT NULL",
            params![hash],
        )?;
    } else {
        tx.execute(
            "INSERT INTO haex_tts_voices_binaries (hash, data) VALUES (?1, ?2)",
            params![hash, voice.wav],
        )?;
    }
    let stored = OwnVoice {
        id: Uuid::new_v4(),
        name: voice.name.to_owned(),
        language: voice.language.to_owned(),
        engine: ENGINE_POCKET.to_owned(),
        reference_hash: hash,
        sample_rate: i64::from(voice.sample_rate),
        consent_confirmed_at: voice.consent_confirmed_at,
        created_at: now,
    };
    tx.execute(
        "INSERT INTO haex_tts_voices \
           (id, name, language, engine, reference_hash, sample_rate, consent_confirmed_at, \
            created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            stored.id.to_string(),
            stored.name,
            stored.language,
            stored.engine,
            stored.reference_hash,
            stored.sample_rate,
            stored.consent_confirmed_at,
            stored.created_at,
        ],
    )?;
    Ok(stored)
}

/// Every own voice, by language and then name.
pub fn list(q: &mut impl Query) -> haex_crdt::Result<Vec<OwnVoice>> {
    q.query_map(
        "SELECT id, name, language, engine, reference_hash, sample_rate, consent_confirmed_at, \
                created_at \
         FROM haex_tts_voices ORDER BY language, name COLLATE NOCASE, id",
        &[],
        |row| {
            Ok(OwnVoice {
                id: uuid::from_row(row, 0)?,
                name: row.get(1)?,
                language: row.get(2)?,
                engine: row.get(3)?,
                reference_hash: row.get(4)?,
                sample_rate: row.get(5)?,
                consent_confirmed_at: row.get(6)?,
                created_at: row.get(7)?,
            })
        },
    )
}

/// The language and the recording (WAV) of the voice `id`; `None` when the voice or its recording
/// is not (yet) in this vault.
pub fn recording(q: &mut impl Query, id: &str) -> haex_crdt::Result<Option<(String, Vec<u8>)>> {
    q.query_row(
        "SELECT v.language, b.data FROM haex_tts_voices v \
         JOIN haex_tts_voices_binaries b ON b.hash = v.reference_hash \
         WHERE v.id = ?1",
        params![id],
        |r| Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?)),
    )
}

/// Deletes the voice `id` and marks its recording once no voice uses it any more. Returns whether
/// the voice existed.
pub fn delete(tx: &mut CrdtTransaction<'_>, id: Uuid) -> haex_crdt::Result<bool> {
    let hash: Option<String> = tx.query_row(
        "SELECT reference_hash FROM haex_tts_voices WHERE id = ?1",
        params![id.to_string()],
        |r| r.get(0),
    )?;
    let Some(hash) = hash else {
        return Ok(false);
    };
    tx.execute(
        "DELETE FROM haex_tts_voices WHERE id = ?1",
        params![id.to_string()],
    )?;
    if !is_used(tx, &hash)? {
        tx.execute(
            "UPDATE haex_tts_voices_binaries SET orphaned_at = ?1 \
             WHERE hash = ?2 AND orphaned_at IS NULL",
            params![clock::now(), hash],
        )?;
    }
    Ok(true)
}

fn is_used(q: &mut impl Query, hash: &str) -> haex_crdt::Result<bool> {
    Ok(q.query_row(
        "SELECT COUNT(*) FROM haex_tts_voices WHERE reference_hash = ?1",
        params![hash],
        |r| r.get::<_, i64>(0),
    )?
    .unwrap_or(0)
        > 0)
}

/// Cleans up recordings once when the vault opens: an unused one is marked, a used one loses its
/// mark, one unused for [`ORPHAN_GRACE_DAYS`] is deleted. Returns how many were deleted.
pub fn prune(tx: &mut CrdtTransaction<'_>, now_millis: i64) -> haex_crdt::Result<u32> {
    let cutoff = clock::format_millis(now_millis - ORPHAN_GRACE_DAYS * 86_400_000);
    let now = clock::format_millis(now_millis);
    let rows: Vec<(String, Option<String>)> = tx.query_map(
        "SELECT hash, orphaned_at FROM haex_tts_voices_binaries",
        &[],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let mut deleted = 0;
    for (hash, orphaned_at) in rows {
        match (is_used(tx, &hash)?, orphaned_at) {
            (true, Some(_)) => {
                tx.execute(
                    "UPDATE haex_tts_voices_binaries SET orphaned_at = NULL WHERE hash = ?1",
                    params![hash],
                )?;
            }
            (false, None) => {
                tx.execute(
                    "UPDATE haex_tts_voices_binaries SET orphaned_at = ?1 WHERE hash = ?2",
                    params![now, hash],
                )?;
            }
            (false, Some(_)) => {
                let expired = tx
                    .query_row(
                        "SELECT COUNT(*) FROM haex_tts_voices_binaries \
                         WHERE hash = ?1 AND datetime(orphaned_at) <= datetime(?2)",
                        params![hash, cutoff],
                        |r| r.get::<_, i64>(0),
                    )?
                    .unwrap_or(0)
                    > 0;
                if expired {
                    tx.execute(
                        "DELETE FROM haex_tts_voices_binaries WHERE hash = ?1",
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
