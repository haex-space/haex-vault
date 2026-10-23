//! The rows of file shares and their grants (spec 056 data-model.md), read and written through the
//! CRDT write path. Ids derive from what makes a row unique, so every device computes the same id
//! and no UNIQUE constraint is needed. Every change to a share or its grants moves `changed_at`;
//! the serving device answers with `served_at` (research R10).

use haex_crdt::rusqlite::{params, Row};
use haex_crdt::CrdtTransaction;
use uuid::Uuid;

use super::{source_of, source_text, FileShareItemKind, FileShareLevel, FileShareSourceState};
use crate::error::{HolziError, Result};
use crate::files::SourceRef;
use crate::storage::query::Query;

/// Namespace of share ids: `UUIDv5(NS, device "\n" source "\n" path)`.
const NS_FILE_SHARES: Uuid = Uuid::from_u128(0x3d6f_0a91_5b2e_4c87_8e14_a9f2_60b3_c7d5);
/// Namespace of grant ids: `UUIDv5(NS, share "\n" vault)`.
const NS_FILE_SHARE_GRANTS: Uuid = Uuid::from_u128(0x91c4_7e20_d35a_4f6b_b2a8_0c5e_1f97_436d);

const SHARE_COLUMNS: &str = "id, vault_device_uuid, source, path, item_kind, name, created_at, \
                             changed_at, served_at, source_state";

/// Longest share name, in characters.
pub const MAX_NAME_CHARS: usize = 255;

/// A file share as stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileShareRow {
    pub id: String,
    /// The serving device (FR-018).
    pub vault_device_uuid: String,
    pub source: SourceRef,
    /// Window path of the root in the source, starting with `/`.
    pub path: String,
    pub item_kind: FileShareItemKind,
    pub name: String,
    pub created_at: String,
    pub changed_at: String,
    pub served_at: Option<String>,
    pub source_state: FileShareSourceState,
}

impl FileShareRow {
    /// Whether the serving device has not seen the latest change yet (FR-026).
    pub fn is_pending(&self) -> bool {
        self.served_at
            .as_deref()
            .is_none_or(|served| served < self.changed_at.as_str())
    }
}

/// A grant of a share to a contact vault.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileShareGrantRow {
    pub share_id: String,
    /// x-only public key of the contact's vault, 64 lowercase hex characters.
    pub vault_pubkey: String,
    pub level: FileShareLevel,
    pub updated_at: String,
}

/// What a new share needs; the serving device is the one creating it (FR-015, FR-018).
#[derive(Debug, Clone)]
pub struct NewFileShare<'a> {
    pub vault_device_uuid: &'a str,
    pub source: &'a SourceRef,
    pub path: &'a str,
    pub item_kind: FileShareItemKind,
    pub name: &'a str,
}

/// The id of the share of `path` in `source` served by `device`: the same on every device.
pub fn share_id(device: &str, source: &str, path: &str) -> String {
    Uuid::new_v5(
        &NS_FILE_SHARES,
        format!("{device}\n{source}\n{path}").as_bytes(),
    )
    .to_string()
}

/// The id of the grant of `share` to `vault_pubkey`.
pub fn grant_id(share: &str, vault_pubkey: &str) -> String {
    Uuid::new_v5(
        &NS_FILE_SHARE_GRANTS,
        format!("{share}\n{vault_pubkey}").as_bytes(),
    )
    .to_string()
}

/// The share `id`, or `None`.
pub fn get_share(q: &mut impl Query, id: &str) -> Result<Option<FileShareRow>> {
    Ok(q.query_row(
        &format!("SELECT {SHARE_COLUMNS} FROM haex_file_shares WHERE id = ?1"),
        &[&id],
        share_of_row,
    )?
    .flatten())
}

/// The share of `path` in `source` served by `device`, if one exists (FR-017).
pub fn share_at(
    q: &mut impl Query,
    device: &str,
    source: &SourceRef,
    path: &str,
) -> Result<Option<FileShareRow>> {
    let Some(source) = source_text(source) else {
        return Ok(None);
    };
    get_share(q, &share_id(device, &source, path))
}

/// Every share of the vault, on whichever device (FR-020).
pub fn all_shares(q: &mut impl Query) -> Result<Vec<FileShareRow>> {
    Ok(q.query_map(
        &format!("SELECT {SHARE_COLUMNS} FROM haex_file_shares ORDER BY name, id"),
        &[],
        share_of_row,
    )?
    .into_iter()
    .flatten()
    .collect())
}

/// The shares `device` serves.
pub fn shares_served_by(q: &mut impl Query, device: &str) -> Result<Vec<FileShareRow>> {
    Ok(q.query_map(
        &format!(
            "SELECT {SHARE_COLUMNS} FROM haex_file_shares WHERE vault_device_uuid = ?1 \
             ORDER BY name, id"
        ),
        &[&device],
        share_of_row,
    )?
    .into_iter()
    .flatten()
    .collect())
}

/// The shares `device` serves to `vault_pubkey`, each with its level (FR-004, the `Shares`
/// request of contracts/files-protocol.md).
pub fn shares_for_vault(
    q: &mut impl Query,
    device: &str,
    vault_pubkey: &str,
) -> Result<Vec<(FileShareRow, FileShareLevel)>> {
    let columns = SHARE_COLUMNS
        .split(", ")
        .map(|column| format!("s.{}", column.trim()))
        .collect::<Vec<_>>()
        .join(", ");
    Ok(q.query_map(
        &format!(
            "SELECT {columns}, g.level FROM haex_file_shares s \
             JOIN haex_file_share_grants g ON g.share_id = s.id \
             WHERE s.vault_device_uuid = ?1 AND g.vault_pubkey = ?2 ORDER BY s.name, s.id"
        ),
        &[&device, &vault_pubkey],
        |r| {
            let share = share_of_row(r)?;
            let level = FileShareLevel::parse(&r.get::<_, String>(10)?);
            Ok(share.zip(level))
        },
    )?
    .into_iter()
    .flatten()
    .collect())
}

/// The grants of `share`.
pub fn grants_of(q: &mut impl Query, share: &str) -> Result<Vec<FileShareGrantRow>> {
    Ok(q.query_map(
        "SELECT share_id, vault_pubkey, level, updated_at FROM haex_file_share_grants \
         WHERE share_id = ?1 ORDER BY vault_pubkey",
        &[&share],
        grant_of_row,
    )?
    .into_iter()
    .flatten()
    .collect())
}

/// The level `vault_pubkey` has in `share`, or `None` (FR-005: read on every request).
pub fn grant_for(
    q: &mut impl Query,
    share: &str,
    vault_pubkey: &str,
) -> Result<Option<FileShareLevel>> {
    Ok(q.query_row(
        "SELECT level FROM haex_file_share_grants WHERE id = ?1",
        &[&grant_id(share, vault_pubkey)],
        |r| r.get::<_, String>(0),
    )?
    .and_then(|text| FileShareLevel::parse(&text)))
}

/// Creates the share, or renames the one that already exists for the same source and device
/// (FR-017); returns its id.
pub fn create_share(
    tx: &mut CrdtTransaction<'_>,
    new: &NewFileShare<'_>,
    now: &str,
) -> Result<String> {
    let source = source_text(new.source).ok_or_else(|| invalid("not a source of this device"))?;
    check_path(new.path)?;
    check_name(new.name)?;
    let id = share_id(new.vault_device_uuid, &source, new.path);
    if get_share(tx, &id)?.is_some() {
        rename_share(tx, &id, new.name, now)?;
        return Ok(id);
    }
    tx.execute(
        &format!(
            "INSERT INTO haex_file_shares ({SHARE_COLUMNS}) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7, NULL, 'ok')"
        ),
        params![
            id,
            new.vault_device_uuid,
            source,
            new.path,
            new.item_kind.as_str(),
            new.name,
            now
        ],
    )?;
    Ok(id)
}

/// Renames `share`; any own device may (FR-019).
pub fn rename_share(
    tx: &mut CrdtTransaction<'_>,
    share: &str,
    name: &str,
    now: &str,
) -> Result<()> {
    check_name(name)?;
    let changed = tx.execute(
        "UPDATE haex_file_shares SET name = ?2, changed_at = ?3 WHERE id = ?1",
        params![share, name, now],
    )?;
    if changed == 0 {
        return Err(invalid("no such share"));
    }
    Ok(())
}

/// Gives `vault_pubkey` `level` in `share`, or changes its level (FR-016, FR-019).
pub fn save_grant(
    tx: &mut CrdtTransaction<'_>,
    share: &str,
    vault_pubkey: &str,
    level: FileShareLevel,
    now: &str,
) -> Result<()> {
    check_vault(vault_pubkey)?;
    let id = grant_id(share, vault_pubkey);
    let exists = tx
        .query_row(
            "SELECT 1 FROM haex_file_share_grants WHERE id = ?1",
            &[&id],
            |r| r.get::<_, i64>(0),
        )?
        .is_some();
    if exists {
        tx.execute(
            "UPDATE haex_file_share_grants SET level = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, level.as_str(), now],
        )?;
    } else {
        tx.execute(
            "INSERT INTO haex_file_share_grants (id, share_id, vault_pubkey, level, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, share, vault_pubkey, level.as_str(), now],
        )?;
    }
    touch(tx, share, now)
}

/// Takes `vault_pubkey`'s grant in `share` away (FR-024, FR-025): nothing to rekey.
pub fn delete_grant(
    tx: &mut CrdtTransaction<'_>,
    share: &str,
    vault_pubkey: &str,
    now: &str,
) -> Result<()> {
    tx.execute(
        "DELETE FROM haex_file_share_grants WHERE id = ?1",
        params![grant_id(share, vault_pubkey)],
    )?;
    touch(tx, share, now)
}

/// Deletes `share` with its grants.
pub fn delete_share(tx: &mut CrdtTransaction<'_>, share: &str) -> Result<()> {
    tx.execute(
        "DELETE FROM haex_file_share_grants WHERE share_id = ?1",
        params![share],
    )?;
    tx.execute("DELETE FROM haex_file_shares WHERE id = ?1", params![share])?;
    Ok(())
}

/// The serving device has seen every change up to `changed_at` (research R10).
pub fn set_served(tx: &mut CrdtTransaction<'_>, share: &str, changed_at: &str) -> Result<()> {
    tx.execute(
        "UPDATE haex_file_shares SET served_at = ?2 WHERE id = ?1",
        params![share, changed_at],
    )?;
    Ok(())
}

/// What the serving device found at the root of `share` (FR-022).
pub fn set_source_state(
    tx: &mut CrdtTransaction<'_>,
    share: &str,
    state: FileShareSourceState,
) -> Result<()> {
    tx.execute(
        "UPDATE haex_file_shares SET source_state = ?2 WHERE id = ?1 AND source_state != ?2",
        params![share, state.as_str()],
    )?;
    Ok(())
}

fn touch(tx: &mut CrdtTransaction<'_>, share: &str, now: &str) -> Result<()> {
    let changed = tx.execute(
        "UPDATE haex_file_shares SET changed_at = ?2 WHERE id = ?1",
        params![share, now],
    )?;
    if changed == 0 {
        return Err(invalid("no such share"));
    }
    Ok(())
}

/// A row with a source, kind or state holzi does not know (written by a newer version) is skipped.
fn share_of_row(r: &Row<'_>) -> haex_crdt::rusqlite::Result<Option<FileShareRow>> {
    let (Some(source), Some(item_kind)) = (
        source_of(&r.get::<_, String>(2)?),
        FileShareItemKind::parse(&r.get::<_, String>(4)?),
    ) else {
        return Ok(None);
    };
    Ok(Some(FileShareRow {
        id: r.get(0)?,
        vault_device_uuid: r.get(1)?,
        source,
        path: r.get(3)?,
        item_kind,
        name: r.get(5)?,
        created_at: r.get(6)?,
        changed_at: r.get(7)?,
        served_at: r.get(8)?,
        source_state: FileShareSourceState::parse(&r.get::<_, String>(9)?),
    }))
}

fn grant_of_row(r: &Row<'_>) -> haex_crdt::rusqlite::Result<Option<FileShareGrantRow>> {
    let Some(level) = FileShareLevel::parse(&r.get::<_, String>(2)?) else {
        return Ok(None);
    };
    Ok(Some(FileShareGrantRow {
        share_id: r.get(0)?,
        vault_pubkey: r.get(1)?,
        level,
        updated_at: r.get(3)?,
    }))
}

fn check_path(path: &str) -> Result<()> {
    if !path.starts_with('/') || path.contains('\0') {
        return Err(invalid("a share path starts with /"));
    }
    Ok(())
}

fn check_name(name: &str) -> Result<()> {
    let chars = name.trim().chars().count();
    if chars == 0 || chars > MAX_NAME_CHARS {
        return Err(invalid("a share name has 1 to 255 characters"));
    }
    Ok(())
}

/// A vault identity as stored: 64 lowercase hex characters.
pub fn check_vault(vault_pubkey: &str) -> Result<()> {
    let ok = vault_pubkey.len() == 64
        && vault_pubkey
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    if !ok {
        return Err(invalid("not a vault identity"));
    }
    Ok(())
}

fn invalid(reason: &str) -> HolziError {
    HolziError::InvalidInput {
        reason: reason.to_owned(),
    }
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
