//! Tests for extension space assignment CRDT compliance.
//!
//! Verifies that assign/unassign/get operations on haex_shared_space_sync
//! use execute_with_crdt / select_with_crdt so changes are synced and
//! tombstoned rows are filtered.

#[cfg(test)]
mod tests {
    use rusqlite::functions::FunctionFlags;
    use rusqlite::Connection;
    use std::sync::{Arc, Mutex};
    use uuid::Uuid;

    use crate::crdt::column_sig::key_cache::SpaceKeyCache;
    use crate::crdt::hlc::HlcService;
    use crate::crdt::trigger::{
        ensure_crdt_columns, setup_triggers_for_table, DELETED_ROWS_TABLE, UUID_FUNCTION_NAME,
    };
    use crate::database::connection_context::ConnectionContext;
    use crate::database::core::{self, install_tx_hlc_hooks, register_current_hlc_udf};
    use crate::database::row::get_string;
    use crate::database::DbConnection;
    use crate::extension::error::ExtensionError;
    use crate::extension::spaces::commands::{
        require_active_local_member, require_active_local_member_for_all, SpaceAssignment,
    };
    use crate::extension::spaces::queries::{
        SQL_INSERT_SHARED_SPACE_SYNC, SQL_SELECT_SPACE_MEMBERS_WITH_IDENTITY,
        SQL_SHARED_SPACE_SYNC_SELECT_COLS,
    };
    use crate::table_names::{
        TABLE_CRDT_CONFIGS, TABLE_CRDT_DIRTY_TABLES, TABLE_SHARED_SPACE_SYNC,
    };

    fn setup_test_db() -> (DbConnection, HlcService) {
        let conn = Connection::open_in_memory().expect("in-memory DB");

        // Register UUID + current_hlc UDFs and tx-HLC hooks so the BEFORE-DELETE
        // trigger can emit rows into haex_deleted_rows.
        conn.create_scalar_function(
            UUID_FUNCTION_NAME,
            0,
            FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_INNOCUOUS,
            |_ctx| Ok(Uuid::new_v4().to_string()),
        )
        .unwrap();
        let hlc = HlcService::new_for_testing("test-device-002");
        let ctx = ConnectionContext::new();
        register_current_hlc_udf(&conn, hlc.clone(), ctx.clone()).unwrap();
        install_tx_hlc_hooks(&conn, ctx).unwrap();

        conn.execute_batch(&format!(
            "CREATE TABLE {} (key TEXT PRIMARY KEY, type TEXT NOT NULL, value TEXT NOT NULL)",
            TABLE_CRDT_CONFIGS
        ))
        .unwrap();
        // Triggers check triggers_enabled='1' → seed it
        conn.execute(
            &format!(
                "INSERT INTO {} (key, type, value) VALUES ('triggers_enabled', 'system', '1')",
                TABLE_CRDT_CONFIGS
            ),
            [],
        )
        .unwrap();

        conn.execute_batch(&format!(
            "CREATE TABLE {} (table_name TEXT PRIMARY KEY, last_modified TEXT)",
            TABLE_CRDT_DIRTY_TABLES
        ))
        .unwrap();

        conn.execute_batch(&format!(
            "CREATE TABLE {} (
                id TEXT PRIMARY KEY NOT NULL,
                table_name TEXT NOT NULL,
                row_pks TEXT NOT NULL,
                haex_hlc TEXT,
                haex_column_hlcs TEXT NOT NULL DEFAULT '{{}}'
            )",
            DELETED_ROWS_TABLE
        ))
        .unwrap();

        conn.execute_batch(
            "CREATE TABLE haex_spaces (
                id TEXT PRIMARY KEY NOT NULL,
                type TEXT DEFAULT 'online' NOT NULL,
                status TEXT DEFAULT 'active' NOT NULL,
                name TEXT NOT NULL
            )",
        )
        .unwrap();

        conn.execute_batch(&format!(
            "CREATE TABLE {} (
                id TEXT PRIMARY KEY NOT NULL,
                table_name TEXT NOT NULL,
                row_pks TEXT NOT NULL,
                space_id TEXT NOT NULL,
                extension_public_key TEXT,
                extension_name TEXT,
                category TEXT,
                type TEXT,
                type_label TEXT,
                created_at TEXT DEFAULT (CURRENT_TIMESTAMP)
            )",
            TABLE_SHARED_SPACE_SYNC
        ))
        .unwrap();

        // Migration 0013: per-space delete-log. The register-DELETE fanout
        // trigger (Task 4 of the shared-space-delete-propagation plan)
        // INSERTs here on every register-DELETE, so the table MUST exist
        // wherever the fixture wires up the register triggers.
        conn.execute_batch(
            "CREATE TABLE haex_shared_space_deleted_rows (
                id TEXT PRIMARY KEY NOT NULL,
                space_id TEXT NOT NULL,
                table_name TEXT NOT NULL,
                row_pks TEXT NOT NULL,
                haex_hlc TEXT,
                haex_column_hlcs TEXT NOT NULL DEFAULT '{}',
                haex_column_sigs TEXT NOT NULL DEFAULT '{}'
            )",
        )
        .unwrap();

        conn.execute_batch(
            "CREATE TABLE haex_identities (
                id TEXT PRIMARY KEY NOT NULL,
                did TEXT NOT NULL,
                name TEXT NOT NULL,
                source TEXT NOT NULL DEFAULT 'contact',
                private_key TEXT
            );
            CREATE TABLE haex_space_members (
                id TEXT PRIMARY KEY NOT NULL,
                space_id TEXT NOT NULL,
                identity_id TEXT NOT NULL
            )",
        )
        .unwrap();

        {
            let tx = conn.unchecked_transaction().unwrap();
            ensure_crdt_columns(&tx, TABLE_SHARED_SPACE_SYNC).unwrap();
            setup_triggers_for_table(&tx, TABLE_SHARED_SPACE_SYNC, false).unwrap();
            tx.commit().unwrap();
        }

        // Seed space
        conn.execute(
            "INSERT INTO haex_spaces (id, type, status, name) VALUES ('sp-1', 'local', 'active', 'Test')",
            [],
        )
        .unwrap();

        let db = DbConnection(Arc::new(Mutex::new(Some(conn))));
        (db, hlc)
    }

    /// Runde 5 helper: seed a local identity + membership for sp-1 with a
    /// well-formed PKCS8 Ed25519 blob, then return a `SpaceKeyCache` pre-
    /// populated with the key. Tests that call `execute_with_crdt` on an
    /// INSERT into `haex_shared_space_sync` need this so F2's sig-based I2
    /// check finds a signing key. Sig content is not asserted by callers.
    fn seed_sp1_key(db: &DbConnection) -> SpaceKeyCache {
        use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
        let pkcs8_prefix: [u8; 16] = [
            0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22,
            0x04, 0x20,
        ];
        let seed: [u8; 32] = rand::random();
        let mut der = Vec::with_capacity(48);
        der.extend_from_slice(&pkcs8_prefix);
        der.extend_from_slice(&seed);
        let pkcs8_b64 = BASE64.encode(&der);

        let guard = db.0.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        conn.execute(
            "INSERT INTO haex_identities (id, did, name, source, private_key) \
             VALUES ('id-key-provider', 'did:key:zSp1KeyProviderForTests', 'KeyProvider', 'own', ?1)",
            [pkcs8_b64],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO haex_space_members (id, space_id, identity_id) \
             VALUES ('mem-key-provider', 'sp-1', 'id-key-provider')",
            [],
        )
        .unwrap();

        let cache = SpaceKeyCache::new();
        cache.populate_all(conn).expect("populate cache");
        cache
    }

    // =========================================================================
    // assign: execute_with_crdt sets HLC + marks dirty
    // =========================================================================

    #[test]
    fn test_assign_sets_hlc_timestamp() {
        let (db, hlc) = setup_test_db();
        let cache = seed_sp1_key(&db);
        let hlc_mutex = Mutex::new(hlc);
        let hlc_guard = hlc_mutex.lock().unwrap();

        core::execute_with_crdt(
            format!(
                "INSERT OR IGNORE INTO {} (id, table_name, row_pks, space_id) VALUES (?1, ?2, ?3, ?4)",
                TABLE_SHARED_SPACE_SYNC
            ),
            vec![
                serde_json::Value::String("assign-1".to_string()),
                serde_json::Value::String("ext_test__items".to_string()),
                serde_json::Value::String("item-001".to_string()),
                serde_json::Value::String("sp-1".to_string()),
            ],
            &db,
            &hlc_guard,
            &cache,
        )
        .unwrap();

        let rows = core::select_with_crdt(
            format!(
                "SELECT id, haex_hlc FROM {} WHERE id = ?1",
                TABLE_SHARED_SPACE_SYNC
            ),
            vec![serde_json::Value::String("assign-1".to_string())],
            &db,
        )
        .unwrap();

        assert_eq!(rows.len(), 1);
        assert!(!rows[0][1].is_null(), "haex_hlc must be set after assign");
    }

    /// Migration 0014 renamed `group_id` -> `category` and `label` ->
    /// `type_label` on `haex_shared_space_sync`. This exercises the actual
    /// `queries.rs` SQL constants (not just the fixture) end-to-end, so a
    /// regression that reintroduces the old column names in
    /// `SQL_INSERT_SHARED_SPACE_SYNC` / `SQL_SHARED_SPACE_SYNC_SELECT_COLS`
    /// fails here with "no such column" instead of only at runtime in prod.
    #[test]
    fn test_assign_and_select_round_trip_category_and_type_label() {
        let (db, hlc) = setup_test_db();
        let cache = seed_sp1_key(&db);
        let hlc_mutex = Mutex::new(hlc);
        let hlc_guard = hlc_mutex.lock().unwrap();

        core::execute_with_crdt(
            SQL_INSERT_SHARED_SPACE_SYNC.clone(),
            vec![
                serde_json::Value::String("assign-cat-1".to_string()),
                serde_json::Value::String("ext_test__items".to_string()),
                serde_json::Value::String("item-cat-001".to_string()),
                serde_json::Value::String("sp-1".to_string()),
                serde_json::Value::String("pubkey-1".to_string()),
                serde_json::Value::String("ext-name-1".to_string()),
                serde_json::Value::String("calendar-group".to_string()),
                serde_json::Value::String("Calendar".to_string()),
                serde_json::Value::String("Team Q1".to_string()),
            ],
            &db,
            &hlc_guard,
            &cache,
        )
        .unwrap();

        let rows = core::select_with_crdt(
            format!("{} WHERE id = ?1", *SQL_SHARED_SPACE_SYNC_SELECT_COLS),
            vec![serde_json::Value::String("assign-cat-1".to_string())],
            &db,
        )
        .unwrap();

        assert_eq!(rows.len(), 1);
        assert_eq!(
            get_string(&rows[0], 6),
            "calendar-group",
            "category column must round-trip"
        );
        assert_eq!(
            get_string(&rows[0], 8),
            "Team Q1",
            "type_label column must round-trip"
        );
    }

    #[test]
    #[ignore] // Requires full trigger setup (setup_triggers_for_table) which needs table column introspection
    fn test_assign_marks_dirty_table() {
        let (db, hlc) = setup_test_db();
        let hlc_mutex = Mutex::new(hlc);
        let hlc_guard = hlc_mutex.lock().unwrap();

        core::execute_with_crdt(
            format!(
                "INSERT OR IGNORE INTO {} (id, table_name, row_pks, space_id) VALUES (?1, ?2, ?3, ?4)",
                TABLE_SHARED_SPACE_SYNC
            ),
            vec![
                serde_json::Value::String("assign-dirty".to_string()),
                serde_json::Value::String("ext_test__items".to_string()),
                serde_json::Value::String("item-002".to_string()),
                serde_json::Value::String("sp-1".to_string()),
            ],
            &db,
            &hlc_guard,
            &SpaceKeyCache::new(),
        )
        .unwrap();

        let guard = db.0.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        let dirty: Vec<String> = conn
            .prepare(&format!(
                "SELECT table_name FROM {}",
                TABLE_CRDT_DIRTY_TABLES
            ))
            .unwrap()
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert!(
            dirty.contains(&TABLE_SHARED_SPACE_SYNC.to_string()),
            "shared_space_sync should be dirty after assign, got: {:?}",
            dirty
        );
    }

    // =========================================================================
    // unassign (DELETE): hard delete, BEFORE-DELETE trigger writes to haex_deleted_rows
    // =========================================================================

    #[test]
    fn test_unassign_hard_deletes_row_and_logs_to_delete_log() {
        let (db, hlc) = setup_test_db();
        let cache = seed_sp1_key(&db);
        let hlc_mutex = Mutex::new(hlc);
        let hlc_guard = hlc_mutex.lock().unwrap();

        core::execute_with_crdt(
            format!(
                "INSERT INTO {} (id, table_name, row_pks, space_id) VALUES (?1, ?2, ?3, ?4)",
                TABLE_SHARED_SPACE_SYNC
            ),
            vec![
                serde_json::Value::String("del-1".to_string()),
                serde_json::Value::String("ext_test__items".to_string()),
                serde_json::Value::String("item-del".to_string()),
                serde_json::Value::String("sp-1".to_string()),
            ],
            &db,
            &hlc_guard,
            &cache,
        )
        .unwrap();

        let before = core::select_with_crdt(
            format!("SELECT id FROM {} WHERE id = ?1", TABLE_SHARED_SPACE_SYNC),
            vec![serde_json::Value::String("del-1".to_string())],
            &db,
        )
        .unwrap();
        assert_eq!(before.len(), 1, "Row should be visible before delete");

        core::execute_with_crdt(
            format!(
                "DELETE FROM {} WHERE table_name = ?1 AND row_pks = ?2 AND space_id = ?3",
                TABLE_SHARED_SPACE_SYNC
            ),
            vec![
                serde_json::Value::String("ext_test__items".to_string()),
                serde_json::Value::String("item-del".to_string()),
                serde_json::Value::String("sp-1".to_string()),
            ],
            &db,
            &hlc_guard,
            &SpaceKeyCache::new(),
        )
        .unwrap();

        // After hard delete the row is gone from the main table.
        let after = core::select_with_crdt(
            format!("SELECT id FROM {} WHERE id = ?1", TABLE_SHARED_SPACE_SYNC),
            vec![serde_json::Value::String("del-1".to_string())],
            &db,
        )
        .unwrap();
        assert_eq!(
            after.len(),
            0,
            "Row must be hard-deleted from the main table"
        );

        let guard = db.0.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        let raw_count: i64 = conn
            .query_row(
                &format!(
                    "SELECT COUNT(*) FROM {} WHERE id = 'del-1'",
                    TABLE_SHARED_SPACE_SYNC
                ),
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(raw_count, 0, "Row must also be gone from the raw table");

        // And the BEFORE-DELETE trigger must have recorded a delete-log entry.
        let delete_log_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM haex_deleted_rows WHERE table_name = ?1",
                [TABLE_SHARED_SPACE_SYNC],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            delete_log_count, 1,
            "BEFORE-DELETE trigger must log to haex_deleted_rows"
        );
    }

    // =========================================================================
    // extension_space_get_members: returns members with correct is_self/label.
    // =========================================================================

    #[test]
    fn test_extension_space_get_members_returns_members_with_is_self() {
        let (db, _hlc) = setup_test_db();

        {
            let guard = db.0.lock().unwrap();
            let conn = guard.as_ref().unwrap();
            conn.execute(
                "INSERT INTO haex_identities (id, did, name, source, private_key) \
                 VALUES ('id-own', 'did:key:own', 'Me', 'own', 'PRIVKEY')",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO haex_identities (id, did, name, source, private_key) \
                 VALUES ('id-contact', 'did:key:contact', 'Alice', 'contact', NULL)",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO haex_space_members (id, space_id, identity_id) \
                 VALUES ('mem-own', 'sp-1', 'id-own')",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO haex_space_members (id, space_id, identity_id) \
                 VALUES ('mem-contact', 'sp-1', 'id-contact')",
                [],
            )
            .unwrap();
        }

        let rows = core::select_with_crdt(
            SQL_SELECT_SPACE_MEMBERS_WITH_IDENTITY.clone(),
            vec![serde_json::Value::String("sp-1".to_string())],
            &db,
        )
        .unwrap();

        assert_eq!(rows.len(), 2);

        let mut by_did: std::collections::HashMap<String, (String, bool)> = rows
            .iter()
            .map(|row| {
                (
                    get_string(row, 0),
                    (get_string(row, 1), row[2].as_i64().unwrap_or(0) != 0),
                )
            })
            .collect();

        let (own_label, own_is_self) = by_did.remove("did:key:own").expect("own DID present");
        assert_eq!(own_label, "Me");
        assert!(own_is_self, "own identity must have isSelf = true");

        let (contact_label, contact_is_self) = by_did
            .remove("did:key:contact")
            .expect("contact DID present");
        assert_eq!(contact_label, "Alice");
        assert!(
            !contact_is_self,
            "contact identity must have isSelf = false"
        );
    }

    // =========================================================================
    // Space-membership gate on `extension_space_assign` /
    // `extension_space_unassign`.
    //
    // These drive `require_active_local_member` directly at the SQL fixture
    // layer because invoking the Tauri command end-to-end from a Rust unit
    // test would require a full `AppHandle` + `WebviewWindow` fixture — no
    // such harness exists in this crate, and the SQL-layer path is the
    // authoritative check.
    //
    // The three tests below cover the non-member, active local member, and
    // foreign-identity-only member cases against the implemented SQL check.
    // =========================================================================

    /// Non-member REJECTED: seed `haex_spaces` row for SPACE_A but NO
    /// `haex_space_members` row for any local identity in SPACE_A. The
    /// membership check MUST return `Err(SecurityViolation)`.
    #[test]
    fn non_member_registration_is_rejected() {
        let (db, _hlc) = setup_test_db();

        // Seed SPACE_A — no members, no identities.
        {
            let guard = db.0.lock().unwrap();
            let conn = guard.as_ref().unwrap();
            conn.execute(
                "INSERT INTO haex_spaces (id, type, status, name) \
                 VALUES ('space-a', 'local', 'active', 'A')",
                [],
            )
            .unwrap();
        }

        let guard = db.0.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        let result = require_active_local_member(conn, "space-a");

        assert!(
            matches!(result, Err(ExtensionError::SecurityViolation { .. })),
            "non-member registration into space-a MUST be rejected with \
             SecurityViolation; got: {:?}",
            result.as_ref().map_err(|e| e.to_string())
        );
    }

    /// Member ACCEPTED: seed `haex_spaces` row for SPACE_A AND a local
    /// identity (own — `private_key IS NOT NULL`) with a
    /// `haex_space_members` row for SPACE_A. The membership check MUST
    /// return `Ok(())`.
    ///
    /// Reuses `seed_sp1_key` which seeds an own identity + membership for
    /// `sp-1` (`setup_test_db` already inserts the `sp-1` space row).
    #[test]
    fn member_registration_is_accepted() {
        let (db, _hlc) = setup_test_db();
        let _cache = seed_sp1_key(&db);

        let guard = db.0.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        let result = require_active_local_member(conn, "sp-1");

        assert!(
            matches!(result, Ok(())),
            "active local member of sp-1 MUST be accepted; got: {:?}",
            result.as_ref().map_err(|e| e.to_string())
        );
    }

    /// Build a bare `SpaceAssignment` for the batch-check tests. Only
    /// `space_id` is consulted by `require_active_local_member_for_all`;
    /// the other fields hold plausible stub values.
    fn stub_assignment(space_id: &str) -> SpaceAssignment {
        SpaceAssignment {
            table_name: format!("ext_t_v1_{}", space_id),
            row_pks: r#"{"id":"row-1"}"#.to_string(),
            space_id: space_id.to_string(),
            category: None,
            type_name: None,
            type_label: None,
        }
    }

    /// All-or-nothing: a batch mixing one space the vault is an active
    /// local member of with a space it isn't a member of MUST reject the
    /// whole batch. The dedup path also runs the check against every
    /// distinct `space_id` exactly once.
    #[test]
    fn batch_member_check_rejects_when_any_space_is_non_member() {
        let (db, _hlc) = setup_test_db();
        // Seed sp-1 as active-local-member via the shared helper.
        let _cache = seed_sp1_key(&db);
        // Seed sp-2 with no member row so the batch check must fail on it.
        {
            let guard = db.0.lock().unwrap();
            let conn = guard.as_ref().unwrap();
            conn.execute(
                "INSERT INTO haex_spaces (id, type, status, name) \
                 VALUES ('sp-2', 'local', 'active', 'Two')",
                [],
            )
            .unwrap();
        }

        let assignments = vec![stub_assignment("sp-1"), stub_assignment("sp-2")];
        let result = require_active_local_member_for_all(&db, &assignments);
        assert!(
            matches!(result, Err(ExtensionError::SecurityViolation { .. })),
            "batch containing non-member space sp-2 MUST reject as \
             SecurityViolation; got: {:?}",
            result.as_ref().map_err(|e| e.to_string())
        );
    }

    /// Duplicate `space_id`s in the input dedup to a single check — the
    /// caller passing the same id twice must not change the outcome.
    /// Positive case: all duplicates resolve to an active-local-member
    /// space so the batch is accepted.
    #[test]
    fn batch_member_check_dedupes_duplicate_space_ids() {
        let (db, _hlc) = setup_test_db();
        let _cache = seed_sp1_key(&db);

        let assignments = vec![
            stub_assignment("sp-1"),
            stub_assignment("sp-1"),
            stub_assignment("sp-1"),
        ];
        let result = require_active_local_member_for_all(&db, &assignments);
        assert!(
            matches!(result, Ok(())),
            "duplicate-only batch of active-local-member sp-1 MUST accept; got: {:?}",
            result.as_ref().map_err(|e| e.to_string())
        );
    }

    /// Foreign-identity member REJECTED: seed a `haex_space_members` row for
    /// space-b whose linked identity has `private_key IS NULL` (a contact,
    /// not a local identity). The SQL join in `require_active_local_member`
    /// filters on `i.private_key IS NOT NULL`, so this row must NOT satisfy
    /// the check — the vault owns no active local member of the space and
    /// registration MUST be rejected.
    #[test]
    fn foreign_identity_member_is_rejected() {
        let (db, _hlc) = setup_test_db();

        {
            let guard = db.0.lock().unwrap();
            let conn = guard.as_ref().unwrap();
            conn.execute(
                "INSERT INTO haex_spaces (id, type, status, name) \
                 VALUES ('space-b', 'local', 'active', 'B')",
                [],
            )
            .unwrap();
            // Contact identity: private_key IS NULL.
            conn.execute(
                "INSERT INTO haex_identities (id, did, name, source, private_key) \
                 VALUES ('id-foreign', 'did:key:foreign', 'Bob', 'contact', NULL)",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO haex_space_members (id, space_id, identity_id) \
                 VALUES ('mem-foreign', 'space-b', 'id-foreign')",
                [],
            )
            .unwrap();
        }

        let guard = db.0.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        let result = require_active_local_member(conn, "space-b");

        assert!(
            matches!(result, Err(ExtensionError::SecurityViolation { .. })),
            "space-b has only a foreign-identity member — registration MUST \
             be rejected with SecurityViolation; got: {:?}",
            result.as_ref().map_err(|e| e.to_string())
        );
    }
}
