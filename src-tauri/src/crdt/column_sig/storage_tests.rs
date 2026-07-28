use super::storage::{upsert_column_sigs, SigRecord};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use rusqlite::Connection;
use serde_json::Value;

fn random_sig_bytes() -> [u8; 64] {
    let a: [u8; 32] = rand::random();
    let b: [u8; 32] = rand::random();
    let mut out = [0u8; 64];
    out[..32].copy_from_slice(&a);
    out[32..].copy_from_slice(&b);
    out
}

fn make_sig(did: &str) -> SigRecord {
    SigRecord {
        author_did: did.to_string(),
        sig: random_sig_bytes(),
    }
}

/// In-memory table with a single TEXT primary key + `haex_column_sigs` JSON meta.
fn seed_single_pk_row(initial_sigs: &str) -> Connection {
    let conn = Connection::open_in_memory().expect("open in-memory");
    conn.execute_batch(
        "CREATE TABLE tbl (
            id TEXT PRIMARY KEY NOT NULL,
            haex_column_sigs TEXT NOT NULL DEFAULT '{}'
         );",
    )
    .expect("create schema");
    conn.execute(
        "INSERT INTO tbl (id, haex_column_sigs) VALUES (?1, ?2)",
        ["pk1", initial_sigs],
    )
    .unwrap();
    conn
}

/// In-memory table with a composite primary key (space_id, member_did).
fn seed_composite_pk_row(initial_sigs: &str) -> Connection {
    let conn = Connection::open_in_memory().expect("open in-memory");
    conn.execute_batch(
        "CREATE TABLE members (
            space_id TEXT NOT NULL,
            member_did TEXT NOT NULL,
            haex_column_sigs TEXT NOT NULL DEFAULT '{}',
            PRIMARY KEY (space_id, member_did)
         );",
    )
    .expect("create schema");
    conn.execute(
        "INSERT INTO members (space_id, member_did, haex_column_sigs) VALUES (?1, ?2, ?3)",
        ["s1", "d1", initial_sigs],
    )
    .unwrap();
    conn
}

fn read_sigs_single(conn: &Connection) -> Value {
    let raw: String = conn
        .query_row(
            "SELECT haex_column_sigs FROM tbl WHERE id = ?1",
            ["pk1"],
            |r| r.get(0),
        )
        .unwrap();
    serde_json::from_str(&raw).unwrap()
}

#[test]
fn upsert_adds_new_column_space_entry() {
    let conn = seed_single_pk_row("{}");
    let sig = make_sig("did:key:zAuthor");
    let sig_b64 = BASE64.encode(sig.sig);

    upsert_column_sigs(&conn, "tbl", r#"{"id":"pk1"}"#, "col_a", "space_A", &sig).unwrap();

    let json = read_sigs_single(&conn);
    assert_eq!(
        json["col_a"]["space_A"]["author_did"],
        Value::String("did:key:zAuthor".to_string())
    );
    assert_eq!(json["col_a"]["space_A"]["sig"], Value::String(sig_b64));
}

#[test]
fn upsert_preserves_other_spaces_for_same_column() {
    let sig_a = make_sig("did:key:zAlice");
    let existing = serde_json::json!({
        "col_a": {
            "space_A": {
                "author_did": sig_a.author_did,
                "sig": BASE64.encode(sig_a.sig),
            }
        }
    })
    .to_string();
    let conn = seed_single_pk_row(&existing);

    let sig_b = make_sig("did:key:zBob");
    upsert_column_sigs(&conn, "tbl", r#"{"id":"pk1"}"#, "col_a", "space_B", &sig_b).unwrap();

    let json = read_sigs_single(&conn);
    assert_eq!(
        json["col_a"]["space_A"]["author_did"],
        Value::String("did:key:zAlice".to_string())
    );
    assert_eq!(
        json["col_a"]["space_B"]["author_did"],
        Value::String("did:key:zBob".to_string())
    );
    assert_eq!(
        json["col_a"]["space_B"]["sig"],
        Value::String(BASE64.encode(sig_b.sig))
    );
}

#[test]
fn upsert_preserves_other_columns() {
    let sig_a = make_sig("did:key:zAlice");
    let existing = serde_json::json!({
        "col_a": {
            "space_A": {
                "author_did": sig_a.author_did,
                "sig": BASE64.encode(sig_a.sig),
            }
        }
    })
    .to_string();
    let conn = seed_single_pk_row(&existing);

    let sig_b = make_sig("did:key:zAlice");
    upsert_column_sigs(&conn, "tbl", r#"{"id":"pk1"}"#, "col_b", "space_A", &sig_b).unwrap();

    let json = read_sigs_single(&conn);
    // Both columns present
    assert!(json["col_a"]["space_A"].is_object());
    assert!(json["col_b"]["space_A"].is_object());
    // col_a untouched
    assert_eq!(
        json["col_a"]["space_A"]["sig"],
        Value::String(BASE64.encode(sig_a.sig))
    );
    // col_b written
    assert_eq!(
        json["col_b"]["space_A"]["sig"],
        Value::String(BASE64.encode(sig_b.sig))
    );
}

#[test]
fn upsert_replaces_existing_entry() {
    let sig_1 = make_sig("did:key:zOld");
    let existing = serde_json::json!({
        "col_a": {
            "space_A": {
                "author_did": sig_1.author_did,
                "sig": BASE64.encode(sig_1.sig),
            }
        }
    })
    .to_string();
    let conn = seed_single_pk_row(&existing);

    let sig_2 = make_sig("did:key:zNew");
    upsert_column_sigs(&conn, "tbl", r#"{"id":"pk1"}"#, "col_a", "space_A", &sig_2).unwrap();

    let json = read_sigs_single(&conn);
    assert_eq!(
        json["col_a"]["space_A"]["author_did"],
        Value::String("did:key:zNew".to_string())
    );
    assert_eq!(
        json["col_a"]["space_A"]["sig"],
        Value::String(BASE64.encode(sig_2.sig))
    );
}

#[test]
fn upsert_composite_pk_row() {
    let conn = seed_composite_pk_row("{}");
    let sig = make_sig("did:key:zAuthor");

    upsert_column_sigs(
        &conn,
        "members",
        r#"{"space_id":"s1","member_did":"d1"}"#,
        "role",
        "space_A",
        &sig,
    )
    .unwrap();

    let raw: String = conn
        .query_row(
            "SELECT haex_column_sigs FROM members WHERE space_id = ?1 AND member_did = ?2",
            ["s1", "d1"],
            |r| r.get(0),
        )
        .unwrap();
    let json: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(
        json["role"]["space_A"]["author_did"],
        Value::String("did:key:zAuthor".to_string())
    );
    assert_eq!(
        json["role"]["space_A"]["sig"],
        Value::String(BASE64.encode(sig.sig))
    );
}

#[test]
fn upsert_rejects_unsafe_table_name() {
    let conn = seed_single_pk_row("{}");
    let sig = make_sig("did:key:zAuthor");

    let err = upsert_column_sigs(
        &conn,
        "tbl; DROP TABLE tbl;--",
        r#"{"id":"pk1"}"#,
        "col_a",
        "space_A",
        &sig,
    )
    .unwrap_err();
    assert!(matches!(err, rusqlite::Error::InvalidParameterName(_)));
}

/// F#1 (Round-3 review): a corrupted root value (non-object) is reset to an
/// empty map — but logged instead of silently swallowed. The reset itself
/// still has to work so that a single bad row doesn't wedge every future
/// upsert on that row.
#[test]
fn upsert_recovers_from_non_object_root() {
    let conn = seed_single_pk_row("\"corrupted-string-value\"");
    let sig = make_sig("did:key:zAuthor");

    upsert_column_sigs(&conn, "tbl", r#"{"id":"pk1"}"#, "col_a", "space_A", &sig).unwrap();

    let json = read_sigs_single(&conn);
    assert!(
        json.is_object(),
        "root JSON must be an object after recovery"
    );
    assert_eq!(
        json["col_a"]["space_A"]["author_did"],
        Value::String("did:key:zAuthor".to_string())
    );
}

/// F#1 companion: a corrupted per-column entry (root object, but the column
/// value is a string instead of an object) is reset to an empty map.
#[test]
fn upsert_recovers_from_non_object_column_entry() {
    let conn = seed_single_pk_row(r#"{"col_a":"corrupted-string-value"}"#);
    let sig = make_sig("did:key:zAuthor");

    upsert_column_sigs(&conn, "tbl", r#"{"id":"pk1"}"#, "col_a", "space_A", &sig).unwrap();

    let json = read_sigs_single(&conn);
    assert!(
        json["col_a"].is_object(),
        "col_a entry must be an object after recovery"
    );
    assert_eq!(
        json["col_a"]["space_A"]["author_did"],
        Value::String("did:key:zAuthor".to_string())
    );
}
