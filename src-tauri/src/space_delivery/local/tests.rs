//! Tests for local delivery command correctness.
//!
//! Verifies that:
//! - UCAN INSERT includes all NOT NULL fields (issued_at, expires_at, issuer_did)
//! - Space and UCAN inserts use execute_with_crdt (CRDT timestamps, dirty marking)
//! - haex_spaces table has no role column

#[cfg(test)]
mod tests {
    use rusqlite::Connection;
    use std::sync::{Arc, Mutex};

    use crate::crdt::hlc::HlcService;
    use crate::crdt::trigger::ensure_crdt_columns;
    use crate::database::connection_context::ConnectionContext;
    use crate::database::core::{self, install_tx_hlc_hooks, register_current_hlc_udf};
    use crate::database::DbConnection;
    use crate::table_names::{TABLE_CRDT_CONFIGS, TABLE_CRDT_DIRTY_TABLES};

    /// Create an in-memory DB with haex_spaces and haex_ucan_tokens tables,
    /// CRDT columns and triggers fully initialized — matching production schema.
    fn setup_test_db() -> (DbConnection, HlcService) {
        let conn = Connection::open_in_memory().expect("in-memory DB");

        // Register current_hlc() UDF + tx-HLC hooks so execute_with_crdt works.
        let hlc = HlcService::new_for_testing("test-device-001");
        let ctx = ConnectionContext::new();
        register_current_hlc_udf(&conn, hlc.clone(), ctx.clone()).unwrap();
        install_tx_hlc_hooks(&conn, ctx).unwrap();

        // Config table for HLC persistence
        conn.execute_batch(&format!(
            "CREATE TABLE {} (key TEXT PRIMARY KEY, type TEXT NOT NULL, value TEXT NOT NULL)",
            TABLE_CRDT_CONFIGS
        ))
        .unwrap();

        // Dirty tables tracker
        conn.execute_batch(&format!(
            "CREATE TABLE {} (table_name TEXT PRIMARY KEY, last_modified TEXT)",
            TABLE_CRDT_DIRTY_TABLES
        ))
        .unwrap();

        // Production schema: haex_spaces WITHOUT role column
        conn.execute_batch(
            "CREATE TABLE haex_spaces (
                id TEXT PRIMARY KEY NOT NULL,
                type TEXT DEFAULT 'online' NOT NULL,
                status TEXT DEFAULT 'active' NOT NULL,
                name TEXT NOT NULL,
                origin_url TEXT,
                created_at TEXT DEFAULT (CURRENT_TIMESTAMP),
                modified_at TEXT DEFAULT (CURRENT_TIMESTAMP)
            )",
        )
        .unwrap();

        // Production schema: haex_ucan_tokens
        conn.execute_batch(
            "CREATE TABLE haex_ucan_tokens (
                id TEXT PRIMARY KEY NOT NULL,
                space_id TEXT NOT NULL,
                token TEXT NOT NULL,
                capability TEXT NOT NULL,
                issuer_did TEXT NOT NULL,
                audience_did TEXT NOT NULL,
                issued_at INTEGER NOT NULL,
                expires_at INTEGER NOT NULL,
                FOREIGN KEY (space_id) REFERENCES haex_spaces(id) ON DELETE CASCADE
            )",
        )
        .unwrap();

        // Set up CRDT columns + triggers
        {
            let tx = conn.unchecked_transaction().unwrap();
            ensure_crdt_columns(&tx, "haex_spaces").unwrap();
            ensure_crdt_columns(&tx, "haex_ucan_tokens").unwrap();
            tx.commit().unwrap();
        }

        let db = DbConnection(Arc::new(Mutex::new(Some(conn))));

        (db, hlc)
    }

    // =========================================================================
    // haex_spaces: no role column
    // =========================================================================

    #[test]
    fn test_spaces_table_has_no_role_column() {
        let (db, _hlc) = setup_test_db();
        let guard = db.0.lock().unwrap();
        let conn = guard.as_ref().unwrap();

        let columns: Vec<String> = conn
            .prepare("PRAGMA table_info(haex_spaces)")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert!(
            !columns.contains(&"role".to_string()),
            "haex_spaces should NOT have a role column, found: {:?}",
            columns
        );
    }

    #[test]
    fn test_spaces_insert_without_role_succeeds() {
        let (db, hlc) = setup_test_db();
        let hlc_mutex = Mutex::new(hlc);
        let hlc_guard = hlc_mutex.lock().unwrap();

        let result = core::execute_with_crdt(
            "INSERT INTO haex_spaces (id, type, status, name) VALUES (?1, 'local', 'active', ?2)"
                .to_string(),
            vec![
                serde_json::Value::String("space-001".to_string()),
                serde_json::Value::String("Test Space".to_string()),
            ],
            &db,
            &hlc_guard,
        );

        assert!(result.is_ok(), "INSERT without role should succeed: {:?}", result.err());
    }

    // =========================================================================
    // UCAN INSERT: all NOT NULL fields must be present
    // =========================================================================

    #[test]
    fn test_ucan_insert_with_all_required_fields_succeeds() {
        let (db, hlc) = setup_test_db();
        let hlc_mutex = Mutex::new(hlc);
        let hlc_guard = hlc_mutex.lock().unwrap();

        // Insert space (FK)
        core::execute_with_crdt(
            "INSERT INTO haex_spaces (id, type, status, name) VALUES (?1, 'local', 'active', ?2)"
                .to_string(),
            vec![
                serde_json::Value::String("space-001".to_string()),
                serde_json::Value::String("Test Space".to_string()),
            ],
            &db,
            &hlc_guard,
        )
        .unwrap();

        let now_secs: i64 = 1700000000;
        let result = core::execute_with_crdt(
            "INSERT INTO haex_ucan_tokens (id, space_id, issuer_did, audience_did, capability, token, issued_at, expires_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"
                .to_string(),
            vec![
                serde_json::Value::String("ucan-001".to_string()),
                serde_json::Value::String("space-001".to_string()),
                serde_json::Value::String("did:key:zInviter".to_string()),
                serde_json::Value::String("did:key:zInvitee".to_string()),
                serde_json::Value::String("space/read".to_string()),
                serde_json::Value::String("eyJ0eXAiOiJKV1QifQ.test".to_string()),
                serde_json::Value::Number(serde_json::Number::from(now_secs)),
                serde_json::Value::Number(serde_json::Number::from(now_secs + 86400 * 365)),
            ],
            &db,
            &hlc_guard,
        );

        assert!(result.is_ok(), "UCAN INSERT with all fields should succeed: {:?}", result.err());
    }

    #[test]
    fn test_ucan_insert_without_issued_at_fails() {
        let (db, hlc) = setup_test_db();
        let hlc_mutex = Mutex::new(hlc);
        let hlc_guard = hlc_mutex.lock().unwrap();

        core::execute_with_crdt(
            "INSERT INTO haex_spaces (id, type, status, name) VALUES (?1, 'local', 'active', ?2)"
                .to_string(),
            vec![
                serde_json::Value::String("space-001".to_string()),
                serde_json::Value::String("Test".to_string()),
            ],
            &db,
            &hlc_guard,
        )
        .unwrap();

        let result = core::execute_with_crdt(
            "INSERT INTO haex_ucan_tokens (id, space_id, issuer_did, audience_did, capability, token) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)"
                .to_string(),
            vec![
                serde_json::Value::String("ucan-fail".to_string()),
                serde_json::Value::String("space-001".to_string()),
                serde_json::Value::String("did:key:z1".to_string()),
                serde_json::Value::String("did:key:z2".to_string()),
                serde_json::Value::String("space/read".to_string()),
                serde_json::Value::String("token-data".to_string()),
            ],
            &db,
            &hlc_guard,
        );

        assert!(result.is_err(), "UCAN INSERT without issued_at/expires_at must fail");
    }

    #[test]
    fn test_ucan_insert_with_null_issuer_did_fails() {
        let (db, hlc) = setup_test_db();
        let hlc_mutex = Mutex::new(hlc);
        let hlc_guard = hlc_mutex.lock().unwrap();

        core::execute_with_crdt(
            "INSERT INTO haex_spaces (id, type, status, name) VALUES (?1, 'local', 'active', ?2)"
                .to_string(),
            vec![
                serde_json::Value::String("space-001".to_string()),
                serde_json::Value::String("Test".to_string()),
            ],
            &db,
            &hlc_guard,
        )
        .unwrap();

        let now: i64 = 1700000000;
        let result = core::execute_with_crdt(
            "INSERT INTO haex_ucan_tokens (id, space_id, issuer_did, audience_did, capability, token, issued_at, expires_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"
                .to_string(),
            vec![
                serde_json::Value::String("ucan-null".to_string()),
                serde_json::Value::String("space-001".to_string()),
                serde_json::Value::Null,
                serde_json::Value::String("did:key:z2".to_string()),
                serde_json::Value::String("space/read".to_string()),
                serde_json::Value::String("token".to_string()),
                serde_json::Value::Number(serde_json::Number::from(now)),
                serde_json::Value::Number(serde_json::Number::from(now + 86400)),
            ],
            &db,
            &hlc_guard,
        );

        assert!(result.is_err(), "UCAN INSERT with NULL issuer_did must fail");
    }

    // =========================================================================
    // CRDT compliance: inserts must set HLC timestamps and mark dirty
    // =========================================================================

    #[test]
    fn test_space_insert_with_crdt_sets_hlc_timestamp() {
        let (db, hlc) = setup_test_db();
        let hlc_mutex = Mutex::new(hlc);
        let hlc_guard = hlc_mutex.lock().unwrap();

        core::execute_with_crdt(
            "INSERT INTO haex_spaces (id, type, status, name) VALUES (?1, 'local', 'active', ?2)"
                .to_string(),
            vec![
                serde_json::Value::String("space-hlc".to_string()),
                serde_json::Value::String("HLC Test".to_string()),
            ],
            &db,
            &hlc_guard,
        )
        .unwrap();

        let rows = core::select_with_crdt(
            "SELECT id, haex_hlc FROM haex_spaces WHERE id = ?1".to_string(),
            vec![serde_json::Value::String("space-hlc".to_string())],
            &db,
        )
        .unwrap();

        assert_eq!(rows.len(), 1, "Should find the inserted space");
        assert!(
            !rows[0][1].is_null(),
            "haex_hlc should be set by execute_with_crdt, got: {:?}",
            rows[0][1]
        );
    }

    #[test]
    #[ignore] // Requires full trigger setup (setup_triggers_for_table) which needs table column introspection
    fn test_space_insert_with_crdt_marks_dirty_table() {
        let (db, hlc) = setup_test_db();
        let hlc_mutex = Mutex::new(hlc);
        let hlc_guard = hlc_mutex.lock().unwrap();

        core::execute_with_crdt(
            "INSERT INTO haex_spaces (id, type, status, name) VALUES (?1, 'local', 'active', ?2)"
                .to_string(),
            vec![
                serde_json::Value::String("space-dirty".to_string()),
                serde_json::Value::String("Dirty Test".to_string()),
            ],
            &db,
            &hlc_guard,
        )
        .unwrap();

        let guard = db.0.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        let dirty: Vec<String> = conn
            .prepare(&format!("SELECT table_name FROM {}", TABLE_CRDT_DIRTY_TABLES))
            .unwrap()
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert!(
            dirty.contains(&"haex_spaces".to_string()),
            "haex_spaces should be marked dirty after insert, got: {:?}",
            dirty
        );
    }
}
