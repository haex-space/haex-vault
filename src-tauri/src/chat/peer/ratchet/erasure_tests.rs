//! Erasure at storage level (spec 095 T010, quickstart scenario 9 step 3, SC-001): a session's
//! earlier key state is gone from the database file and its write-ahead log once the session moved
//! on, was deleted and the database closed as the vault closes.
//!
//! The vault's file is encrypted and this build of SQLite cannot read it page by page, so the test
//! runs on an unencrypted file with the vault's settings (WAL, `secure_delete`), the tables of
//! migration `0063_chat_ratchet` and the statements of [`super::store`].

// A plain SQLite file without haex-crdt, read below the CRDT write path.
#![allow(clippy::disallowed_methods)]

use haex_crdt::rusqlite::{params, Connection};
use vodozemac::olm::{Account, OlmMessage, Session, SessionConfig};

use super::store::{DELETE_SESSION, UPSERT_SESSION};
use crate::identity::migrations_chat_ratchet::CHAT_RATCHET_0063;

fn pickle(session: &Session) -> String {
    serde_json::to_string(&session.pickle()).expect("pickle")
}

/// Two matching sides of a session after the receiver has accepted an opening message.
fn session_pair() -> (Session, Session) {
    let sender = Account::new();
    let mut receiver = Account::new();
    receiver.generate_fallback_key();
    let start = *receiver.fallback_key().values().next().expect("start key");
    let mut outbound = sender
        .create_outbound_session(SessionConfig::version_1(), receiver.curve25519_key(), start)
        .expect("outbound");
    let OlmMessage::PreKey(opening) = outbound.encrypt("hallo").expect("encrypt") else {
        panic!("an opening message")
    };
    let inbound = receiver
        .create_inbound_session(
            SessionConfig::version_1(),
            sender.curve25519_key(),
            &opening,
        )
        .expect("inbound")
        .session;
    (outbound, inbound)
}

fn write_session(conn: &Connection, session_id: &str, pickle: &str) {
    conn.execute(
        UPSERT_SESSION,
        params![
            "installation",
            "peer",
            session_id,
            pickle,
            "identity",
            1,
            None::<i64>,
            2,
            "key",
            2,
            None::<i64>
        ],
    )
    .expect("write session");
}

fn checkpoint(conn: &Connection) {
    conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))
        .expect("checkpoint");
}

fn contains(path: &std::path::Path, needle: &[u8]) -> bool {
    let bytes = std::fs::read(path).unwrap_or_default();
    bytes.windows(needle.len()).any(|window| window == needle)
}

#[test]
fn an_earlier_key_state_is_gone_from_the_file_and_the_log_after_the_close() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("erasure.db");
    let wal = dir.path().join("erasure.db-wal");
    let conn = Connection::open(&path).expect("open");
    let mode: String = conn
        .query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))
        .expect("wal");
    assert_eq!(mode, "wal");
    conn.execute_batch("PRAGMA secure_delete = ON")
        .expect("secure_delete");
    for statement in CHAT_RATCHET_0063.split("--> statement-breakpoint") {
        if statement.trim_start().starts_with("CREATE TABLE") {
            conn.execute_batch(statement).expect("table");
        }
    }

    let (mut session, mut peer) = session_pair();
    let session_id = session.session_id();
    // Move the sending ratchet forward by accepting the peer's first reply. The
    // old active sending root key is no longer part of the session pickle then.
    let earlier = pickle(&session);
    let OlmMessage::Normal(reply) = peer.encrypt("weiter").expect("peer reply") else {
        panic!("an established session sends a normal message")
    };
    session
        .decrypt(&OlmMessage::Normal(reply))
        .expect("the session moves on");
    let advanced = pickle(&session);
    // The active sending ratchet's root key is secret material discarded when this
    // session advances. The type tag alone would also differ, but proves nothing about erasure.
    let earlier_json: serde_json::Value = serde_json::from_str(&earlier).expect("earlier pickle");
    let needle = serde_json::to_vec(&earlier_json["sending_ratchet"]["active_ratchet"]["root_key"])
        .expect("serialize the active sending ratchet root key");
    let advanced_json: serde_json::Value =
        serde_json::from_str(&advanced).expect("advanced pickle");
    assert_ne!(
        advanced_json["sending_ratchet"]["active_ratchet"]["root_key"],
        earlier_json["sending_ratchet"]["active_ratchet"]["root_key"],
        "advancing discards the prior active sending root key"
    );

    write_session(&conn, &session_id, &earlier);
    checkpoint(&conn);
    assert!(
        contains(&path, &needle),
        "positive control: the test finds the stored key state"
    );

    write_session(&conn, &session_id, &advanced);
    conn.execute(DELETE_SESSION, params!["installation", "peer", session_id])
        .expect("delete");
    checkpoint(&conn);
    drop(conn);

    assert!(!contains(&path, &needle), "not in the database file");
    assert!(!contains(&wal, &needle), "not in the write-ahead log");
}
