//! Tests for extension space assignment CRDT compliance.
//!
//! Verifies that assign/unassign/get operations on haex_shared_space_sync
//! use execute_with_crdt / select_with_crdt so changes are synced and
//! tombstoned rows are filtered.

#[cfg(test)]
mod tests {
    use rusqlite::Connection;
    use std::sync::{Arc, Mutex};

    use crate::crdt::hlc::HlcService;
    use crate::crdt::trigger::ensure_crdt_columns;
    use crate::database::core;
    use crate::database::DbConnection;
    use crate::table_names::{TABLE_CRDT_CONFIGS, TABLE_CRDT_DIRTY_TABLES, TABLE_SHARED_SPACE_SYNC};

    fn setup_test_db() -> (DbConnection, HlcService) {
        let conn = Connection::open_in_memory().expect("in-memory DB");

        conn.execute_batch(&format!(
            "CREATE TABLE {} (key TEXT PRIMARY KEY, type TEXT NOT NULL, value TEXT NOT NULL)",
            TABLE_CRDT_CONFIGS
        ))
        .unwrap();

        conn.execute_batch(&format!(
            "CREATE TABLE {} (table_name TEXT PRIMARY KEY, last_modified TEXT)",
            TABLE_CRDT_DIRTY_TABLES
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
            tx.commit().unwrap();
        }

        // Seed space
        conn.execute(
            "INSERT INTO haex_spaces (id, type, status, name) VALUES ('sp-1', 'local', 'active', 'Test')",
            [],
        )
        .unwrap();

        let hlc = HlcService::new_for_testing("test-device-002");
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
                "SELECT id, haex_timestamp FROM {} WHERE id = ?1",
                TABLE_SHARED_SPACE_SYNC
            ),
            vec![serde_json::Value::String("assign-1".to_string())],
            &db,
        )
        .unwrap();

        assert_eq!(rows.len(), 1);
        assert!(!rows[0][1].is_null(), "haex_timestamp must be set after assign");
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
    // unassign (DELETE): tombstone set, select_with_crdt filters it
    // =========================================================================

    #[test]
    fn test_unassign_tombstones_row_and_select_filters_it() {
        let (db, hlc) = setup_test_db();
        let hlc_mutex = Mutex::new(hlc);
        let hlc_guard = hlc_mutex.lock().unwrap();

        // Insert
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

        // Verify visible
        let before = core::select_with_crdt(
            format!("SELECT id FROM {} WHERE id = ?1", TABLE_SHARED_SPACE_SYNC),
            vec![serde_json::Value::String("del-1".to_string())],
            &db,
        )
        .unwrap();
        assert_eq!(before.len(), 1, "Row should be visible before delete");

        // Delete via CRDT (sets tombstone)
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

        // select_with_crdt should filter tombstoned rows
        let after = core::select_with_crdt(
            format!("SELECT id FROM {} WHERE id = ?1", TABLE_SHARED_SPACE_SYNC),
            vec![serde_json::Value::String("del-1".to_string())],
            &db,
        )
        .unwrap();
        assert_eq!(after.len(), 0, "Tombstoned row must be hidden by select_with_crdt");

        // Raw select should still see it
        let guard = db.0.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        let raw_count: i64 = conn
            .query_row(
                &format!("SELECT COUNT(*) FROM {} WHERE id = 'del-1'", TABLE_SHARED_SPACE_SYNC),
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(raw_count, 1, "Tombstoned row should still exist in raw table");
    }
}
