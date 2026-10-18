use super::*;
use crate::error::HolziError;

fn reason(result: Result<impl Sized>) -> String {
    match result {
        Err(HolziError::PasswordsImportFailed { reason }) => reason,
        Err(other) => panic!("unexpected error {other:?}"),
        Ok(_) => panic!("expected a failure"),
    }
}

fn file_with(dir: &tempfile::TempDir, name: &str, bytes: &[u8]) -> std::path::PathBuf {
    let path = dir.path().join(name);
    std::fs::write(&path, bytes).expect("write");
    path
}

#[test]
fn an_empty_or_short_file_is_not_a_vault() {
    let dir = tempfile::tempdir().expect("dir");
    assert_eq!(
        reason(open(&file_with(&dir, "empty.db", b""), "pw")),
        "unsupported_format"
    );
    assert_eq!(
        reason(open(&file_with(&dir, "short.db", &[7u8; 100]), "pw")),
        "unsupported_format"
    );
}

#[test]
fn an_unencrypted_sqlite_database_is_not_a_vault() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("plain.db");
    let conn = Connection::open(&path).expect("open");
    conn.execute_batch("CREATE TABLE t (x); INSERT INTO t VALUES (1);")
        .expect("fill");
    drop(conn);
    assert_eq!(reason(open(&path, "pw")), "unsupported_format");
}

#[test]
fn random_bytes_look_like_a_wrong_password() {
    let dir = tempfile::tempdir().expect("dir");
    let bytes: Vec<u8> = (0..8192u32)
        .map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8)
        .collect();
    assert_eq!(
        reason(open(&file_with(&dir, "random.db", &bytes), "pw")),
        "legacy_haex_vault_locked"
    );
}

#[test]
fn a_missing_file_is_unreadable() {
    let dir = tempfile::tempdir().expect("dir");
    assert_eq!(
        reason(open(&dir.path().join("missing.db"), "pw")),
        "unreadable"
    );
}

fn encrypted(dir: &tempfile::TempDir, sql: &str) -> std::path::PathBuf {
    let path = dir.path().join("vault.db");
    let conn = Connection::open(&path).expect("open");
    conn.pragma_update(None, "key", "pw").expect("key");
    conn.execute_batch(sql).expect("schema");
    drop(conn);
    path
}

#[test]
fn the_right_password_opens_the_copy_and_a_wrong_one_does_not() {
    let dir = tempfile::tempdir().expect("dir");
    let path = encrypted(&dir, "CREATE TABLE t (x); INSERT INTO t VALUES (1);");
    assert_eq!(reason(open(&path, "wrong")), "legacy_haex_vault_locked");
    let source = open(&path, "pw").expect("open");
    let x: i64 = source
        .conn
        .query_row("SELECT x FROM t", [], |r| r.get(0))
        .expect("read");
    assert_eq!(x, 1);
}

#[test]
fn a_vault_without_the_item_table_has_no_passwords() {
    let dir = tempfile::tempdir().expect("dir");
    let path = encrypted(&dir, "CREATE TABLE haex_settings (id TEXT PRIMARY KEY);");
    let source = open(&path, "pw").expect("open");
    assert_eq!(reason(check_layout(&source.conn)), "no_passwords");
}

#[test]
fn a_missing_required_table_is_a_layout_the_reader_cannot_read() {
    let dir = tempfile::tempdir().expect("dir");
    let path = encrypted(
        &dir,
        "CREATE TABLE haex_passwords_item_details (id TEXT PRIMARY KEY, title TEXT);",
    );
    let source = open(&path, "pw").expect("open");
    assert_eq!(reason(check_layout(&source.conn)), "unsupported_format");
}
