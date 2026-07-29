//! Task 10 fresh-vault + retrofit regression for shared-space delete
//! propagation (ADR 0002 §6.5).
//!
//! Two guarantees this test locks in that neither unit nor integration
//! tests below the migration layer catch (Runde-10 lesson from
//! `[[column-sig-canonical-encoding]]`):
//!
//! 1. **Fresh-vault path.** All bundled drizzle migrations 0000..=0013
//!    apply cleanly when the DDL is piped through `CrdtTransformer` —
//!    no "duplicate column: haex_hlc" collisions between the
//!    transformer's meta-column injection and the migration SQL.
//!    Both new tables (haex_shared_space_deleted_rows and
//!    haex_space_compaction_anchors) carry the CRDT meta columns as
//!    a consequence.
//!
//! 2. **Retrofit path.** An old vault stopped at 0012 accepts migration
//!    0013 followed by `ensure_crdt_columns` and reaches an equivalent
//!    schema (same meta columns) — the "in-place ALTER" path.
//!
//! Both paths use the actual bundled SQL, so a hand-edit to any
//! migration file that shadows a CrdtTransformer-injected column would
//! be caught here rather than at first-run in CI.

use haex_vault_lib::crdt::transformer::CrdtTransformer;
use haex_vault_lib::crdt::trigger::ensure_crdt_columns;
use rusqlite::Connection;
use std::path::PathBuf;

fn migrations_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("database/migrations")
}

fn read_migration_by_prefix(prefix: &str) -> String {
    let dir = migrations_dir();
    let path = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .find(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(prefix) && n.ends_with(".sql"))
        })
        .unwrap_or_else(|| panic!("no migration with prefix '{prefix}' in {dir:?}"));
    std::fs::read_to_string(&path).unwrap()
}

fn apply_migration_via_transformer(conn: &Connection, sql: &str) {
    let transformer = CrdtTransformer::new();
    for stmt in sql.split("--> statement-breakpoint") {
        let stmt = stmt.trim();
        if stmt.is_empty() {
            continue;
        }
        // The transformer's parser only understands DDL/DML; statements
        // like `PRAGMA foreign_keys=OFF;` that ship in some drizzle
        // migrations parse-error. Production runner passes those through
        // unchanged (they're not CRDT-relevant). Mirror that behaviour
        // here: try to transform, fall back to raw on parse failure.
        let transformed = transformer
            .transform_ddl_statement(stmt)
            .unwrap_or_else(|_| stmt.to_string());
        conn.execute_batch(&transformed)
            .unwrap_or_else(|e| panic!("apply failed for {transformed}\n{e:?}"));
    }
}

fn pragma_column_names(conn: &Connection, table: &str) -> Vec<String> {
    let mut stmt = conn
        .prepare(&format!("SELECT name FROM pragma_table_info('{table}')"))
        .unwrap();
    stmt.query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

/// Load the drizzle journal and return the migration tag names in idx order.
fn journaled_tags() -> Vec<String> {
    let journal_path = migrations_dir().join("meta/_journal.json");
    let raw = std::fs::read_to_string(&journal_path).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let mut entries: Vec<(u64, String)> = parsed["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                e["idx"].as_u64().unwrap(),
                e["tag"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    entries.sort_by_key(|(idx, _)| *idx);
    entries.into_iter().map(|(_, tag)| tag).collect()
}

#[test]
fn fresh_vault_applies_all_migrations_including_0013_via_transformer() {
    let conn = Connection::open_in_memory().unwrap();
    let tags = journaled_tags();
    assert!(
        tags.iter().any(|t| t.starts_with("0013_")),
        "journal must list migration 0013, got: {tags:?}"
    );
    for tag in &tags {
        let path = migrations_dir().join(format!("{tag}.sql"));
        let sql = std::fs::read_to_string(&path).unwrap();
        apply_migration_via_transformer(&conn, &sql);
    }

    // Both new tables must exist post-migration.
    for table in [
        "haex_shared_space_deleted_rows",
        "haex_space_compaction_anchors",
    ] {
        let cols = pragma_column_names(&conn, table);
        assert!(!cols.is_empty(), "{table} must exist after all migrations");
        // CrdtTransformer injects the CRDT meta columns for every non
        // _no_sync CREATE TABLE — this catches the Runde-10 collision.
        assert!(
            cols.iter().any(|c| c == "haex_hlc"),
            "{table} needs haex_hlc, got {cols:?}"
        );
        assert!(
            cols.iter().any(|c| c == "haex_column_hlcs"),
            "{table} needs haex_column_hlcs, got {cols:?}"
        );
        assert!(
            cols.iter().any(|c| c == "haex_column_sigs"),
            "{table} needs haex_column_sigs, got {cols:?}"
        );
    }
}

#[test]
fn retrofit_path_from_0012_applies_0013_and_ensures_crdt_columns() {
    // Simulate the in-place upgrade of a vault sitting at 0012.
    let mut conn = Connection::open_in_memory().unwrap();
    let tags = journaled_tags();
    let tags_up_to_12: Vec<&String> = tags
        .iter()
        .take_while(|t| !t.starts_with("0013_"))
        .collect();
    for tag in tags_up_to_12 {
        let sql = read_migration_by_prefix(&format!("{}_", &tag[..4]));
        apply_migration_via_transformer(&conn, &sql);
    }

    // Now apply 0013 raw (no transformer) — mimicking the retrofit path
    // where the migration ships to an already-open vault and
    // `ensure_crdt_columns` back-fills the meta columns after the ALTER.
    let sql_0013 = read_migration_by_prefix("0013_");
    for stmt in sql_0013.split("--> statement-breakpoint") {
        let stmt = stmt.trim();
        if !stmt.is_empty() {
            conn.execute_batch(stmt).unwrap();
        }
    }
    let tx = conn.transaction().unwrap();
    for table in [
        "haex_shared_space_deleted_rows",
        "haex_space_compaction_anchors",
    ] {
        ensure_crdt_columns(&tx, table).unwrap();
    }
    tx.commit().unwrap();

    for table in [
        "haex_shared_space_deleted_rows",
        "haex_space_compaction_anchors",
    ] {
        let cols = pragma_column_names(&conn, table);
        assert!(
            cols.iter().any(|c| c == "haex_hlc"),
            "retrofit path: {table} needs haex_hlc, got {cols:?}"
        );
        assert!(
            cols.iter().any(|c| c == "haex_column_sigs"),
            "retrofit path: {table} needs haex_column_sigs, got {cols:?}"
        );
    }
}
