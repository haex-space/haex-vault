//! Guard tests for the hand-maintained manual-migrations folder.
//!
//! These tests validate (they do NOT generate) the consistency between
//! `database/migrations-manual/_journal.json` and the `.sql` files sitting
//! next to it. The journal is hand-maintained at dev time, so it is easy to
//! add a `.sql` file and forget to journal it (it would then never be applied
//! by the runner) or journal a tag whose `.sql` is missing (the runner would
//! fail at vault open). Both classes of bug are caught here at test time.
//!
//! The bundled runner reads these files via the Tauri resource resolver at
//! runtime; here in a plain unit test there is no AppHandle, so we resolve the
//! folder relative to `CARGO_MANIFEST_DIR` (the `src-tauri` crate root).

use serde::Deserialize;
use std::collections::HashSet;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
struct ManualJournalEntry {
    tag: String,
}

#[derive(Debug, Deserialize)]
struct ManualJournal {
    entries: Vec<ManualJournalEntry>,
}

fn manual_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("database/migrations-manual")
}

#[test]
fn manual_journal_and_sql_files_are_in_sync() {
    let dir = manual_dir();
    let journal_path = dir.join("_journal.json");

    let journal_content =
        std::fs::read_to_string(&journal_path).expect("manual _journal.json must exist");
    let journal: ManualJournal =
        serde_json::from_str(&journal_content).expect("manual _journal.json must be valid JSON");

    // Collect into a Vec first so duplicate tags are detectable — building a
    // HashSet directly would silently dedupe a duplicated manual tag.
    let journaled_tags_vec: Vec<String> = journal.entries.iter().map(|e| e.tag.clone()).collect();
    let journaled_tags: HashSet<String> = journaled_tags_vec.iter().cloned().collect();
    assert_eq!(
        journaled_tags_vec.len(),
        journaled_tags.len(),
        "manual _journal.json contains duplicate tags: {journaled_tags_vec:?}"
    );

    // Every manual tag must carry the `manual_` prefix. This is the naming
    // convention that keeps manual tags from colliding by-name with drizzle
    // tags in the applied-migrations tracking (see `load_bundled_migrations`).
    let missing_prefix: Vec<&String> = journaled_tags
        .iter()
        .filter(|tag| !tag.starts_with("manual_"))
        .collect();
    assert!(
        missing_prefix.is_empty(),
        "manual journal tags must start with the 'manual_' prefix: {missing_prefix:?}"
    );

    // Collect the .sql files actually present in the folder.
    let mut sql_tags: HashSet<String> = HashSet::new();
    for entry in std::fs::read_dir(&dir).expect("manual migrations folder must be readable") {
        let entry = entry.expect("readable dir entry");
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("sql") {
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .expect("sql file has a valid stem")
                .to_string();
            sql_tags.insert(stem);
        }
    }

    // Every journal entry must resolve to an existing .sql file.
    for tag in &journaled_tags {
        let sql_path = dir.join(format!("{tag}.sql"));
        assert!(
            sql_path.exists(),
            "manual journal lists tag '{tag}' but {sql_path:?} does not exist"
        );
    }

    // Every .sql file in the folder must be listed in the journal
    // (catches "file present but not journaled" — it would never be applied).
    let missing_from_journal: Vec<&String> = sql_tags.difference(&journaled_tags).collect();
    assert!(
        missing_from_journal.is_empty(),
        "manual .sql files present but not listed in _journal.json: {missing_from_journal:?}"
    );

    // And the reverse set difference, with a clearer message.
    let missing_files: Vec<&String> = journaled_tags.difference(&sql_tags).collect();
    assert!(
        missing_files.is_empty(),
        "manual journal tags with no matching .sql file: {missing_files:?}"
    );
}

// ===== Pending-column inner-helper tests =====
//
// These exercise the free helpers that operate on a bare `&Connection`
// (no Tauri State), so the owner-sync request handler and the sync-loop
// recovery step can reuse them.

use crate::table_names::TABLE_CRDT_PENDING_COLUMNS;
use rusqlite::Connection;

/// Build an in-memory connection with the pending-columns table created using
/// the real table-name const, so the schema tracks the production table name.
fn pending_columns_conn() -> Connection {
    let conn = Connection::open_in_memory().expect("open in-memory connection");
    conn.execute_batch(&format!(
        "CREATE TABLE {TABLE_CRDT_PENDING_COLUMNS} (
            table_name TEXT NOT NULL,
            column_name TEXT NOT NULL,
            row_pks TEXT NOT NULL,
            PRIMARY KEY(table_name, column_name, row_pks)
        );"
    ))
    .expect("create pending-columns table");
    conn
}

fn insert_pending(conn: &Connection, table_name: &str, column_name: &str, row_pks: &str) {
    conn.execute(
        &format!(
            "INSERT INTO {TABLE_CRDT_PENDING_COLUMNS} (table_name, column_name, row_pks) VALUES (?, ?, ?)"
        ),
        rusqlite::params![table_name, column_name, row_pks],
    )
    .expect("insert pending column");
}

fn insert_pending_row(conn: &Connection, t: &str, c: &str, pks: &str) {
    conn.execute(
        &format!(
            "INSERT INTO {TABLE_CRDT_PENDING_COLUMNS} (table_name, column_name, row_pks) VALUES (?, ?, ?)"
        ),
        rusqlite::params![t, c, pks],
    )
    .unwrap();
}

#[test]
fn pending_columns_count_is_zero_on_empty() {
    let conn = pending_columns_conn();
    assert_eq!(super::pending_columns_count(&conn).unwrap(), 0);
}

#[test]
fn pending_columns_count_matches_inserted_rows() {
    let conn = pending_columns_conn();
    insert_pending(&conn, "haex_files", "thumbnail", r#"{"id":"r1"}"#);
    insert_pending(&conn, "haex_files", "duration", r#"{"id":"r1"}"#);
    insert_pending(&conn, "haex_notes", "color", r#"{"id":"r1"}"#);
    assert_eq!(super::pending_columns_count(&conn).unwrap(), 3);
}

#[test]
fn get_pending_columns_inner_returns_inserted_pairs() {
    let conn = pending_columns_conn();
    insert_pending(&conn, "haex_files", "thumbnail", r#"{"id":"r1"}"#);
    insert_pending(&conn, "haex_notes", "color", r#"{"id":"r1"}"#);

    let mut pairs: Vec<(String, String)> = super::get_pending_columns_inner(&conn)
        .unwrap()
        .into_iter()
        .map(|c| (c.table_name, c.column_name))
        .collect();
    pairs.sort();

    assert_eq!(
        pairs,
        vec![
            ("haex_files".to_string(), "thumbnail".to_string()),
            ("haex_notes".to_string(), "color".to_string()),
        ]
    );
}

#[test]
fn get_pending_columns_inner_is_empty_on_empty_table() {
    let conn = pending_columns_conn();
    assert!(super::get_pending_columns_inner(&conn).unwrap().is_empty());
}

#[test]
fn clear_pending_column_inner_deletes_only_matching_row() {
    let conn = pending_columns_conn();
    insert_pending(&conn, "haex_files", "thumbnail", r#"{"id":"r1"}"#);
    insert_pending(&conn, "haex_files", "duration", r#"{"id":"r1"}"#);
    insert_pending(&conn, "haex_notes", "color", r#"{"id":"r1"}"#);

    super::clear_pending_column_inner(&conn, "haex_files", "thumbnail").unwrap();

    let mut remaining: Vec<(String, String)> = super::get_pending_columns_inner(&conn)
        .unwrap()
        .into_iter()
        .map(|c| (c.table_name, c.column_name))
        .collect();
    remaining.sort();

    assert_eq!(
        remaining,
        vec![
            ("haex_files".to_string(), "duration".to_string()),
            ("haex_notes".to_string(), "color".to_string()),
        ]
    );
}

#[test]
fn clear_pending_column_inner_nonexistent_pair_is_noop_ok() {
    let conn = pending_columns_conn();
    insert_pending(&conn, "haex_files", "thumbnail", r#"{"id":"r1"}"#);

    // Clearing a pair that does not exist must succeed and leave the row intact.
    super::clear_pending_column_inner(&conn, "haex_files", "does_not_exist").unwrap();
    super::clear_pending_column_inner(&conn, "no_such_table", "thumbnail").unwrap();

    assert_eq!(super::pending_columns_count(&conn).unwrap(), 1);
}

#[test]
fn get_pending_columns_inner_returns_distinct_columns() {
    let conn = pending_columns_conn();
    insert_pending_row(&conn, "devices", "bio", r#"{"id":"r1"}"#);
    insert_pending_row(&conn, "devices", "bio", r#"{"id":"r2"}"#); // same column, 2 rows
    let cols = super::get_pending_columns_inner(&conn).unwrap();
    assert_eq!(cols.len(), 1, "TS-facing list dedups to one (table,column)");
    assert_eq!(
        (cols[0].table_name.as_str(), cols[0].column_name.as_str()),
        ("devices", "bio")
    );
}

#[test]
fn get_pending_column_rows_inner_returns_each_owed_row() {
    let conn = pending_columns_conn();
    insert_pending_row(&conn, "devices", "bio", r#"{"id":"r1"}"#);
    insert_pending_row(&conn, "devices", "bio", r#"{"id":"r2"}"#);
    let rows = super::get_pending_column_rows_inner(&conn).unwrap();
    assert_eq!(rows.len(), 2);
}

#[test]
fn clear_pending_column_row_inner_clears_only_that_row() {
    let conn = pending_columns_conn();
    insert_pending_row(&conn, "devices", "bio", r#"{"id":"r1"}"#);
    insert_pending_row(&conn, "devices", "bio", r#"{"id":"r2"}"#);
    super::clear_pending_column_row_inner(&conn, "devices", "bio", r#"{"id":"r1"}"#).unwrap();
    let rows = super::get_pending_column_rows_inner(&conn).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].row_pks, r#"{"id":"r2"}"#);
}

#[test]
fn clear_pending_column_inner_clears_all_rows_of_column() {
    // HTTP authoritative path: clear-all-for-column wipes every owed row.
    let conn = pending_columns_conn();
    insert_pending_row(&conn, "devices", "bio", r#"{"id":"r1"}"#);
    insert_pending_row(&conn, "devices", "bio", r#"{"id":"r2"}"#);
    super::clear_pending_column_inner(&conn, "devices", "bio").unwrap();
    assert!(super::get_pending_column_rows_inner(&conn)
        .unwrap()
        .is_empty());
}

// Composite-PK rows: row_pks is a multi-key JSON string. The row-aware read +
// clear must treat the full string as the row identity (locks in correct
// handling for the many haex_* tables with composite PKs).
#[test]
fn row_aware_helpers_handle_composite_row_pks() {
    let conn = pending_columns_conn();
    let composite = r#"{"a":"1","b":"2"}"#;
    insert_pending_row(&conn, "links", "label", composite);
    insert_pending_row(&conn, "links", "label", r#"{"a":"1","b":"3"}"#);
    let rows = super::get_pending_column_rows_inner(&conn).unwrap();
    assert_eq!(
        rows.len(),
        2,
        "two distinct composite rows tracked separately"
    );
    super::clear_pending_column_row_inner(&conn, "links", "label", composite).unwrap();
    let rows = super::get_pending_column_rows_inner(&conn).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].row_pks, r#"{"a":"1","b":"3"}"#,
        "only the exact composite row cleared"
    );
}

#[test]
fn pending_columns_migration_0003_widens_pk_to_row_aware() {
    // The shipped 0003 migration must produce a (table_name, column_name,
    // row_pks) PK: the same (table,column) for two different rows must coexist.
    let mig_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("database/migrations");
    let sql_path = std::fs::read_dir(&mig_dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .find(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("0003_") && n.ends_with(".sql"))
        })
        .expect("0003 migration must exist");
    let sql = std::fs::read_to_string(&sql_path).unwrap();

    let conn = Connection::open_in_memory().unwrap();
    // Old (pre-0003) table shape so the migration's DROP has a target.
    conn.execute_batch(
        "CREATE TABLE haex_crdt_pending_columns_no_sync (
             table_name TEXT NOT NULL,
             column_name TEXT NOT NULL,
             PRIMARY KEY(table_name, column_name)
         );",
    )
    .unwrap();
    // Apply the shipped migration (runner splits on the breakpoint marker).
    for stmt in sql.split("--> statement-breakpoint") {
        let stmt = stmt.trim();
        if !stmt.is_empty() {
            conn.execute_batch(stmt).unwrap();
        }
    }

    // Two rows differing ONLY by row_pks must both insert (new PK admits them).
    conn.execute(
        "INSERT INTO haex_crdt_pending_columns_no_sync (table_name, column_name, row_pks)
         VALUES ('devices','bio','{\"id\":\"r1\"}')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO haex_crdt_pending_columns_no_sync (table_name, column_name, row_pks)
         VALUES ('devices','bio','{\"id\":\"r2\"}')",
        [],
    )
    .unwrap();
    // Exact triple-duplicate must violate the new PK.
    let dup = conn.execute(
        "INSERT INTO haex_crdt_pending_columns_no_sync (table_name, column_name, row_pks)
         VALUES ('devices','bio','{\"id\":\"r1\"}')",
        [],
    );
    assert!(
        dup.is_err(),
        "exact (table,column,row_pks) duplicate must be rejected"
    );
}
