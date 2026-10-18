//! Opening the vault file of haex-vault 2.x (Legacy) (spec 037, research R1, R2, R5, R10). The file and its
//! `-wal` are **copied** into a temporary directory and only the copy is opened: a reader of a WAL
//! database creates `-shm` next to it and may checkpoint into it on close, and `immutable=1` would
//! skip the changes still in the `-wal`. The copies stay encrypted with the vault password.

use std::collections::BTreeSet;
use std::io::Read;
use std::path::Path;

use haex_crdt::rusqlite::{Connection, ErrorCode, OpenFlags};

use super::super::{failed, Problem};
use super::corrupt;
use crate::error::Result;
use crate::passwords::model::AttentionKind;

/// The smallest SQLCipher file: one page of the default size.
const MIN_FILE_BYTES: u64 = 4096;
const PLAIN_SQLITE_HEADER: &[u8; 16] = b"SQLite format 3\0";

/// The columns haex-vault 2.x's sync layer adds to every table at run time; known and ignored.
const CRDT_COLUMNS: [&str; 3] = ["haex_hlc", "haex_column_hlcs", "haex_column_sigs"];

/// The tables of the password manager and their columns, exactly as haex-vault 2.x @ `8dce379`
/// creates them (`0000_jazzy_chat.sql:414-552`). A missing one means a layout the reader cannot
/// read; anything beyond these and the sync columns is reported.
pub(super) const COLUMNS: &[(&str, &[&str])] = &[
    (
        "haex_passwords_binaries",
        &["hash", "data", "size", "type", "created_at"],
    ),
    (
        "haex_passwords_generator_presets",
        &[
            "id",
            "name",
            "length",
            "uppercase",
            "lowercase",
            "numbers",
            "symbols",
            "exclude_chars",
            "use_pattern",
            "pattern",
            "is_default",
            "created_at",
            "updated_at",
        ],
    ),
    ("haex_passwords_group_items", &["item_id", "group_id"]),
    (
        "haex_passwords_groups",
        &[
            "id",
            "name",
            "description",
            "icon",
            "sort_order",
            "color",
            "parent_id",
            "created_at",
            "updated_at",
        ],
    ),
    (
        "haex_passwords_item_binaries",
        &["id", "item_id", "binary_hash", "file_name"],
    ),
    (
        "haex_passwords_item_details",
        &[
            "id",
            "title",
            "username",
            "password",
            "note",
            "icon",
            "color",
            "url",
            "otp_secret",
            "otp_digits",
            "otp_period",
            "otp_algorithm",
            "expires_at",
            "autofill_aliases",
            "created_at",
            "updated_at",
        ],
    ),
    (
        "haex_passwords_item_key_values",
        &["id", "item_id", "key", "value", "updated_at"],
    ),
    (
        "haex_passwords_item_snapshots",
        &[
            "id",
            "item_id",
            "snapshot_data",
            "created_at",
            "modified_at",
        ],
    ),
    ("haex_passwords_item_tags", &["id", "item_id", "tag_id"]),
    (
        "haex_passwords_passkeys",
        &[
            "id",
            "item_id",
            "credential_id",
            "relying_party_id",
            "relying_party_name",
            "user_handle",
            "user_name",
            "user_display_name",
            "private_key",
            "public_key",
            "algorithm",
            "sign_count",
            "is_discoverable",
            "icon",
            "color",
            "nickname",
            "created_at",
            "last_used_at",
        ],
    ),
    (
        "haex_passwords_snapshot_binaries",
        &["id", "snapshot_id", "binary_hash", "file_name"],
    ),
    (
        "haex_passwords_tags",
        &["id", "name", "color", "created_at"],
    ),
];

/// The opened copy. The directory with the copies goes when this is dropped, after the connection
/// (fields drop in declaration order).
pub(super) struct Source {
    pub conn: Connection,
    _dir: tempfile::TempDir,
}

/// What the check of the layout found besides the problems: whether the file holds tables of the
/// old haex-pass extension (named `<public key>__haex-pass__haex_passwords_…`).
pub(super) struct Layout {
    pub problems: Vec<Problem>,
    pub has_haex_pass_tables: bool,
}

/// Copies `path` (and `<path>-wal` when it is there) and opens the copy with `password`. Only reads
/// `path`; never creates, changes or deletes anything next to it.
pub(super) fn open(path: &Path, password: &str) -> Result<Source> {
    check_header(path)?;
    let dir = tempfile::tempdir().map_err(|_| failed("unreadable"))?;
    let copy = dir.path().join("vault.db");
    std::fs::copy(path, &copy).map_err(|_| failed("unreadable"))?;
    let mut wal = path.as_os_str().to_owned();
    wal.push("-wal");
    let wal = Path::new(&wal);
    if wal.is_file() {
        std::fs::copy(wal, dir.path().join("vault.db-wal")).map_err(|_| failed("unreadable"))?;
    }
    let conn = Connection::open_with_flags(
        &copy,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|_| failed("unreadable"))?;
    // ponytail: rusqlite renders the key into the text of a `PRAGMA key` statement that is not
    // zeroed, as holzi's own vault open does (haex-crdt). Ceiling: a plain copy of the vault
    // password in freed memory until it is reused. Upgrade path: `sqlite3_key_v2` through
    // `rusqlite::ffi` with a zeroizing buffer.
    conn.pragma_update(None, "key", password)
        .map_err(|_| failed("unreadable"))?;
    match conn.query_row("SELECT COUNT(*) FROM sqlite_master", [], |r| {
        r.get::<_, i64>(0)
    }) {
        Ok(_) => Ok(Source { conn, _dir: dir }),
        Err(error) if error.sqlite_error_code() == Some(ErrorCode::NotADatabase) => {
            Err(failed("legacy_haex_vault_locked"))
        }
        Err(error) => Err(corrupt(error)),
    }
}

/// Refuses what is recognisably not an encrypted vault before anything is copied: an empty or too
/// small file, or an unencrypted SQLite database (its header is plain text).
fn check_header(path: &Path) -> Result<()> {
    let metadata = std::fs::metadata(path).map_err(|_| failed("unreadable"))?;
    if !metadata.is_file() {
        return Err(failed("unreadable"));
    }
    if metadata.len() < MIN_FILE_BYTES {
        return Err(failed("unsupported_format"));
    }
    let mut head = [0u8; 16];
    std::fs::File::open(path)
        .and_then(|mut file| file.read_exact(&mut head))
        .map_err(|_| failed("unreadable"))?;
    if &head == PLAIN_SQLITE_HEADER {
        return Err(failed("unsupported_format"));
    }
    Ok(())
}

/// Checks the layout of the password manager (R5): no item table means no passwords, a missing
/// required table or column means a layout the reader cannot read; unknown columns and tables of
/// the password manager are reported, never dropped silently.
pub(super) fn check_layout(conn: &Connection) -> Result<Layout> {
    let tables = table_names(conn)?;
    if !tables.contains("haex_passwords_item_details") {
        return Err(failed("no_passwords"));
    }
    let mut problems = Vec::new();
    for (table, expected) in COLUMNS {
        if !tables.contains(*table) {
            return Err(failed("unsupported_format"));
        }
        let columns = column_names(conn, table)?;
        if expected.iter().any(|c| !columns.contains(*c)) {
            return Err(failed("unsupported_format"));
        }
        for column in &columns {
            let known =
                expected.contains(&column.as_str()) || CRDT_COLUMNS.contains(&column.as_str());
            if !known {
                problems.push(Problem::field(
                    AttentionKind::UnknownSourceData,
                    &format!("{table}.{column}"),
                ));
            }
        }
    }
    for table in &tables {
        let ours = table.starts_with("haex_passwords_");
        if ours && !COLUMNS.iter().any(|(name, _)| name == table) {
            problems.push(Problem::field(AttentionKind::UnknownSourceData, table));
        }
    }
    let has_haex_pass_tables = tables
        .iter()
        .any(|t| t.contains("__haex-pass__haex_passwords_"));
    Ok(Layout {
        problems,
        has_haex_pass_tables,
    })
}

fn table_names(conn: &Connection) -> Result<BTreeSet<String>> {
    let mut statement = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
        .map_err(corrupt)?;
    let names = statement
        .query_map([], |r| r.get::<_, String>(0))
        .and_then(Iterator::collect)
        .map_err(corrupt)?;
    Ok(names)
}

fn column_names(conn: &Connection, table: &str) -> Result<BTreeSet<String>> {
    let mut statement = conn
        .prepare("SELECT name FROM pragma_table_info(?1)")
        .map_err(corrupt)?;
    let names = statement
        .query_map([table], |r| r.get::<_, String>(0))
        .and_then(Iterator::collect)
        .map_err(corrupt)?;
    Ok(names)
}

#[cfg(test)]
#[path = "open_tests.rs"]
mod tests;
