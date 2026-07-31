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

/// Applies a specific bundled migration by tag to an in-memory connection.
/// Mirrors the production runner: splits on `--> statement-breakpoint`, applies
/// each statement in order. Does NOT pipe through CrdtTransformer — callers
/// that need the injected `haex_hlc` / `haex_column_hlcs` / `haex_column_sigs`
/// meta columns should call `ensure_crdt_columns` afterwards, matching the
/// retrofit path.
fn apply_migration_by_tag(conn: &Connection, tag_prefix: &str) {
    let mig_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("database/migrations");
    let sql_path = std::fs::read_dir(&mig_dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .find(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(tag_prefix) && n.ends_with(".sql"))
        })
        .unwrap_or_else(|| {
            panic!("migration with prefix '{tag_prefix}' must exist in {mig_dir:?}")
        });
    let sql = std::fs::read_to_string(&sql_path).unwrap();
    for stmt in sql.split("--> statement-breakpoint") {
        let stmt = stmt.trim();
        if !stmt.is_empty() {
            conn.execute_batch(stmt)
                .unwrap_or_else(|e| panic!("statement in {sql_path:?} failed: {e}\nSQL:\n{stmt}"));
        }
    }
}

/// Test-fixture stub: create the two tables that migration 0013's indexes
/// reference (`haex_shared_space_sync` from migration 0012, `haex_vault_settings`
/// from migration 0000). Production always has both by the time 0013 runs;
/// isolated-migration fixtures don't apply prior tags, so we synthesize just
/// enough of each schema to accept 0013's `CREATE INDEX` statements.
fn create_shared_space_sync_stub(conn: &Connection) {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS haex_shared_space_sync (
             id TEXT PRIMARY KEY NOT NULL,
             table_name TEXT NOT NULL,
             row_pks TEXT NOT NULL,
             space_id TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS haex_vault_settings (
             id TEXT PRIMARY KEY NOT NULL,
             key TEXT NOT NULL,
             value TEXT,
             device_id TEXT
         );",
    )
    .expect("create haex_shared_space_sync + haex_vault_settings stubs");
}

fn pragma_column_names(conn: &Connection, table: &str) -> Vec<String> {
    let mut stmt = conn
        .prepare(&format!("SELECT name FROM pragma_table_info('{table}')"))
        .expect("prepare pragma");
    stmt.query_map([], |row| row.get(0))
        .expect("pragma query")
        .collect::<Result<Vec<String>, _>>()
        .expect("collect columns")
}

#[test]
fn fresh_vault_shared_space_delete_log_receives_crdt_meta_via_transformer() {
    // Fresh-vault path: the migration SQL is piped through CrdtTransformer
    // at CREATE-TABLE transform time, which injects the CRDT meta columns.
    // Regression guard for Runde-10 fresh-vault issue from
    // [[column-sig-canonical-encoding]] — a hardcoded exclusion list in the
    // transformer would silently skip the new tables and leave them without
    // haex_hlc, so discover_crdt_tables would never pick them up.
    use crate::crdt::transformer::CrdtTransformer;
    let transformer = CrdtTransformer::new();

    let mig_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("database/migrations");
    let sql_path = std::fs::read_dir(&mig_dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .find(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("0013_") && n.ends_with(".sql"))
        })
        .expect("0013 migration must exist");
    let raw = std::fs::read_to_string(&sql_path).unwrap();

    let conn = Connection::open_in_memory().unwrap();
    create_shared_space_sync_stub(&conn);
    for stmt in raw.split("--> statement-breakpoint") {
        let stmt = stmt.trim();
        if stmt.is_empty() {
            continue;
        }
        let transformed = transformer
            .transform_ddl_statement(stmt)
            .unwrap_or_else(|e| panic!("transform failed for stmt: {stmt}\n{e:?}"));
        conn.execute_batch(&transformed)
            .unwrap_or_else(|e| panic!("apply failed for {transformed}\n{e:?}"));
    }

    for table in [
        "haex_shared_space_deleted_rows",
        "haex_space_compaction_anchors",
    ] {
        let cols = pragma_column_names(&conn, table);
        assert!(
            cols.iter().any(|c| c == "haex_hlc"),
            "{table} must have haex_hlc after CrdtTransformer, got: {cols:?}"
        );
        assert!(
            cols.iter().any(|c| c == "haex_column_hlcs"),
            "{table} must have haex_column_hlcs, got: {cols:?}"
        );
        assert!(
            cols.iter().any(|c| c == "haex_column_sigs"),
            "{table} must have haex_column_sigs, got: {cols:?}"
        );
    }
}

#[test]
fn retrofit_ensure_crdt_columns_adds_meta_to_shared_space_delete_log() {
    // Retrofit path: an existing dev vault at 0012 applies 0013, then
    // ensure_crdt_columns injects the CRDT meta cols. Task 2 lesson from
    // [[column-sig-canonical-encoding]] Runde 10 — verify both tables.
    use crate::crdt::trigger::ensure_crdt_columns;
    let mut conn = Connection::open_in_memory().unwrap();
    create_shared_space_sync_stub(&conn);
    apply_migration_by_tag(&conn, "0013_");
    let tx = conn.transaction().unwrap();
    ensure_crdt_columns(&tx, "haex_shared_space_deleted_rows").unwrap();
    ensure_crdt_columns(&tx, "haex_space_compaction_anchors").unwrap();
    tx.commit().unwrap();

    for table in [
        "haex_shared_space_deleted_rows",
        "haex_space_compaction_anchors",
    ] {
        let cols = pragma_column_names(&conn, table);
        assert!(
            cols.iter().any(|c| c == "haex_hlc"),
            "{table} must have haex_hlc after ensure_crdt_columns, got: {cols:?}"
        );
        assert!(
            cols.iter().any(|c| c == "haex_column_hlcs"),
            "{table} must have haex_column_hlcs, got: {cols:?}"
        );
        assert!(
            cols.iter().any(|c| c == "haex_column_sigs"),
            "{table} must have haex_column_sigs, got: {cols:?}"
        );
    }
}

#[test]
fn migration_0013_creates_shared_space_delete_log_table() {
    let conn = Connection::open_in_memory().unwrap();
    create_shared_space_sync_stub(&conn);
    apply_migration_by_tag(&conn, "0013_");
    let cols = pragma_column_names(&conn, "haex_shared_space_deleted_rows");
    assert!(
        cols.iter().any(|c| c == "id"),
        "missing id column, got: {cols:?}"
    );
    assert!(
        cols.iter().any(|c| c == "space_id"),
        "missing space_id column, got: {cols:?}"
    );
    assert!(
        cols.iter().any(|c| c == "table_name"),
        "missing table_name column, got: {cols:?}"
    );
    assert!(
        cols.iter().any(|c| c == "row_pks"),
        "missing row_pks column, got: {cols:?}"
    );
    // CRDT meta columns are added by CrdtTransformer / ensure_crdt_columns —
    // verified separately in Task 2.
}

#[test]
fn migration_0013_creates_compaction_anchors_table() {
    let conn = Connection::open_in_memory().unwrap();
    create_shared_space_sync_stub(&conn);
    apply_migration_by_tag(&conn, "0013_");
    let cols = pragma_column_names(&conn, "haex_space_compaction_anchors");
    assert!(
        cols.iter().any(|c| c == "space_id"),
        "missing space_id column, got: {cols:?}"
    );
    assert!(
        cols.iter().any(|c| c == "min_valid_hlc"),
        "missing min_valid_hlc column, got: {cols:?}"
    );
}

/// Test-fixture stub: the `haex_shared_space_sync` shape immediately before
/// migration 0014 runs — i.e. after 0000 (create) + 0010 (add
/// `authored_by_did`) + 0012 (drop `authored_by_did`) + 0013 (adds an index,
/// doesn't touch this table's columns). `authored_by_did` is intentionally
/// absent here (dropped by 0012, ADR 0002 §6.3) — 0014 reintroduces it as the
/// Registry-Row-Ownership author, this time paired with `row_sig` so the
/// claim is cryptographically verifiable (see
/// docs/plans/2026-07-31-shared-space-authorization-design.md).
fn create_shared_space_sync_pre_0014_stub(conn: &Connection) {
    conn.execute_batch(
        "CREATE TABLE haex_shared_space_sync (
             id TEXT PRIMARY KEY NOT NULL,
             table_name TEXT NOT NULL,
             row_pks TEXT NOT NULL,
             space_id TEXT NOT NULL,
             extension_public_key TEXT,
             extension_name TEXT,
             group_id TEXT,
             type TEXT,
             label TEXT,
             created_at TEXT DEFAULT (CURRENT_TIMESTAMP)
         );",
    )
    .expect("create pre-0014 haex_shared_space_sync stub");
}

#[test]
fn migration_0014_renames_and_adds_registry_authorization_columns() {
    let conn = Connection::open_in_memory().unwrap();
    create_shared_space_sync_pre_0014_stub(&conn);
    apply_migration_by_tag(&conn, "0014_");

    let cols = pragma_column_names(&conn, "haex_shared_space_sync");
    assert!(
        cols.iter().any(|c| c == "category"),
        "missing category (renamed from group_id), got: {cols:?}"
    );
    assert!(
        cols.iter().any(|c| c == "type_label"),
        "missing type_label (renamed from label), got: {cols:?}"
    );
    assert!(
        cols.iter().any(|c| c == "category_label"),
        "missing category_label, got: {cols:?}"
    );
    assert!(
        cols.iter().any(|c| c == "authored_by_did"),
        "missing authored_by_did, got: {cols:?}"
    );
    assert!(
        cols.iter().any(|c| c == "row_sig"),
        "missing row_sig, got: {cols:?}"
    );
    assert!(
        !cols.iter().any(|c| c == "group_id"),
        "group_id must be renamed away, got: {cols:?}"
    );
    assert!(
        !cols.iter().any(|c| c == "label"),
        "label must be renamed away, got: {cols:?}"
    );
}

#[test]
fn migration_0014_unique_constraint_rejects_duplicate_author_category() {
    let conn = Connection::open_in_memory().unwrap();
    create_shared_space_sync_pre_0014_stub(&conn);
    apply_migration_by_tag(&conn, "0014_");

    let insert =
        |id: &str, author: &str, space: &str, table: &str, row_pks: &str, category: &str| {
            conn.execute(
                "INSERT INTO haex_shared_space_sync
                (id, table_name, row_pks, space_id, authored_by_did, category, row_sig)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'sig')",
                rusqlite::params![id, table, row_pks, space, author, category],
            )
        };

    insert(
        "share-1",
        "alice-did",
        "space-1",
        "ext_cal_v1",
        r#"{"id":"r1"}"#,
        "work",
    )
    .expect("first insert for alice/space-1/ext_cal_v1/work must succeed");

    // Different content row, same (author, space, table, category) — must be
    // rejected by the new unique index, not merely by the pre-existing
    // (table_name, row_pks, space_id) uniqueness (hence the distinct row_pks).
    let dup = insert(
        "share-2",
        "alice-did",
        "space-1",
        "ext_cal_v1",
        r#"{"id":"r2"}"#,
        "work",
    );
    assert!(
        dup.is_err(),
        "second insert with same (author, space, table, category) must fail"
    );

    insert(
        "share-3",
        "bob-did",
        "space-1",
        "ext_cal_v1",
        r#"{"id":"r3"}"#,
        "work",
    )
    .expect("different author with same (space, table, category) must be allowed");
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
    // Faithful to the production runner: it splits on the same breakpoint marker
    // and additionally pipes each statement through CrdtTransformer, which is a
    // no-op for `_no_sync` tables (this marker table is one), so the SQL executed
    // here is byte-identical to what the runner applies.
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

/// Test-fixture stub: a minimal `haex_spaces` parent table, just enough for
/// migration 0015's `space_id` FK to resolve against a real parent row.
/// FK enforcement is on by default in this project's SQLCipher-backed
/// rusqlite build, so inserts against a non-existent parent id fail on the
/// FK before any other constraint is even reached.
fn create_haex_spaces_stub(conn: &Connection) {
    conn.execute_batch("CREATE TABLE haex_spaces (id TEXT PRIMARY KEY NOT NULL);")
        .expect("create haex_spaces stub");
}

#[test]
fn migration_0015_creates_ucan_grants_no_sync_table() {
    let conn = Connection::open_in_memory().unwrap();
    create_haex_spaces_stub(&conn);
    apply_migration_by_tag(&conn, "0015_");

    let exists: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='haex_space_ucan_grants_no_sync'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(exists, 1);
}

#[test]
fn migration_0015_role_check_rejects_invalid_value() {
    let conn = Connection::open_in_memory().unwrap();
    create_haex_spaces_stub(&conn);
    apply_migration_by_tag(&conn, "0015_");
    // Real parent row so the insert below fails on the role CHECK alone,
    // not incidentally on the space_id FK (FK enforcement is on by default
    // in this project's SQLCipher-backed rusqlite build).
    conn.execute("INSERT INTO haex_spaces (id) VALUES ('s1')", [])
        .unwrap();

    let bad = conn.execute(
        "INSERT INTO haex_space_ucan_grants_no_sync
            (id, space_id, issuer_did, audience_did, ucan_token, role, created_at)
         VALUES ('x', 's1', 'a', 'b', 'token', 'invalid_role', '2026-07-31')",
        [],
    );
    assert!(
        bad.is_err(),
        "role CHECK constraint must reject 'invalid_role'"
    );
}

#[test]
fn migration_0015_role_check_accepts_issued_and_received() {
    let conn = Connection::open_in_memory().unwrap();
    create_haex_spaces_stub(&conn);
    apply_migration_by_tag(&conn, "0015_");
    conn.execute("INSERT INTO haex_spaces (id) VALUES ('s1')", [])
        .unwrap();

    conn.execute(
        "INSERT INTO haex_space_ucan_grants_no_sync
            (id, space_id, issuer_did, audience_did, ucan_token, role, created_at)
         VALUES ('g-issued', 's1', 'a', 'b', 'token', 'issued', '2026-07-31')",
        [],
    )
    .expect("role='issued' must be accepted");

    conn.execute(
        "INSERT INTO haex_space_ucan_grants_no_sync
            (id, space_id, issuer_did, audience_did, ucan_token, role, created_at)
         VALUES ('g-received', 's1', 'b', 'a', 'token', 'received', '2026-07-31')",
        [],
    )
    .expect("role='received' must be accepted");
}

#[test]
fn migration_0015_fk_cascade_deletes_grant_when_space_deleted() {
    let conn = Connection::open_in_memory().unwrap();
    create_haex_spaces_stub(&conn);
    apply_migration_by_tag(&conn, "0015_");
    // FK enforcement is on by default in this project's SQLCipher-backed
    // rusqlite build (see create_haex_spaces_stub doc-comment) — no explicit
    // `PRAGMA foreign_keys = ON` needed here.

    conn.execute("INSERT INTO haex_spaces (id) VALUES ('s1')", [])
        .unwrap();
    conn.execute(
        "INSERT INTO haex_space_ucan_grants_no_sync
            (id, space_id, issuer_did, audience_did, ucan_token, role, created_at)
         VALUES ('g1', 's1', 'a', 'b', 'token', 'issued', '2026-07-31')",
        [],
    )
    .unwrap();

    let before: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM haex_space_ucan_grants_no_sync",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        before, 1,
        "grant insert must have actually landed before we test the cascade"
    );

    conn.execute("DELETE FROM haex_spaces WHERE id = 's1'", [])
        .unwrap();

    let remaining: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM haex_space_ucan_grants_no_sync",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        remaining, 0,
        "deleting the parent space must cascade-delete its ucan grants"
    );
}

#[test]
fn migration_0015_partial_unique_rejects_duplicate_active_grant() {
    let conn = Connection::open_in_memory().unwrap();
    create_haex_spaces_stub(&conn);
    apply_migration_by_tag(&conn, "0015_");
    conn.execute("INSERT INTO haex_spaces (id) VALUES ('s1')", [])
        .unwrap();

    let insert = |id: &str, role: &str| {
        conn.execute(
            "INSERT INTO haex_space_ucan_grants_no_sync
                (id, space_id, issuer_did, audience_did, ucan_token, role, created_at)
             VALUES (?1, 's1', 'alice', 'bob', 'token', ?2, '2026-07-31')",
            rusqlite::params![id, role],
        )
    };

    insert("g1", "issued").expect("first active grant must be accepted");

    let dup = insert("g2", "issued");
    assert!(
        dup.is_err(),
        "a second active grant for the same (space, issuer, audience, role) must be rejected"
    );

    conn.execute(
        "UPDATE haex_space_ucan_grants_no_sync SET revoked_at = '2026-08-01' WHERE id = 'g1'",
        [],
    )
    .expect("revoking g1 must succeed");

    insert("g3", "issued").expect(
        "once g1 is revoked, a new active grant for the same tuple must be accepted \
         (revoked grants don't count against the partial unique index)",
    );

    insert("g4", "received")
        .expect("a different role for the same (space, issuer, audience) must be accepted");
}
