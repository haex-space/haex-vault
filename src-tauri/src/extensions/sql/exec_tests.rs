// These tests read what the CRDT layer wrote.
#![allow(clippy::disallowed_methods)]

use serde_json::json;

use super::*;
use crate::extensions::sql::test_support::setup;

#[test]
fn writes_and_reads_of_own_tables_have_the_sdk_result_shape() {
    let s = setup();
    let inserted = s
        .sql(
            "INSERT INTO t:pages (id, body, n) VALUES (?, ?, ?) RETURNING id, n",
            json!(["p1", "hello", 7]),
        )
        .unwrap();
    assert_eq!(inserted.columns, ["id", "n"]);
    assert_eq!(inserted.rows, vec![vec![json!("p1"), json!(7)]]);
    assert_eq!(inserted.rows_affected, 1);
    assert!(inserted.last_insert_id.is_some());

    let updated = s
        .sql(
            "UPDATE t:pages SET body = ? WHERE id = ?",
            json!(["bye", "p1"]),
        )
        .unwrap();
    assert_eq!(updated.rows_affected, 1);
    assert_eq!(
        updated.last_insert_id, None,
        "the connection's last rowid is not this statement's"
    );

    let all = s.sql("SELECT * FROM t:pages", json!([])).unwrap();
    assert_eq!(
        all.columns,
        ["id", "body", "n", "data"],
        "sync columns are stripped"
    );
    assert_eq!(
        all.rows,
        vec![vec![json!("p1"), json!("bye"), json!(7), json!(null)]]
    );

    let none = s.sql("SELECT id FROM t:pages WHERE 0", json!([])).unwrap();
    assert_eq!(none.columns, ["id"]);
    assert!(none.rows.is_empty());

    assert_eq!(
        s.sql("DELETE FROM t:pages", json!([]))
            .unwrap()
            .rows_affected,
        1
    );
    assert_eq!(s.count("SELECT count(*) FROM t:pages"), 0);
}

#[test]
fn the_current_date_and_time_keywords_work_as_drizzle_writes_them() {
    // Drizzle inlines a `sql\`(CURRENT_TIMESTAMP)\`` default into the INSERT.
    let s = setup();
    let inserted = s
        .sql(
            "INSERT INTO t:pages (id, body) VALUES (?, (CURRENT_TIMESTAMP)) \
             RETURNING body = datetime('now'), CURRENT_DATE = date('now'), \
             CURRENT_TIME = time('now')",
            json!(["p1"]),
        )
        .unwrap();
    // SQLite holds `now` constant for one statement step; checking in a later statement can cross
    // the second boundary and make the timestamp comparison flaky.
    assert_eq!(inserted.rows, vec![vec![json!(1), json!(1), json!(1)]]);
}

#[test]
fn blobs_and_booleans_round_trip_and_writes_are_stamped() {
    let s = setup();
    s.sql(
        "INSERT INTO t:pages (id, n, data) VALUES (?, ?, ?)",
        json!(["b", true, {"$bytes": "AAEC"}]),
    )
    .unwrap();
    let read = s.sql("SELECT n, data FROM t:pages", json!([])).unwrap();
    assert_eq!(read.rows, vec![vec![json!(1), json!("AAEC")]]);
    assert_eq!(
        s.count("SELECT count(*) FROM t:pages WHERE haex_hlc_no_sync IS NOT NULL"),
        1,
        "the CRDT transformer stamped the row (FR-028)"
    );
}

#[test]
fn a_device_local_table_deletes_without_a_delete_marker() {
    let s = setup();
    s.sql(
        "INSERT INTO t:cache_no_sync (key, value) VALUES ('k', 'v')",
        json!([]),
    )
    .unwrap();
    let markers = s.count("SELECT count(*) FROM haex_deleted_rows");
    s.sql("DELETE FROM t:cache_no_sync", json!([])).unwrap();
    assert_eq!(s.count("SELECT count(*) FROM haex_deleted_rows"), markers);
}

#[test]
fn a_failing_statement_rolls_back_the_whole_transaction() {
    let s = setup();
    let error = s
        .transaction(&[
            "INSERT INTO t:pages (id) VALUES ('a')",
            "INSERT INTO t:pages (id) VALUES ('b')",
            "INSERT INTO t:pages (id) VALUES ('a')",
        ])
        .unwrap_err();
    assert_eq!(error.code.as_u16(), 2000);
    assert_eq!(s.count("SELECT count(*) FROM t:pages"), 0);

    let ok = s
        .transaction(&[
            "INSERT INTO t:pages (id) VALUES ('a')",
            "INSERT INTO t:pages (id) VALUES ('b')",
        ])
        .unwrap();
    assert_eq!(ok.rows_affected, 2);
}

#[test]
fn limits_end_a_statement_with_7000_and_roll_back() {
    let s = setup();
    for id in ["a", "b", "c"] {
        s.sql("INSERT INTO t:pages (id) VALUES (?)", json!([id]))
            .unwrap();
    }
    let rows = Limits {
        max_rows: 2,
        ..Limits::default()
    };
    assert_eq!(
        s.sql_with("SELECT id FROM t:pages", json!([]), &rows)
            .unwrap_err()
            .code
            .as_u16(),
        7000
    );
    assert_eq!(
        s.sql_with("UPDATE t:pages SET n = 1 RETURNING id", json!([]), &rows)
            .unwrap_err()
            .code
            .as_u16(),
        7000
    );
    assert_eq!(
        s.count("SELECT count(*) FROM t:pages WHERE n = 1"),
        0,
        "rolled back"
    );

    let short = Limits {
        max_sql_bytes: 10,
        ..Limits::default()
    };
    assert_eq!(
        s.sql_with("SELECT id FROM t:pages", json!([]), &short)
            .unwrap_err()
            .code
            .as_u16(),
        7000
    );

    let small = Limits {
        max_response_bytes: 1000,
        ..Limits::default()
    };
    assert_eq!(
        s.sql_with("SELECT zeroblob(4000)", json!([]), &small)
            .unwrap_err()
            .code
            .as_u16(),
        7000
    );

    let quick = Limits {
        timeout_ms: 1,
        ..Limits::default()
    };
    let endless = "WITH RECURSIVE n(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM n) \
                   SELECT count(*) FROM n";
    assert_eq!(
        s.sql_with(endless, json!([]), &quick)
            .unwrap_err()
            .code
            .as_u16(),
        7000
    );
}

#[test]
fn core_tables_are_refused_and_foreign_ones_need_a_permission() {
    let s = setup();
    assert_eq!(
        s.sql("SELECT * FROM chat_threads", json!([]))
            .unwrap_err()
            .code
            .as_u16(),
        1000
    );
    let foreign = format!("{}__cal__events", "b".repeat(64));
    let error = s
        .sql(&format!("SELECT * FROM {foreign}"), json!([]))
        .unwrap_err();
    assert_eq!(error.code.as_u16(), 1004);
    assert_eq!(
        error.details,
        Some(json!({"resourceType": "database", "action": "read", "target": foreign}))
    );
}
