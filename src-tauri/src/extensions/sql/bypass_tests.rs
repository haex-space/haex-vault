//! The bypass corpus of extension SQL (spec 017, SC-002, T052, contracts/sql-policy.md
//! §Umgehungssammlung), run three ways: pre-check alone, authorizer alone (pre-check skipped by
//! `exec::prepare_unchecked`), both. Every forbidden form must fail each way; every allowed form
//! must pass each way.
//!
//! Starting set ported from haex-space/haex-vault-legacy `8dce379d94e18fcd42c3b73686a06f984ca3f574`,
//! `src-tauri/src/extension/database/tests/sql_injection_tests/` (comment_smuggling,
//! dangerous_statements, edge_cases, multi_statement, extraction, table_prefix, union_select) with
//! holzi's tables in place of the haex-vault 2.x (Legacy) tables (`haex_extensions` → `chat_threads` and `haex_*`).
//! These run here as unit tests because they need the crate's test vault and the pre-check skip.

// The corpus reads what was stored to show nothing changed.
#![allow(clippy::disallowed_methods)]

use std::sync::Arc;

use serde_json::json;

use super::exec::{existing_tables, prepare, prepare_unchecked, run, Limits};
use super::test_support::{setup, setup_reading_foreign, t, tf, Setup};
use crate::extensions::error::BridgeError;

const FORBIDDEN: &[&str] = &[
    // WITH and subqueries in every clause.
    "WITH x AS (SELECT * FROM chat_threads) SELECT * FROM x",
    "WITH t:pages AS (SELECT * FROM chat_threads) SELECT * FROM t:pages",
    "SELECT * FROM t:pages WHERE EXISTS (SELECT 1 FROM chat_threads)",
    "SELECT count(*) FROM chat_threads",
    "SELECT 1 FROM sqlite_master",
    "SELECT count(*) FROM t:pages, chat_threads",
    "SELECT (SELECT count(*) FROM chat_threads) FROM t:pages",
    "SELECT * FROM t:pages GROUP BY id HAVING count(*) < (SELECT count(*) FROM chat_threads)",
    "SELECT * FROM t:pages ORDER BY (SELECT id FROM chat_threads)",
    "SELECT * FROM t:pages LIMIT (SELECT count(*) FROM chat_threads)",
    "SELECT * FROM (VALUES ((SELECT id FROM chat_threads)))",
    "INSERT INTO t:pages (id) VALUES ((SELECT id FROM chat_threads))",
    "INSERT INTO t:pages (id) VALUES ('r') RETURNING (SELECT id FROM chat_threads)",
    "INSERT INTO t:pages (id) VALUES ('p') ON CONFLICT (id) DO UPDATE SET body = (SELECT id FROM chat_threads)",
    "SELECT * FROM t:pages p JOIN t:pages q ON (SELECT 1 FROM chat_threads)",
    "SELECT CASE WHEN 1 THEN (SELECT id FROM chat_threads) END FROM t:pages",
    "SELECT lower((SELECT id FROM chat_threads)) FROM t:pages",
    // Names, qualifiers, quoting.
    "SELECT * FROM main.chat_threads",
    "SELECT * FROM temp.t:pages",
    "SELECT * FROM \"Chat_Threads\"",
    "SELECT * FROM [chat_threads]",
    "SELECT * FROM `chat_threads`",
    // Prefix collision and schema tables.
    "SELECT * FROM t:x__pages",
    "SELECT * FROM sqlite_master",
    "SELECT * FROM sqlite_schema",
    "SELECT * FROM pragma_table_info('chat_threads')",
    "SELECT * FROM json_each((SELECT id FROM chat_threads))",
    "SELECT * FROM haex_crdt_configs_no_sync",
    // Functions.
    "SELECT load_extension('x')",
    "SELECT sqlcipher_export('x')",
    "SELECT fts3_tokenizer('x')",
    // Statement kinds and comment tricks.
    "BEGIN",
    "COMMIT",
    "SAVEPOINT a",
    "ATTACH DATABASE 'x.db' AS attack",
    "PRAGMA writable_schema = ON",
    "PRAGMA table_info(chat_threads)",
    "VACUUM",
    "CREATE VIEW evil AS SELECT * FROM chat_threads",
    "DROP TABLE t:pages",
    "SELECT * FROM t:pages; DELETE FROM chat_threads",
    "SELECT * FROM t:pages /* ; */ ; DELETE FROM chat_threads",
    "INSERT INTO t:pages (id) VALUES ('m');INSERT INTO chat_threads (id) VALUES ('m');",
    // haex-vault 2.x's union and extraction cases.
    "SELECT id FROM t:pages UNION SELECT id FROM chat_threads",
    "UPDATE t:pages SET body = (SELECT id FROM chat_threads LIMIT 1)",
    "DELETE FROM t:pages WHERE id IN (SELECT id FROM chat_threads)",
    "INSERT INTO t:pages (id) SELECT id FROM chat_threads",
    "UPDATE chat_threads SET title = 'x'",
    "DELETE FROM chat_threads",
    "INSERT INTO haex_crdt_configs_no_sync (key, value) VALUES ('x', 'y')",
];

const ALLOWED: &[&str] = &[
    "SELECT * FROM t:pages",
    "INSERT INTO t:pages (id, body) VALUES ('p', 'x') RETURNING id",
    "UPDATE t:pages SET body = 'y' WHERE id = 'seed'",
    "DELETE FROM t:pages WHERE id = 'seed'",
    "INSERT INTO t:pages (id) VALUES ('seed') ON CONFLICT (id) DO UPDATE SET body = 'z'",
    "WITH RECURSIVE n(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM n WHERE x < 3) \
     SELECT p.id FROM t:pages p JOIN n ON n.x = 1",
    "SELECT value FROM json_each('[1,2,3]')",
    "DELETE FROM t:cache_no_sync WHERE key = 'k'",
    // No column read: SQLite names the table without a database.
    "SELECT count(*) FROM t:pages",
    "SELECT 1 FROM t:pages",
    "SELECT id FROM t:pages p WHERE EXISTS (SELECT 1 FROM t:cache_no_sync)",
    "WITH c AS (SELECT id FROM t:pages) SELECT count(*) FROM c",
    "SELECT count(*) FROM json_each('[1,2,3]')",
    "SELECT 1 FROM t:pages p, json_each(p.body)",
];

fn seeded() -> Setup {
    let s = setup();
    s.sql("INSERT INTO t:pages (id) VALUES ('seed')", json!([]))
        .unwrap();
    s
}

fn pre_check_only(s: &Setup, sql: &str) -> Result<(), BridgeError> {
    let existing = existing_tables(&s.vault)?;
    prepare(
        &t(sql),
        Vec::new(),
        &s.policy,
        &Limits::default(),
        &existing,
    )
    .map(drop)
}

fn authorizer_only(s: &Setup, sql: &str) -> Result<(), BridgeError> {
    let checked = prepare_unchecked(&t(sql), Vec::new());
    run(
        &s.vault,
        Arc::clone(&s.policy),
        &Limits::default(),
        vec![checked],
    )
    .map(drop)
}

fn both(s: &Setup, sql: &str) -> Result<(), BridgeError> {
    s.sql(sql, json!([])).map(drop)
}

/// What a forbidden statement must not have changed.
fn fingerprint(s: &Setup) -> (i64, i64, i64) {
    (
        s.count("SELECT count(*) FROM chat_threads"),
        s.count("SELECT count(*) FROM t:pages"),
        s.count("SELECT count(*) FROM haex_crdt_configs_no_sync"),
    )
}

#[test]
fn every_forbidden_form_fails_in_each_layer_alone_and_together() {
    for way in [pre_check_only, authorizer_only, both] {
        for sql in FORBIDDEN {
            let s = seeded();
            let before = fingerprint(&s);
            assert!(way(&s, sql).is_err(), "accepted: {sql}");
            assert_eq!(fingerprint(&s), before, "changed data: {sql}");
        }
    }
}

#[test]
fn every_allowed_form_passes_in_each_layer_alone_and_together() {
    for way in [pre_check_only, authorizer_only, both] {
        for sql in ALLOWED {
            let s = seeded();
            if let Err(error) = way(&s, sql) {
                panic!("refused: {sql}: {error:?}");
            }
        }
    }
}

#[test]
fn sync_columns_are_refused_by_the_pre_check_and_overwritten_without_it() {
    let s = seeded();
    let sql = "UPDATE t:pages SET haex_hlc_no_sync = 'forged' WHERE id = 'seed'";
    assert_eq!(pre_check_only(&s, sql).unwrap_err().code.as_u16(), 1000);
    // Without the pre-check the transformer stamps the row itself.
    let _ = authorizer_only(&s, sql);
    assert_eq!(
        s.count("SELECT count(*) FROM t:pages WHERE haex_hlc_no_sync = 'forged'"),
        0
    );
}

#[test]
fn two_statements_in_one_string_fail_without_the_pre_check_too() {
    let s = seeded();
    let sql = "SELECT * FROM t:pages; SELECT 1";
    assert_eq!(pre_check_only(&s, sql).unwrap_err().code.as_u16(), 1000);
    assert_eq!(authorizer_only(&s, sql).unwrap_err().code.as_u16(), 1000);
}

#[test]
fn a_with_name_shadowing_a_table_is_refused_by_the_pre_check_and_reads_nothing_real_without_it() {
    let s = seeded();
    let sql = "WITH chat_threads AS (SELECT 'cte' AS id) SELECT id FROM chat_threads";
    assert_eq!(pre_check_only(&s, sql).unwrap_err().code.as_u16(), 1000);
    // The authorizer alone sees only the CTE: what comes back is the CTE's own row.
    let checked = prepare_unchecked(sql, Vec::new());
    let result = run(
        &s.vault,
        Arc::clone(&s.policy),
        &Limits::default(),
        vec![checked],
    )
    .unwrap();
    assert_eq!(result.rows, vec![vec![json!("cte")]]);
}

/// With "read" on the extension `f:` (US6): what it may not do with that extension's tables.
const FORBIDDEN_WITH_READ: &[&str] = &[
    "INSERT INTO f:events (id) VALUES ('x')",
    "REPLACE INTO f:events (id, title) VALUES ('e1', 'changed')",
    "INSERT INTO f:events (id) SELECT id FROM t:pages RETURNING id",
    "UPDATE f:events SET title = 'changed'",
    "UPDATE f:events SET title = 'changed' WHERE id IN (SELECT id FROM t:pages)",
    "DELETE FROM f:events",
    "WITH x AS (SELECT 1) DELETE FROM f:events",
    "CREATE TABLE f:extra (id TEXT)",
    "ALTER TABLE f:events ADD COLUMN x TEXT",
    "ALTER TABLE f:events RENAME TO f:moved",
    "DROP TABLE f:events",
    "CREATE INDEX f:idx ON f:events (title)",
    "SELECT * FROM f:events e JOIN chat_threads c ON c.id = e.id",
    "SELECT * FROM f:events WHERE id IN (SELECT id FROM extensions)",
];

const ALLOWED_WITH_READ: &[&str] = &[
    "SELECT title FROM f:events",
    "SELECT count(*) FROM f:events",
    "SELECT p.id, e.title FROM t:pages p LEFT JOIN f:events e ON e.id = p.id",
    "SELECT * FROM t:pages WHERE EXISTS (SELECT 1 FROM f:events)",
    "WITH e AS (SELECT * FROM f:events) SELECT title FROM e",
    "INSERT INTO t:pages (id, body) SELECT id, title FROM f:events",
];

fn seeded_reading_foreign() -> Setup {
    let s = setup_reading_foreign();
    s.sql("INSERT INTO t:pages (id) VALUES ('seed')", json!([]))
        .unwrap();
    s
}

/// What a forbidden statement must not have changed in the other extension's table.
fn foreign_fingerprint(s: &Setup) -> (i64, i64, i64) {
    (
        s.count(&tf("SELECT count(*) FROM f:events")),
        s.count(&tf("SELECT count(*) FROM f:events WHERE title = 'kept'")),
        s.count("SELECT count(*) FROM sqlite_master"),
    )
}

#[test]
fn a_read_permission_never_writes_or_changes_the_other_extension_in_any_layer() {
    for way in [pre_check_only, authorizer_only, both] {
        for sql in FORBIDDEN_WITH_READ {
            let s = seeded_reading_foreign();
            let before = (fingerprint(&s), foreign_fingerprint(&s));
            assert!(way(&s, &tf(sql)).is_err(), "accepted: {sql}");
            assert_eq!(
                (fingerprint(&s), foreign_fingerprint(&s)),
                before,
                "changed data: {sql}"
            );
        }
    }
}

#[test]
fn a_read_permission_reads_in_each_layer_alone_and_together() {
    for way in [pre_check_only, authorizer_only, both] {
        for sql in ALLOWED_WITH_READ {
            let s = seeded_reading_foreign();
            if let Err(error) = way(&s, &tf(sql)) {
                panic!("refused: {sql}: {error:?}");
            }
        }
    }
}

#[test]
fn the_tables_of_an_extension_that_is_not_installed_are_refused_in_each_layer_despite_a_grant() {
    for way in [pre_check_only, authorizer_only, both] {
        let mut s = seeded_reading_foreign();
        let mut policy = (*s.policy).clone();
        policy.installed.clear();
        s.policy = Arc::new(policy);
        for sql in ALLOWED_WITH_READ {
            assert!(way(&s, &tf(sql)).is_err(), "accepted: {sql}");
        }
    }
}
