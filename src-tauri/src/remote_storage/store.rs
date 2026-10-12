//! The tables of storage connections (spec 038, data-model.md): rows read and written through the
//! CRDT write path, the field rules checked before a write (the synced tables carry no
//! constraints), and the removals that take the permissions of extensions with them (FR-007).
//!
//! Writes are check-then-write like the other CRDT tables (`storage/preferences.rs`): an `INSERT`
//! for a new id, an `UPDATE` for an existing one.

use std::collections::HashMap;

use haex_crdt::rusqlite::{params, Row};
use haex_crdt::CrdtTransaction;

use super::{
    Addressing, ConnectionRow, EndpointScope, Protocol, ProviderKind, StorageRow, TestOutcome,
};
use crate::error::{HolziError, Result};
use crate::extensions::permissions::store::TABLES as PERMISSION_TABLES;
use crate::extensions::permissions::PermissionKind;
use crate::storage::query::Query;

const CONNECTION_COLUMNS: &str = "id, provider_name, provider_kind, endpoint, endpoint_scope, \
     region, addressing, credentials_item_id, created_at, updated_at, protocol, host_key, \
     host_key_confirmed_at";

const STORAGE_COLUMNS: &str = "id, connection_id, name, bucket, created_at, updated_at";

/// Builds a validation error identifying the rejected row field.
fn invalid(field: &str) -> HolziError {
    HolziError::StorageInvalid {
        field: field.to_owned(),
    }
}

/// A stored text that is not one of the known values: the row is unreadable, not a guess.
fn unknown(column: usize) -> haex_crdt::rusqlite::Error {
    haex_crdt::rusqlite::Error::InvalidColumnType(
        column,
        "unknown value".to_owned(),
        haex_crdt::rusqlite::types::Type::Text,
    )
}

/// Decodes a connection row in `CONNECTION_COLUMNS` order, rejecting unknown enum values.
fn connection_of(r: &Row<'_>) -> haex_crdt::rusqlite::Result<ConnectionRow> {
    Ok(ConnectionRow {
        id: r.get(0)?,
        provider_name: r.get(1)?,
        provider_kind: ProviderKind::parse(&r.get::<_, String>(2)?).ok_or_else(|| unknown(2))?,
        endpoint: r.get(3)?,
        endpoint_scope: EndpointScope::parse(&r.get::<_, String>(4)?).ok_or_else(|| unknown(4))?,
        region: r.get(5)?,
        addressing: Addressing::parse(&r.get::<_, String>(6)?).ok_or_else(|| unknown(6))?,
        credentials_item_id: r.get(7)?,
        created_at: r.get(8)?,
        updated_at: r.get(9)?,
        protocol: Protocol::parse(&r.get::<_, String>(10)?).ok_or_else(|| unknown(10))?,
        host_key: r.get(11)?,
        host_key_confirmed_at: r.get(12)?,
    })
}

/// Decodes a storage row in `STORAGE_COLUMNS` order.
fn storage_of(r: &Row<'_>) -> haex_crdt::rusqlite::Result<StorageRow> {
    Ok(StorageRow {
        id: r.get(0)?,
        connection_id: r.get(1)?,
        name: r.get(2)?,
        bucket: r.get(3)?,
        created_at: r.get(4)?,
        updated_at: r.get(5)?,
    })
}

/// Every connection, by name.
pub fn connections(q: &mut impl Query) -> Result<Vec<ConnectionRow>> {
    Ok(q.query_map(
        &format!(
            "SELECT {CONNECTION_COLUMNS} FROM haex_storage_connections \
             ORDER BY provider_name COLLATE NOCASE, id"
        ),
        &[],
        connection_of,
    )?)
}

/// Loads the connection with `id`, or returns `None` when no row exists.
pub fn connection(q: &mut impl Query, id: &str) -> Result<Option<ConnectionRow>> {
    Ok(q.query_row(
        &format!("SELECT {CONNECTION_COLUMNS} FROM haex_storage_connections WHERE id = ?1"),
        &[&id],
        connection_of,
    )?)
}

/// Every storage, by name.
pub fn storages(q: &mut impl Query) -> Result<Vec<StorageRow>> {
    Ok(q.query_map(
        &format!("SELECT {STORAGE_COLUMNS} FROM haex_storages ORDER BY name COLLATE NOCASE, id"),
        &[],
        storage_of,
    )?)
}

/// Loads the storage with `id`, or returns `None` when no row exists.
pub fn storage(q: &mut impl Query, id: &str) -> Result<Option<StorageRow>> {
    Ok(q.query_row(
        &format!("SELECT {STORAGE_COLUMNS} FROM haex_storages WHERE id = ?1"),
        &[&id],
        storage_of,
    )?)
}

/// Returns the IDs of storages belonging to a connection for removal.
fn storages_of(q: &mut impl Query, connection_id: &str) -> Result<Vec<String>> {
    Ok(q.query_map(
        "SELECT id FROM haex_storages WHERE connection_id = ?1",
        &[&connection_id],
        |r| r.get(0),
    )?)
}

/// Checks the trimmed character count is between one and `max` and rejects control characters.
fn name_ok(text: &str, max: usize) -> bool {
    let length = text.trim().chars().count();
    (1..=max).contains(&length) && !text.chars().any(char::is_control)
}

/// A bucket name after the S3 rules: 3–63 characters of lower-case letters, digits, `.` and `-`,
/// starting and ending with a letter or digit, no `..`.
pub fn bucket_ok(bucket: &str) -> bool {
    let bytes = bucket.as_bytes();
    (3..=63).contains(&bytes.len())
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'-'))
        && bytes[0].is_ascii_alphanumeric()
        && bytes[bytes.len() - 1].is_ascii_alphanumeric()
        && !bucket.contains("..")
}

/// A folder of a WebDAV or SFTP storage (spec 050 data-model.md): empty for the root, otherwise
/// parts separated by `/`, none empty, `.` or `..`, none longer than 255 bytes, no control
/// characters (so no `/` at the start or the end).
pub fn folder_ok(folder: &str) -> bool {
    folder.is_empty()
        || folder.split('/').all(|part| {
            !part.is_empty()
                && part != "."
                && part != ".."
                && part.len() <= 255
                && !part.chars().any(char::is_control)
        })
}

/// The shape of a WebDAV endpoint (spec 050 data-model.md): `https`, or `http` (local only, which
/// [`super::address`] checks), with a trailing `/`, no user, query or fragment.
fn webdav_endpoint_ok(endpoint: &str) -> bool {
    // The parser adds the `/` of an empty path itself, so the text must carry it.
    endpoint.ends_with('/')
        && url::Url::parse(endpoint).is_ok_and(|url| {
            matches!(url.scheme(), "https" | "http")
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none()
                && url.path().ends_with('/')
        })
}

/// The shape of an SFTP endpoint (spec 050 data-model.md): `sftp://host` or `sftp://host:port`
/// (port 1–65535), without a path, user, query or fragment.
fn sftp_endpoint_ok(endpoint: &str) -> bool {
    url::Url::parse(endpoint).is_ok_and(|url| {
        url.scheme() == "sftp"
            && url.host_str().is_some()
            && url.port() != Some(0)
            && url.username().is_empty()
            && url.password().is_none()
            && matches!(url.path(), "" | "/")
            && url.query().is_none()
            && url.fragment().is_none()
    })
}

/// The field rules of a connection (038 and 050 data-model.md); the addresses of the endpoint are
/// checked by [`super::address`].
pub fn check_connection(row: &ConnectionRow) -> Result<()> {
    if !name_ok(&row.provider_name, 80) {
        return Err(invalid("providerName"));
    }
    if !row.protocol.fits(row.provider_kind) {
        return Err(invalid("providerKind"));
    }
    let endpoint = row.endpoint.trim();
    match row.protocol {
        Protocol::S3 => {
            if !name_ok(&row.region, 64) {
                return Err(invalid("region"));
            }
            if endpoint.is_empty() && row.provider_kind != ProviderKind::Aws {
                return Err(invalid("endpoint"));
            }
        }
        Protocol::WebDav => {
            if !webdav_endpoint_ok(endpoint) {
                return Err(invalid("endpoint"));
            }
        }
        Protocol::Sftp => {
            if !sftp_endpoint_ok(endpoint) {
                return Err(invalid("endpoint"));
            }
        }
    }
    if row.protocol != Protocol::S3
        && (!row.region.is_empty() || row.addressing != Addressing::Path)
    {
        return Err(invalid("region"));
    }
    // Only SFTP has a host key, and it is never saved without one (research R5).
    let host_key = row
        .host_key
        .as_deref()
        .is_some_and(|key| !key.trim().is_empty());
    if host_key != (row.protocol == Protocol::Sftp) {
        return Err(invalid("hostKey"));
    }
    if row.credentials_item_id.is_empty() {
        return Err(invalid("credentials"));
    }
    Ok(())
}

/// The field rules of a storage on a connection of `protocol` (data-model.md): a bucket for S3, a
/// folder for WebDAV and SFTP.
pub fn check_storage(row: &StorageRow, protocol: Protocol) -> Result<()> {
    if !name_ok(&row.name, 80) {
        return Err(invalid("name"));
    }
    if !target_ok(protocol, &row.bucket) {
        return Err(invalid(target_field(protocol)));
    }
    Ok(())
}

/// The name of the field of a bucket or folder in an error: `bucket` for S3, `folder` for WebDAV
/// and SFTP.
pub fn target_field(protocol: Protocol) -> &'static str {
    if protocol.is_tree() {
        "folder"
    } else {
        "bucket"
    }
}

/// Whether `bucket` is a valid bucket (S3) or folder (WebDAV, SFTP) on a connection of `protocol`.
pub fn target_ok(protocol: Protocol, bucket: &str) -> bool {
    match protocol {
        Protocol::S3 => bucket_ok(bucket),
        Protocol::WebDav | Protocol::Sftp => folder_ok(bucket),
    }
}

/// Writes `row`: a new id is inserted, a known one updated.
pub fn put_connection(tx: &mut CrdtTransaction<'_>, row: &ConnectionRow) -> Result<()> {
    check_connection(row)?;
    let stored = connection(tx, &row.id)?;
    // The protocol of a saved connection never changes (spec 050 FR-001).
    if stored
        .as_ref()
        .is_some_and(|stored| stored.protocol != row.protocol)
    {
        return Err(invalid("protocol"));
    }
    let exists = stored.is_some();
    let values = params![
        row.id,
        row.provider_name.trim(),
        row.provider_kind.as_str(),
        row.endpoint.trim(),
        row.endpoint_scope.as_str(),
        row.region.trim(),
        row.addressing.as_str(),
        row.credentials_item_id,
        row.created_at,
        row.updated_at,
        row.protocol.as_str(),
        row.host_key,
        row.host_key_confirmed_at,
    ];
    if exists {
        tx.execute(
            "UPDATE haex_storage_connections SET provider_name = ?2, provider_kind = ?3, \
             endpoint = ?4, endpoint_scope = ?5, region = ?6, addressing = ?7, \
             credentials_item_id = ?8, updated_at = ?10, host_key = ?12, \
             host_key_confirmed_at = ?13 WHERE id = ?1 AND protocol = ?11",
            values,
        )?;
    } else {
        tx.execute(
            &format!(
                "INSERT INTO haex_storage_connections ({CONNECTION_COLUMNS}) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)"
            ),
            values,
        )?;
    }
    Ok(())
}

/// Writes `row` on an existing connection: a new id is inserted, a known one updated (its
/// connection stays).
pub fn put_storage(tx: &mut CrdtTransaction<'_>, row: &StorageRow) -> Result<()> {
    let Some(connection) = connection(tx, &row.connection_id)? else {
        return Err(HolziError::StorageNotFound);
    };
    check_storage(row, connection.protocol)?;
    let values = params![
        row.id,
        row.connection_id,
        row.name.trim(),
        row.bucket,
        row.created_at,
        row.updated_at,
    ];
    if storage(tx, &row.id)?.is_some() {
        tx.execute(
            "UPDATE haex_storages SET name = ?3, bucket = ?4, updated_at = ?6 WHERE id = ?1",
            values,
        )?;
    } else {
        tx.execute(
            &format!(
                "INSERT INTO haex_storages ({STORAGE_COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6)"
            ),
            values,
        )?;
    }
    Ok(())
}

/// Removes a storage, its test result and every permission of an extension or an agent that names
/// it, in the one write of `tx` (FR-007, spec 044 R14). A permission for `*` stays: it names no
/// storage.
pub fn remove_storage(tx: &mut CrdtTransaction<'_>, id: &str) -> Result<()> {
    let kind = PermissionKind::RemoteStorage.as_str();
    for table in PERMISSION_TABLES {
        tx.execute(
            &format!("DELETE FROM {table} WHERE kind = ?1 AND target = ?2"),
            params![kind, id],
        )?;
    }
    crate::files::permissions::remove_storage(tx, id)?;
    crate::files::encrypted::cache::remove_storage(tx, id)?;
    tx.execute(
        "DELETE FROM storage_tests_no_sync WHERE storage_id = ?1",
        params![id],
    )?;
    tx.execute("DELETE FROM haex_storages WHERE id = ?1", params![id])?;
    Ok(())
}

/// Removes a connection with its storages as [`remove_storage`] does. Returns the id of its
/// credentials entry when no other connection uses it, so the caller deletes it for good.
pub fn remove_connection(tx: &mut CrdtTransaction<'_>, id: &str) -> Result<Option<String>> {
    let Some(row) = connection(tx, id)? else {
        return Err(HolziError::StorageNotFound);
    };
    for storage_id in storages_of(tx, id)? {
        remove_storage(tx, &storage_id)?;
    }
    tx.execute(
        "DELETE FROM haex_storage_connections WHERE id = ?1",
        params![id],
    )?;
    Ok((!credentials_in_use(tx, &row.credentials_item_id)?).then_some(row.credentials_item_id))
}

/// Whether a connection uses the credentials entry `item_id`.
pub fn credentials_in_use(q: &mut impl Query, item_id: &str) -> Result<bool> {
    Ok(q.query_row(
        "SELECT COUNT(*) FROM haex_storage_connections WHERE credentials_item_id = ?1",
        &[&item_id],
        |r| r.get::<_, i64>(0),
    )?
    .unwrap_or(0)
        > 0)
}

/// The names of the connections that use the credentials entry `item_id`.
pub fn connections_using(q: &mut impl Query, item_id: &str) -> Result<Vec<String>> {
    Ok(q.query_map(
        "SELECT provider_name FROM haex_storage_connections WHERE credentials_item_id = ?1 \
         ORDER BY provider_name COLLATE NOCASE",
        &[&item_id],
        |r| r.get(0),
    )?)
}

/// Remembers the last test result of a storage on this device.
pub fn record_test(
    tx: &mut CrdtTransaction<'_>,
    storage_id: &str,
    outcome: TestOutcome,
    at: &str,
) -> Result<()> {
    tx.execute(
        "DELETE FROM storage_tests_no_sync WHERE storage_id = ?1",
        params![storage_id],
    )?;
    tx.execute(
        "INSERT INTO storage_tests_no_sync (storage_id, tested_at, outcome) VALUES (?1, ?2, ?3)",
        params![storage_id, at, outcome.as_str()],
    )?;
    Ok(())
}

/// The last test result per storage on this device: `(tested_at, outcome)`.
pub fn tests(q: &mut impl Query) -> Result<HashMap<String, (String, TestOutcome)>> {
    let rows: Vec<(String, String, String)> = q.query_map(
        "SELECT storage_id, tested_at, outcome FROM storage_tests_no_sync",
        &[],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    Ok(rows
        .into_iter()
        .filter_map(|(id, at, outcome)| Some((id, (at, TestOutcome::parse(&outcome)?))))
        .collect())
}

/// The names of the extensions with a permission for this storage (by its id, not `*`): those
/// that lose it when the storage goes (FR-007).
pub fn extensions_of(q: &mut impl Query, storage_id: &str) -> Result<Vec<String>> {
    let kind = PermissionKind::RemoteStorage.as_str();
    Ok(q.query_map(
        "SELECT name FROM ( \
           SELECT COALESCE(e.display_name, e.name) AS name FROM extension_permissions p \
             JOIN extensions e ON e.id = p.extension_id \
             WHERE p.kind = ?1 AND p.target = ?2 AND p.status = 'granted' \
           UNION \
           SELECT COALESCE(d.display_name, d.name) FROM dev_extension_permissions_no_sync p \
             JOIN dev_extensions_no_sync d ON d.id = p.extension_id \
             WHERE p.kind = ?1 AND p.target = ?2 AND p.status = 'granted') \
         ORDER BY name COLLATE NOCASE",
        &[&kind, &storage_id],
        |r| r.get(0),
    )?)
}
