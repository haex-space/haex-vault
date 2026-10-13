//! What this device has read of encrypted folders (data-model.md, research R6, R8): heads by their
//! object key and sidecars by their `sid`, each with the ETag it had, in the two `_no_sync` tables of
//! migration `0030`. A row whose ETag still matches saves the request; everything here can be
//! rebuilt from the bucket. Folder keys and file keys never land here. `open_ack` is a device
//! preference, not cache: a reload keeps it, and it moves with the folder to a new object key.

use haex_crdt::rusqlite::params;
use haex_crdt::CrdtTransaction;

use super::format::sidecar::{FileFacts, Sidecar, SidecarKind, WrappedDek};
use super::format::{decode_id, encode_id, Id};
use crate::files::EncryptedState;
use crate::storage::query::Query;

/// A head as this device read it last.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadRow {
    pub object_key: String,
    pub etag: String,
    /// Base32 of `fid`.
    pub folder_id: String,
    /// Only when `readable`.
    pub name: Option<String>,
    pub state: EncryptedState,
    /// The generation the folder key was unwrapped with.
    pub key_id: Option<Id>,
    pub open_ack: bool,
}

/// One sidecar as this device read it last.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryRow {
    pub sid: Id,
    pub etag: String,
    pub sidecar: Sidecar,
}

fn state_text(state: EncryptedState) -> &'static str {
    match state {
        EncryptedState::Readable => "readable",
        EncryptedState::OtherVault => "otherVault",
        EncryptedState::NewerFormat => "newerFormat",
        EncryptedState::Damaged => "damaged",
    }
}

fn parse_state(text: &str) -> Option<EncryptedState> {
    Some(match text {
        "readable" => EncryptedState::Readable,
        "otherVault" => EncryptedState::OtherVault,
        "newerFormat" => EncryptedState::NewerFormat,
        "damaged" => EncryptedState::Damaged,
        _ => return None,
    })
}

type RawHead = (
    String,
    String,
    String,
    Option<String>,
    String,
    Option<Vec<u8>>,
    i64,
);

/// The heads cached for the encrypted folders right below `parent` (a prefix, `""` at the root).
pub fn heads_under(
    q: &mut impl Query,
    storage_id: &str,
    parent: &str,
) -> haex_crdt::Result<Vec<HeadRow>> {
    let rows: Vec<RawHead> = q.query_map(
        "SELECT object_key, etag, folder_id, name, state, key_id, open_ack \
             FROM encrypted_folder_heads_no_sync \
             WHERE storage_id = ?1 AND substr(object_key, 1, length(?2)) = ?2",
        params![storage_id, parent],
        |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
            ))
        },
    )?;
    Ok(rows
        .into_iter()
        .filter(|(key, ..)| !key[parent.len()..].trim_end_matches("/h").contains('/'))
        .filter_map(|(object_key, etag, folder_id, name, state, key_id, ack)| {
            Some(HeadRow {
                object_key,
                etag,
                folder_id,
                name,
                state: parse_state(&state)?,
                key_id: key_id.and_then(|id| id.try_into().ok()),
                open_ack: ack != 0,
            })
        })
        .collect())
}

/// Writes `row` as read at `loaded_at`. A new object key of a known folder takes the folder's
/// `open_ack` along (the folder moved); an existing row keeps its own.
pub fn put_head(
    tx: &mut CrdtTransaction<'_>,
    storage_id: &str,
    row: &HeadRow,
    loaded_at: i64,
) -> haex_crdt::Result<()> {
    tx.execute(
        "INSERT INTO encrypted_folder_heads_no_sync \
         (storage_id, object_key, etag, folder_id, name, state, key_id, open_ack, loaded_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, \
                 COALESCE((SELECT MAX(open_ack) FROM encrypted_folder_heads_no_sync \
                           WHERE storage_id = ?1 AND folder_id = ?4), 0), ?8) \
         ON CONFLICT (storage_id, object_key) DO UPDATE SET etag = excluded.etag, \
           folder_id = excluded.folder_id, name = excluded.name, state = excluded.state, \
           key_id = excluded.key_id, loaded_at = excluded.loaded_at",
        params![
            storage_id,
            row.object_key,
            row.etag,
            row.folder_id,
            row.name,
            state_text(row.state),
            row.key_id.as_ref().map(|id| id.to_vec()),
            loaded_at
        ],
    )?;
    Ok(())
}

/// Whether "Nicht mehr fragen" was chosen on this device for opening files of folder `folder_id`
/// with the system's app (FR-028).
pub fn open_ack(q: &mut impl Query, storage_id: &str, folder_id: &str) -> haex_crdt::Result<bool> {
    let acks: Vec<i64> = q.query_map(
        "SELECT open_ack FROM encrypted_folder_heads_no_sync \
         WHERE storage_id = ?1 AND folder_id = ?2",
        params![storage_id, folder_id],
        |r| r.get(0),
    )?;
    Ok(acks.iter().any(|ack| *ack != 0))
}

/// The object keys of the heads this device read for folder `folder_id` (normally one; a folder
/// copied within the storage keeps its id), and the decrypted names of the readable ones.
pub fn heads_of_folder(
    q: &mut impl Query,
    storage_id: &str,
    folder_id: &str,
) -> haex_crdt::Result<Vec<(String, Option<String>)>> {
    q.query_map(
        "SELECT object_key, CASE WHEN state = 'readable' THEN name END          FROM encrypted_folder_heads_no_sync WHERE storage_id = ?1 AND folder_id = ?2          ORDER BY object_key",
        params![storage_id, folder_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
}

/// The object keys of every head cached for `storage_id`, at any depth (the device's folders,
/// spec 048 US7).
pub fn head_keys(q: &mut impl Query, storage_id: &str) -> haex_crdt::Result<Vec<String>> {
    q.query_map(
        "SELECT object_key FROM encrypted_folder_heads_no_sync WHERE storage_id = ?1 \
             ORDER BY object_key",
        params![storage_id],
        |r| r.get(0),
    )
}

/// Notes "Nicht mehr fragen" for folder `folder_id` at the head `object_key`. A folder this device
/// created has no row before a listing read its head: the row starts without an ETag then, so the
/// next listing reads the head and fills it in, keeping the answer.
pub fn set_open_ack(
    tx: &mut CrdtTransaction<'_>,
    storage_id: &str,
    folder_id: &str,
    object_key: &str,
) -> haex_crdt::Result<()> {
    let updated = tx.execute(
        "UPDATE encrypted_folder_heads_no_sync SET open_ack = 1 \
         WHERE storage_id = ?1 AND folder_id = ?2",
        params![storage_id, folder_id],
    )?;
    if updated == 0 {
        tx.execute(
            "INSERT INTO encrypted_folder_heads_no_sync \
             (storage_id, object_key, etag, folder_id, name, state, key_id, open_ack, loaded_at) \
             VALUES (?1, ?2, '', ?3, NULL, 'readable', NULL, 1, 0)",
            params![storage_id, object_key, folder_id],
        )?;
    }
    Ok(())
}

/// Drops the heads at `object_keys` (their folders are gone from the listing).
pub fn drop_heads(
    tx: &mut CrdtTransaction<'_>,
    storage_id: &str,
    object_keys: &[String],
) -> haex_crdt::Result<()> {
    for key in object_keys {
        tx.execute(
            "DELETE FROM encrypted_folder_heads_no_sync WHERE storage_id = ?1 AND object_key = ?2",
            params![storage_id, key],
        )?;
    }
    Ok(())
}

type RawEntry = (
    String,
    String,
    String,
    String,
    String,
    String,
    i64,
    Option<String>,
    i64,
    (Option<i64>, Option<i64>, Option<String>, Option<String>),
    (Option<String>, Option<String>, Option<Vec<u8>>),
);

/// The sidecars cached for folder `folder_id`; a row that no longer parses is left out, so it is
/// read again.
pub fn entries(
    q: &mut impl Query,
    storage_id: &str,
    folder_id: &str,
) -> haex_crdt::Result<Vec<EntryRow>> {
    let rows: Vec<RawEntry> = q.query_map(
        "SELECT sidecar, etag, entry_id, parent_id, name, kind, revision, base, written, \
                size, modified, content_type, sha256, object_id, content_id, wrapped_dek \
         FROM encrypted_folder_entries_no_sync WHERE storage_id = ?1 AND folder_id = ?2",
        params![storage_id, folder_id],
        |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
                r.get(7)?,
                r.get(8)?,
                (r.get(9)?, r.get(10)?, r.get(11)?, r.get(12)?),
                (r.get(13)?, r.get(14)?, r.get(15)?),
            ))
        },
    )?;
    Ok(rows.into_iter().filter_map(entry_of).collect())
}

fn entry_of(raw: RawEntry) -> Option<EntryRow> {
    let (sid, etag, entry, parent, name, kind, revision, base, written, file, ids) = raw;
    let kind = match kind.as_str() {
        "folder" => SidecarKind::Folder,
        "file" => {
            let (size, modified, content_type, sha256) = file;
            let (content, cid, dek) = ids;
            let sha = data_encoding::HEXLOWER
                .decode(sha256?.as_bytes())
                .ok()?
                .try_into()
                .ok()?;
            SidecarKind::File(FileFacts {
                size: u64::try_from(size?).ok()?,
                modified: u64::try_from(modified?).ok()?,
                content_type,
                sha256: sha,
                content: decode_id(&content?)?,
                cid: decode_id(&cid?)?,
                dek: WrappedDek::from_bytes(&dek?)?,
            })
        }
        _ => return None,
    };
    Some(EntryRow {
        sid: decode_id(&sid)?,
        etag,
        sidecar: Sidecar {
            entry: decode_id(&entry)?,
            parent: decode_id(&parent)?,
            name,
            revision: u64::try_from(revision).ok()?,
            base: match base {
                Some(base) => Some(decode_id(&base)?),
                None => None,
            },
            written: u64::try_from(written).ok()?,
            kind,
        },
    })
}

/// Writes `rows` for folder `folder_id`, replacing rows of the same `sid`.
pub fn put_entries(
    tx: &mut CrdtTransaction<'_>,
    storage_id: &str,
    folder_id: &str,
    rows: &[EntryRow],
) -> haex_crdt::Result<()> {
    for row in rows {
        let s = &row.sidecar;
        let file = match &s.kind {
            SidecarKind::File(facts) => Some(facts),
            SidecarKind::Folder => None,
        };
        let int = |value: u64| i64::try_from(value).unwrap_or(i64::MAX);
        tx.execute(
            "INSERT OR REPLACE INTO encrypted_folder_entries_no_sync \
             (storage_id, folder_id, sidecar, etag, entry_id, parent_id, name, kind, revision, \
              base, written, size, modified, content_type, sha256, object_id, content_id, \
              wrapped_dek) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, \
                     ?18)",
            params![
                storage_id,
                folder_id,
                encode_id(&row.sid),
                row.etag,
                encode_id(&s.entry),
                encode_id(&s.parent),
                s.name,
                if file.is_some() { "file" } else { "folder" },
                int(s.revision),
                s.base.as_ref().map(encode_id),
                int(s.written),
                file.map(|f| int(f.size)),
                file.map(|f| int(f.modified)),
                file.and_then(|f| f.content_type.clone()),
                file.map(|f| data_encoding::HEXLOWER.encode(&f.sha256)),
                file.map(|f| encode_id(&f.content)),
                file.map(|f| encode_id(&f.cid)),
                file.map(|f| f.dek.as_bytes().to_vec()),
            ],
        )?;
    }
    Ok(())
}

/// Drops the sidecars `sids` of folder `folder_id` (gone from the bucket).
pub fn drop_entries(
    tx: &mut CrdtTransaction<'_>,
    storage_id: &str,
    folder_id: &str,
    sids: &[Id],
) -> haex_crdt::Result<()> {
    for sid in sids {
        tx.execute(
            "DELETE FROM encrypted_folder_entries_no_sync \
             WHERE storage_id = ?1 AND folder_id = ?2 AND sidecar = ?3",
            params![storage_id, folder_id, encode_id(sid)],
        )?;
    }
    Ok(())
}

/// Drops the head and the sidecars cached for the folder `folder_id` (it was deleted).
pub fn drop_folder(
    tx: &mut CrdtTransaction<'_>,
    storage_id: &str,
    folder_id: &str,
) -> haex_crdt::Result<()> {
    for table in [
        "encrypted_folder_heads_no_sync",
        "encrypted_folder_entries_no_sync",
    ] {
        tx.execute(
            &format!("DELETE FROM {table} WHERE storage_id = ?1 AND folder_id = ?2"),
            params![storage_id, folder_id],
        )?;
    }
    Ok(())
}

/// Drops everything cached for the storage `storage_id` (it is being removed).
pub fn remove_storage(tx: &mut CrdtTransaction<'_>, storage_id: &str) -> haex_crdt::Result<()> {
    for table in [
        "encrypted_folder_heads_no_sync",
        "encrypted_folder_entries_no_sync",
    ] {
        tx.execute(
            &format!("DELETE FROM {table} WHERE storage_id = ?1"),
            params![storage_id],
        )?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "cache_tests.rs"]
mod tests;
