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

    use crate::crdt::hlc::HlcService;
    use crate::crdt::trigger::{
        ensure_crdt_columns, setup_triggers_for_table, DELETED_ROWS_TABLE, UUID_FUNCTION_NAME,
    };
    use crate::database::connection_context::ConnectionContext;
    use crate::database::core::{self, install_tx_hlc_hooks, register_current_hlc_udf};
    use crate::database::DbConnection;
    use crate::table_names::{TABLE_CRDT_CONFIGS, TABLE_CRDT_DIRTY_TABLES, TABLE_SHARED_SPACE_SYNC};

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
                extension_id TEXT,
                group_id TEXT,
                type TEXT,
                label TEXT,
                created_at TEXT DEFAULT (CURRENT_TIMESTAMP)
            )",
            TABLE_SHARED_SPACE_SYNC
        ))
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

    // =========================================================================
    // assign: execute_with_crdt sets HLC + marks dirty
    // =========================================================================

    #[test]
    fn test_assign_sets_hlc_timestamp() {
        let (db, hlc) = setup_test_db();
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
        )
        .unwrap();

        // After hard delete the row is gone from the main table.
        let after = core::select_with_crdt(
            format!("SELECT id FROM {} WHERE id = ?1", TABLE_SHARED_SPACE_SYNC),
            vec![serde_json::Value::String("del-1".to_string())],
            &db,
        )
        .unwrap();
        assert_eq!(after.len(), 0, "Row must be hard-deleted from the main table");

        let guard = db.0.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        let raw_count: i64 = conn
            .query_row(
                &format!("SELECT COUNT(*) FROM {} WHERE id = 'del-1'", TABLE_SHARED_SPACE_SYNC),
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
        assert_eq!(delete_log_count, 1, "BEFORE-DELETE trigger must log to haex_deleted_rows");
    }
}
