use super::*;

fn sample_row(sink: &LogSink) {
    sink.write(
        "log-1",
        "2026-07-21T00:00:00Z",
        "info",
        "test",
        None,
        "hello world",
        None,
        "device-a",
    )
    .expect("write");
}

#[test]
fn in_memory_writes_and_reads_back() {
    let sink = LogSink::in_memory().expect("in_memory");
    sample_row(&sink);
    let conn = sink.conn.lock().unwrap();
    let (id, level, source, message, device_id): (String, String, String, String, String) = conn
        .query_row(
            "SELECT id, level, source, message, device_id FROM haex_logs_no_sync",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
        )
        .expect("row present");
    assert_eq!(id, "log-1");
    assert_eq!(level, "info");
    assert_eq!(source, "test");
    assert_eq!(message, "hello world");
    assert_eq!(device_id, "device-a");
}

#[test]
fn table_has_no_crdt_columns() {
    // Load-bearing invariant of the whole plan: the log table must not
    // gain `haex_hlc` / `haex_column_hlcs`, otherwise discover_crdt_tables
    // would pick it up and re-establish the feedback loop.
    let sink = LogSink::in_memory().expect("in_memory");
    let conn = sink.conn.lock().unwrap();
    let mut stmt = conn
        .prepare("PRAGMA table_info(haex_logs_no_sync)")
        .unwrap();
    let cols: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();
    assert!(
        !cols.iter().any(|c| c == "haex_hlc"),
        "haex_hlc leaked into haex_logs_no_sync. Cols: {cols:?}"
    );
    assert!(
        !cols.iter().any(|c| c == "haex_column_hlcs"),
        "haex_column_hlcs leaked into haex_logs_no_sync. Cols: {cols:?}"
    );
}

#[test]
fn execute_runs_arbitrary_delete() {
    let sink = LogSink::in_memory().expect("in_memory");
    sample_row(&sink);
    let deleted = sink
        .execute(
            "DELETE FROM haex_logs_no_sync WHERE id = ?1",
            &[JsonValue::String("log-1".into())],
        )
        .expect("execute");
    assert_eq!(deleted, 1);
    let n: i64 = {
        let conn = sink.conn.lock().unwrap();
        conn.query_row("SELECT COUNT(*) FROM haex_logs_no_sync", [], |r| r.get(0))
            .unwrap()
    };
    assert_eq!(n, 0);
}

#[test]
fn clone_shares_connection() {
    // Sink is Clone via Arc — both handles must see the same table.
    let sink_a = LogSink::in_memory().expect("in_memory");
    let sink_b = sink_a.clone();
    sample_row(&sink_a);
    let n: i64 = {
        let conn = sink_b.conn.lock().unwrap();
        conn.query_row("SELECT COUNT(*) FROM haex_logs_no_sync", [], |r| r.get(0))
            .unwrap()
    };
    assert_eq!(n, 1, "cloned sink handle must see writes from the original");
}
