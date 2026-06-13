//! Shared test fixtures for `space_delivery::local` tests.
//!
//! This module is `#[cfg(test)]` only. It exposes a small helper surface
//! sufficient for `auth_gate_tests` — primarily a minimal in-memory DB
//! seeded with `haex_identities` + `haex_space_members` (the two tables
//! `is_active_space_member` joins over), plus a `make_ucan` builder.
//!
//! The schema deliberately mirrors a subset of `inbound_sync_tests::
//! setup_authz_db`: only the columns the membership-check SQL touches.
//! Seeding goes through `database::core::execute` so the rows are shaped
//! the same way a CRDT-applied row would be (CRDT-helper-friendly).
//!
//! ## Why not bypass CRDT helpers for test seeding?
//!
//! `haex_space_members` is a sync table — per project convention every
//! mutation must flow through `core::execute_with_crdt` / `core::execute`
//! so that triggers/HLC tracking happen consistently. The inbound_sync
//! test suite already follows this pattern (see `insert_identity` /
//! `insert_member`); we match it here for the same reason.

#![cfg(test)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use rusqlite::Connection;
use serde_json::json;

use crate::crdt::hlc::HlcService;
use crate::crdt::trigger::ensure_crdt_columns;
use crate::database::connection_context::ConnectionContext;
use crate::database::core::{self, install_tx_hlc_hooks, register_current_hlc_udf};
use crate::database::DbConnection;
use crate::table_names::{TABLE_CRDT_CONFIGS, TABLE_CRDT_DIRTY_TABLES};
use crate::ucan::{CapabilityLevel, ValidatedUcan};

/// In-memory DB with the minimum schemas `is_active_space_member` reads:
/// `haex_identities` + `haex_space_members`, plus the CRDT bookkeeping
/// tables the HLC hooks require, plus `haex_logs` so AuthGate audit-row
/// assertions can read what `log_to_db` writes.
///
/// Returns the DB handle alongside the `Arc<Mutex<HlcService>>` that
/// `auth_gate::authorize_request` and `logging::log_to_db` consume. The
/// HLC inside that Arc shares the same underlying clock as the one
/// registered via the `current_hlc()` UDF — so an audit-log INSERT
/// goes through the same HLC source as production.
///
/// ## Schema parity with production
///
/// The columns below are a deliberate subset of the production Drizzle
/// schemas (`src/database/schemas/identity.ts` for `haex_identities`,
/// `src/database/schemas/spaces.ts` for `haex_space_members`,
/// `src/database/schemas/logs.ts` for `haex_logs`). We mirror every
/// `NOT NULL` column production declares — even the ones our tests
/// never read — so that `insert_identity` / `insert_member` /
/// `log_to_db` exercise the same constraints production code does.
///
/// Mirrored from `haex_identities`: `id`, `did` (UNIQUE), `name` (NOT NULL),
/// `source` (NOT NULL DEFAULT 'contact'), `private_key`, `created_at`.
/// Deliberately omitted from `haex_identities`: `avatar`, `avatar_options`,
/// `notes` — purely optional UI columns the membership-check SQL never
/// touches.
///
/// Mirrored from `haex_space_members`: `id`, `space_id` (NOT NULL),
/// `identity_id` (NOT NULL), `role` (NOT NULL DEFAULT 'read'),
/// `authored_by_did`, `joined_at`. No production columns omitted —
/// the table is small enough that we keep it at full parity.
///
/// The `haex_logs` schema, the two CRDT bookkeeping tables, and the
/// `ensure_crdt_columns` call all live in [`init_logs_db_inner`] — this
/// function only adds `haex_identities` and `haex_space_members` on top.
/// Look at `init_logs_db_inner` for the `haex_logs` column list and the
/// `_no_sync` table-name convention.
///
/// CRDT-helper columns (e.g. `haex_tombstone`, HLC timestamps) are added
/// by `core::execute` at write-time, not by the migration, so they don't
/// appear in any CREATE TABLE in this module.
pub(crate) fn setup_membership_db() -> (DbConnection, Arc<Mutex<HlcService>>) {
    let (conn, hlc_service) = init_logs_db_inner();

    conn.execute_batch(
        "CREATE TABLE haex_identities (
            id TEXT PRIMARY KEY,
            did TEXT NOT NULL UNIQUE,
            name TEXT NOT NULL,
            source TEXT NOT NULL DEFAULT 'contact',
            private_key TEXT,
            created_at TEXT
        );

        CREATE TABLE haex_space_members (
            id TEXT PRIMARY KEY,
            space_id TEXT NOT NULL,
            identity_id TEXT NOT NULL,
            role TEXT NOT NULL DEFAULT 'read',
            authored_by_did TEXT,
            joined_at TEXT
        );",
    )
    .expect("create membership schema");

    let db = DbConnection(Arc::new(Mutex::new(Some(conn))));
    let hlc = Arc::new(Mutex::new(hlc_service));
    (db, hlc)
}

/// Open an in-memory DB seeded with everything `log_to_db` needs and nothing
/// else: HLC service + UDF + tx hooks, the two CRDT bookkeeping tables
/// (`haex_crdt_configs_no_sync`, `haex_crdt_dirty_tables_no_sync`), the
/// `haex_logs` table mirrored from production, and `ensure_crdt_columns`
/// run against it so `execute_with_crdt` writes succeed.
///
/// Returns the raw `Connection` + `HlcService` so callers can add their
/// own tables (membership, peers, …) before wrapping in a `DbConnection`.
/// [`setup_membership_db`] and `auth_gate_tests::empty_db` both build on
/// this — extracted to keep the two fixtures byte-identical on every
/// detail that's *not* their per-test schema (was a real source of drift
/// before the dedup landed: e.g. the missing `ensure_crdt_columns` call
/// would silently let `log_to_db` write garbage rows).
///
/// ## `haex_logs` schema parity
///
/// Mirrored from production (`src/database/schemas/logs.ts`):
/// `id`, `timestamp` (NOT NULL), `level` (NOT NULL), `source` (NOT NULL),
/// `extension_id` (nullable), `message` (NOT NULL), `metadata` (nullable),
/// `device_id` (NOT NULL). The `extension_id` FK to `haex_extensions` is
/// dropped here — we never seed `haex_extensions`, and `log_to_db` always
/// inserts NULL there.
///
/// ## Why this is not `ensure_crdt_columns_and_triggers`
///
/// `_and_triggers` would also install a BEFORE-DELETE trigger that writes
/// into `haex_deleted_rows` (not seeded in this fixture) and calls the
/// `current_hlc()` / `uuid_v4()` UDFs. Today's tests INSERT into
/// `haex_logs` but never DELETE, so the missing trigger is harmless. If
/// you extend the suite to cover delete paths, seed `haex_deleted_rows`
/// plus the required UDFs first, then switch this call to the
/// `_and_triggers` variant — otherwise the trigger body will fail with
/// `no such table: haex_deleted_rows`.
pub(crate) fn init_logs_db_inner() -> (Connection, HlcService) {
    let conn = Connection::open_in_memory().expect("in-memory DB");
    let hlc_service = HlcService::new_for_testing("test-device");
    let ctx = ConnectionContext::new();
    register_current_hlc_udf(&conn, hlc_service.clone(), ctx.clone()).expect("register hlc udf");
    install_tx_hlc_hooks(&conn, ctx).expect("install hlc hooks");

    conn.execute_batch(&format!(
        "CREATE TABLE {} (key TEXT PRIMARY KEY, type TEXT NOT NULL, value TEXT NOT NULL);",
        TABLE_CRDT_CONFIGS
    ))
    .expect("create crdt_configs");
    conn.execute_batch(&format!(
        "CREATE TABLE {} (table_name TEXT PRIMARY KEY, last_modified TEXT);",
        TABLE_CRDT_DIRTY_TABLES
    ))
    .expect("create crdt_dirty_tables");

    conn.execute_batch(
        "CREATE TABLE haex_logs (
            id TEXT PRIMARY KEY,
            timestamp TEXT NOT NULL,
            level TEXT NOT NULL,
            source TEXT NOT NULL,
            extension_id TEXT,
            message TEXT NOT NULL,
            metadata TEXT,
            device_id TEXT NOT NULL
        );",
    )
    .expect("create logs schema");

    {
        let tx = conn.unchecked_transaction().expect("begin crdt-columns tx");
        ensure_crdt_columns(&tx, "haex_logs").expect("ensure crdt columns on haex_logs");
        tx.commit().expect("commit crdt-columns tx");
    }

    (conn, hlc_service)
}

/// Insert an identity row keyed by `identity_id` with public DID `did`.
///
/// `name` and `source` are `NOT NULL` in production, so we provide
/// sensible defaults that match the production schema's column DEFAULT:
/// `name = "Test Identity"`, `source = "contact"`. `"contact"` is also
/// what every test in this codebase actually seeds — both AuthGate
/// peers and inbound-sync attackers are *peer* identities, never local
/// owners. If a future row-ownership rule keys on `source = 'own'` (e.g.
/// "reject pushes from your own DID"), this default forces tests to
/// opt in to that polarity explicitly instead of silently mis-asserting.
pub(crate) fn insert_identity(db: &DbConnection, identity_id: &str, did: &str) {
    core::execute(
        "INSERT INTO haex_identities (id, did, name, source) VALUES (?1, ?2, ?3, ?4)"
            .to_string(),
        vec![
            json!(identity_id),
            json!(did),
            json!("Test Identity"),
            json!("contact"),
        ],
        db,
    )
    .expect("seed identity");
}

/// Insert a space-membership row joining `space_id` and `identity_id`.
/// The presence of this row is what `is_active_space_member` checks; with
/// the delete-log model (no `haex_tombstone` column) revocation = absence.
pub(crate) fn insert_member(
    db: &DbConnection,
    member_row_id: &str,
    space_id: &str,
    identity_id: &str,
    role: &str,
) {
    core::execute(
        "INSERT INTO haex_space_members (id, space_id, identity_id, role) \
         VALUES (?1, ?2, ?3, ?4)"
            .to_string(),
        vec![
            json!(member_row_id),
            json!(space_id),
            json!(identity_id),
            json!(role),
        ],
        db,
    )
    .expect("seed membership");
}

/// Build a `ValidatedUcan` with a single capability entry. Mirrors the
/// helper in `inbound_sync_tests` — kept in sync deliberately because
/// the AuthGate consumes the same shape produced by `validate_token`.
pub(crate) fn make_ucan(
    audience: &str,
    space_id: &str,
    level: CapabilityLevel,
) -> ValidatedUcan {
    let mut capabilities = HashMap::new();
    capabilities.insert(space_id.to_string(), level);
    ValidatedUcan {
        issuer: "did:key:zIssuer".to_string(),
        audience: audience.to_string(),
        capabilities,
        expires_at: u64::MAX,
    }
}
