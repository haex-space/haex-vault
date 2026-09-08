//! Shared fixtures for the space-scoped scanner tests, plus the test-only
//! unscoped single-table scan.
//!
//! Mirrors the `inbound_sync_tests/helpers.rs` convention: schema setup and
//! row seeding live here, the subject files hold only assertions.

use crate::crdt::space_scanner::{scan_table_for_local_changes_scoped, LocalColumnChange};
use crate::database::error::DatabaseError;
use rusqlite::Connection;

/// Test-only helper: unscoped single-table scan. Production code must use
/// `scan_table_for_local_changes_scoped` (or the space-scoped whitelist
/// entry point `scan_space_scoped_tables_for_local_changes`) — an unscoped
/// scan over a table shared by multiple spaces leaks cross-space rows.
pub(super) fn scan_table_for_local_changes(
    conn: &Connection,
    table_name: &str,
    after_hlc: Option<&str>,
    device_id: &str,
) -> Result<Vec<LocalColumnChange>, DatabaseError> {
    scan_table_for_local_changes_scoped(conn, table_name, after_hlc, device_id, None, None)
}

/// Helper: create an in-memory DB with a CRDT-enabled table and return the connection.
pub(super) fn setup_test_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE test_items (
                id TEXT PRIMARY KEY,
                name TEXT,
                value INTEGER,
                haex_hlc_no_trigger TEXT,
                haex_column_hlcs_no_trigger TEXT NOT NULL DEFAULT '{}'
            );",
    )
    .unwrap();
    conn
}

pub(super) fn insert_row(conn: &Connection, id: &str, name: &str, value: i64, hlc: &str) {
    let hlcs = format!("{{\"name\":\"{hlc}\",\"value\":\"{hlc}\"}}");
    conn.execute(
        "INSERT INTO test_items (id, name, value, haex_hlc_no_trigger, haex_column_hlcs_no_trigger)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![id, name, value, hlc, hlcs],
    )
    .unwrap();
}

/// Creates a CRDT table that carries a `space_id` discriminator, used to
/// exercise the scoped-filter path.
pub(super) fn setup_scoped_test_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE scoped_items (
                id TEXT PRIMARY KEY,
                space_id TEXT NOT NULL,
                data TEXT,
                haex_hlc_no_trigger TEXT,
                haex_column_hlcs_no_trigger TEXT NOT NULL DEFAULT '{}',
                haex_column_sigs_no_trigger TEXT NOT NULL DEFAULT '{}'
            );",
    )
    .unwrap();
    conn
}

pub(super) fn insert_scoped_row(
    conn: &Connection,
    id: &str,
    space_id: &str,
    data: &str,
    hlc: &str,
) {
    let hlcs = format!("{{\"space_id\":\"{hlc}\",\"data\":\"{hlc}\"}}");
    let sigs = serde_json::json!({
        "space_id": {
            (space_id): {
                "authorDid": "did:key:test",
                "sig": "",
                "storageClass": "text",
            }
        },
        "data": {
            (space_id): {
                "authorDid": "did:key:test",
                "sig": "",
                "storageClass": "text",
            }
        },
    })
    .to_string();
    conn.execute(
        "INSERT INTO scoped_items
             (id, space_id, data, haex_hlc_no_trigger, haex_column_hlcs_no_trigger, haex_column_sigs_no_trigger)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![id, space_id, data, hlc, hlcs, sigs],
    )
    .unwrap();
}

/// Creates a "vault-private-like" CRDT table that is NOT in
/// [`SPACE_SCOPED_CRDT_TABLES`] and carries no `space_id` column — the
/// shape of a per-vault private table (e.g. passwords). Used to prove the
/// owner scanner ships such tables, which a space-scoped scan never would.
pub(super) fn setup_vault_private_test_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE haex_passwords (
                id TEXT PRIMARY KEY,
                secret TEXT,
                haex_hlc_no_trigger TEXT,
                haex_column_hlcs_no_trigger TEXT NOT NULL DEFAULT '{}'
            );",
    )
    .unwrap();
    conn
}

pub(super) fn insert_private_row(conn: &Connection, id: &str, secret: &str, hlc: &str) {
    let hlcs = format!("{{\"secret\":\"{hlc}\"}}");
    conn.execute(
        "INSERT INTO haex_passwords (id, secret, haex_hlc_no_trigger, haex_column_hlcs_no_trigger)
             VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![id, secret, hlc, hlcs],
    )
    .unwrap();
}

/// Convention: extension-owned CRDT tables use an `ext_<name>_v<n>`
/// prefix. Matches the literal used in `inbound_sync_tests::
/// validate_and_attribute::EXT_TABLE`, so the receiver-side and
/// scanner-side tests exercise the same shape.
pub(super) const EXT_TABLE: &str = "ext_notes_v1";

/// Wrap an in-memory `Connection` in a [`DbConnection`] the way the
/// production code expects. Mirrors the pattern in
/// `space_delivery::local::inbound_sync_tests::helpers::setup_authz_db`.
pub(super) fn wrap_db(conn: Connection) -> crate::database::DbConnection {
    use std::sync::{Arc, Mutex};
    crate::database::DbConnection(Arc::new(Mutex::new(Some(conn))))
}

/// In-memory DB with the schemas the registry-driven scan needs:
/// the register itself, one whitelisted control-plane table
/// (`haex_space_members`), and one extension-owned content table
/// (`EXT_TABLE`). No CRDT triggers or migrations — the scanner reads
/// plain rows.
pub(super) fn setup_registry_scan_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE haex_shared_space_sync (
                id TEXT PRIMARY KEY NOT NULL,
                table_name TEXT NOT NULL,
                row_pks TEXT NOT NULL,
                space_id TEXT NOT NULL,
                haex_hlc_no_trigger TEXT,
                haex_column_hlcs_no_trigger TEXT NOT NULL DEFAULT '{}',
                haex_column_sigs_no_trigger TEXT NOT NULL DEFAULT '{}'
            );

            CREATE TABLE haex_space_members (
                id TEXT PRIMARY KEY,
                space_id TEXT NOT NULL,
                identity_id TEXT NOT NULL,
                role TEXT NOT NULL DEFAULT 'read',
                authored_by_did TEXT,
                joined_at TEXT,
                haex_hlc_no_trigger TEXT,
                haex_column_hlcs_no_trigger TEXT NOT NULL DEFAULT '{}',
                haex_column_sigs_no_trigger TEXT NOT NULL DEFAULT '{}'
            );

            CREATE TABLE ext_notes_v1 (
                id TEXT PRIMARY KEY,
                body TEXT,
                haex_hlc_no_trigger TEXT,
                haex_column_hlcs_no_trigger TEXT NOT NULL DEFAULT '{}',
                haex_column_sigs_no_trigger TEXT NOT NULL DEFAULT '{}'
            );",
    )
    .unwrap();
    conn
}

/// Seed one row into `EXT_TABLE` with a per-column signature keyed by
/// `sig_space_id` (which is how the W1 write path attaches sigs
/// through `execute_with_crdt`). `sig_space_id = None` writes no sig,
/// used by the leak guard to prove the scanner still emits scoped
/// data even in that shape.
pub(super) fn insert_ext_row(
    conn: &Connection,
    id: &str,
    body: &str,
    hlc: &str,
    sig_space_id: Option<&str>,
) {
    let hlcs = format!("{{\"body\":\"{hlc}\"}}");
    let sigs = match sig_space_id {
        Some(space_id) => serde_json::json!({
            "body": {
                (space_id): {
                    "authorDid": "did:key:test",
                    "sig": "",
                    "storageClass": "text",
                }
            }
        })
        .to_string(),
        None => "{}".to_string(),
    };
    conn.execute(
        "INSERT INTO ext_notes_v1 (id, body, haex_hlc_no_trigger, haex_column_hlcs_no_trigger, haex_column_sigs_no_trigger)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![id, body, hlc, hlcs, sigs],
    )
    .unwrap();
}

/// Seed a `(table, row_pks, space_id)` triple into the register.
///
/// Direct INSERT (not `core::execute_with_crdt`) — the scanner only
/// READS `haex_shared_space_sync`, so bypassing the register-fanout
/// trigger is fine. Same trade-off as
/// `inbound_sync_tests::helpers::insert_registered`.
pub(super) fn insert_registry_entry(
    conn: &Connection,
    registry_row_id: &str,
    space_id: &str,
    table_name: &str,
    row_pks: &str,
) {
    // Give the register row a plausible HLC + per-column HLC map so the
    // scanner emits realistic `LocalColumnChange`s from the register
    // table itself (`haex_shared_space_sync` IS on the whitelist).
    // Without this, the register-row changes come out with
    // `hlc_timestamp = "haex_hlc_no_trigger"` (a literal-string fallback), which
    // is confusing when debugging failures on the ext-table assertions.
    let hlc = "1000000000000000000/aabbccdd";
    let hlcs = format!("{{\"table_name\":\"{hlc}\",\"row_pks\":\"{hlc}\",\"space_id\":\"{hlc}\"}}");
    conn.execute(
        "INSERT INTO haex_shared_space_sync
             (id, table_name, row_pks, space_id, haex_hlc_no_trigger, haex_column_hlcs_no_trigger)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![registry_row_id, table_name, row_pks, space_id, hlc, hlcs],
    )
    .unwrap();
}

/// Seed one row into `haex_space_members` with per-column sigs keyed by
/// `space_id`. Used by the regression guard test.
pub(super) fn insert_member_row(
    conn: &Connection,
    id: &str,
    space_id: &str,
    identity_id: &str,
    hlc: &str,
) {
    let hlcs = format!("{{\"identity_id\":\"{hlc}\",\"role\":\"{hlc}\"}}");
    let sigs = serde_json::json!({
        "identity_id": {
            (space_id): {
                "authorDid": "did:key:test",
                "sig": "",
                "storageClass": "text",
            }
        },
        "role": {
            (space_id): {
                "authorDid": "did:key:test",
                "sig": "",
                "storageClass": "text",
            }
        }
    })
    .to_string();
    conn.execute(
        "INSERT INTO haex_space_members
             (id, space_id, identity_id, role, haex_hlc_no_trigger, haex_column_hlcs_no_trigger, haex_column_sigs_no_trigger)
             VALUES (?1, ?2, ?3, 'read', ?4, ?5, ?6)",
        rusqlite::params![id, space_id, identity_id, hlc, hlcs, sigs],
    )
    .unwrap();
}
