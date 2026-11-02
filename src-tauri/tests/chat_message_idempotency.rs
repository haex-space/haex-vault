//! Integration coverage for the `idempotency_key` column added to
//! `chat_messages` (spec 002 §send_message, Slice E / T045).
//!
//! Needs a real bootstrapped `Database` for `current_hlc()` and the
//! partial unique index, so this lives in the integration suite rather
//! than a colocated `chat_messages_tests.rs` (see
//! `src-tauri/src/storage/preferences_tests.rs` for the same split).

// These tests read raw vault state (counts, CRDT columns, the schema) that the
// CRDT write path does not expose.
#![allow(clippy::disallowed_methods)]

use std::path::PathBuf;
use std::sync::{Arc, Barrier};

use haex_crdt::{Database, DatabaseConfig, NoopSignatureProvider, SqlCipherKey};
use uuid::Uuid;

use holzi_lib::chat::send_admission::{
    derive_message_ids, persist_send_transaction, resolve_idempotent_send, IdempotentSend,
    PersistedSend,
};
use holzi_lib::identity::{
    holzi_migration_source, installation_id_path, HolziBootstrap, HOLZI_TRIGGER_VERSION,
};
use holzi_lib::storage::chat_messages::{self, ChatMessage, FinishReason, MessageRole};
use holzi_lib::storage::query;

const PASSPHRASE: &str = "chat-message-idempotency";

fn make_config(db_path: PathBuf, installation_id: PathBuf) -> DatabaseConfig {
    DatabaseConfig {
        path: db_path,
        key: SqlCipherKey::new(PASSPHRASE),
        create_if_missing: true,
        bootstrap: Arc::new(HolziBootstrap::new(installation_id).with_alias("test")),
        signature_provider: Arc::new(NoopSignatureProvider),
        migration_source: holzi_migration_source(),
        trigger_version: HOLZI_TRIGGER_VERSION,
        max_transaction_bytes: haex_crdt::MAX_CRDT_TRANSACTION_BYTES,
        max_value_bytes: haex_crdt::MAX_VALUE_BYTES,
    }
}

fn open_db() -> Database {
    let tmp = tempfile::tempdir().expect("tmp dir");
    let db_path = tmp.path().join("vault.db");
    let installation_id_file = installation_id_path(tmp.path());
    let db = Database::open(make_config(db_path, installation_id_file)).expect("genesis open");
    std::mem::forget(tmp); // keep the tempdir alive for the DB's lifetime
    db
}

fn sample_message(
    id: Uuid,
    thread_id: Uuid,
    content: &str,
    idempotency_key: Option<&str>,
) -> ChatMessage {
    ChatMessage {
        id,
        thread_id,
        parent_id: None,
        role: MessageRole::User,
        content: content.to_string(),
        provider_id: None,
        model_id: None,
        prompt_tokens: None,
        completion_tokens: None,
        finish_reason: Some(FinishReason::Complete),
        created_at: 0,
        idempotency_key: idempotency_key.map(|s| s.to_string()),
        tool_name: None,
        tool_call_id: None,
        tool_input: None,
        tool_is_error: None,
        tool_source: None,
        autonomy_mode: None,
        tool_origin: None,
    }
}

#[test]
fn find_by_idempotency_key_returns_none_when_absent() {
    let db = open_db();
    query::read(&db, |r| {
        assert!(chat_messages::find_by_idempotency_key(r, "missing-key")
            .unwrap()
            .is_none());
        Ok(())
    })
    .unwrap();
}

#[test]
fn find_by_idempotency_key_returns_the_matching_row() {
    let db = open_db();
    let thread_id = Uuid::new_v4();
    let msg_id = Uuid::new_v4();
    db.write(|tx| {
        chat_messages::insert_message(
            tx,
            &sample_message(msg_id, thread_id, "hello", Some("key-1")),
        )
        .unwrap();
        let found = chat_messages::find_by_idempotency_key(tx, "key-1")
            .unwrap()
            .expect("row must be found");
        assert_eq!(found.id, msg_id);
        assert_eq!(found.thread_id, thread_id);
        assert_eq!(found.content, "hello");
        Ok(())
    })
    .unwrap();
}

#[test]
fn reusing_an_idempotency_key_on_a_second_insert_violates_the_unique_index() {
    let db = open_db();
    db.write(|tx| {
        chat_messages::insert_message(
            tx,
            &sample_message(Uuid::new_v4(), Uuid::new_v4(), "first", Some("dup-key")),
        )
        .unwrap();
        let err = chat_messages::insert_message(
            tx,
            &sample_message(Uuid::new_v4(), Uuid::new_v4(), "second", Some("dup-key")),
        )
        .expect_err("second insert with the same idempotency_key must fail");
        assert!(format!("{err}").contains("UNIQUE constraint failed"));
        Ok(())
    })
    .unwrap();
}

#[test]
fn multiple_null_idempotency_keys_are_allowed() {
    let db = open_db();
    db.write(|tx| {
        chat_messages::insert_message(
            tx,
            &sample_message(Uuid::new_v4(), Uuid::new_v4(), "assistant reply 1", None),
        )
        .unwrap();
        chat_messages::insert_message(
            tx,
            &sample_message(Uuid::new_v4(), Uuid::new_v4(), "assistant reply 2", None),
        )
        .unwrap();
        Ok(())
    })
    .unwrap();
}

#[test]
fn resolve_fresh_key_yields_the_deterministic_ids() {
    let db = open_db();
    let (expected_user, expected_assistant) = derive_message_ids("fresh-key");
    query::read(&db, |r| {
        let decision = resolve_idempotent_send(r, "fresh-key", None, "hi there").unwrap();
        assert_eq!(
            decision,
            IdempotentSend::Fresh {
                user_message_id: expected_user,
                assistant_message_id: expected_assistant,
            }
        );
        Ok(())
    })
    .unwrap();
}

#[test]
fn resolve_duplicate_key_with_matching_thread_and_content_reuses_ids() {
    let db = open_db();
    let thread_id = Uuid::new_v4();
    let (user_id, assistant_id) = derive_message_ids("retry-key");
    db.write(|tx| {
        chat_messages::insert_message(
            tx,
            &sample_message(user_id, thread_id, "same content", Some("retry-key")),
        )
        .unwrap();

        let decision =
            resolve_idempotent_send(tx, "retry-key", Some(thread_id), "same content").unwrap();
        assert_eq!(
            decision,
            IdempotentSend::Duplicate {
                thread_id,
                user_message_id: user_id,
                assistant_message_id: assistant_id,
            }
        );
        Ok(())
    })
    .unwrap();
}

#[test]
fn resolve_duplicate_key_without_a_requested_thread_does_not_mismatch() {
    // The retry-triggering scenario: the frontend never learned the
    // resolved thread id from the first (uncertain) call, so it resends
    // `threadId: None` — that must NOT be treated as a mismatch.
    let db = open_db();
    let thread_id = Uuid::new_v4();
    let (user_id, _assistant_id) = derive_message_ids("no-thread-key");
    db.write(|tx| {
        chat_messages::insert_message(
            tx,
            &sample_message(user_id, thread_id, "content", Some("no-thread-key")),
        )
        .unwrap();

        let decision = resolve_idempotent_send(tx, "no-thread-key", None, "content").unwrap();
        assert!(matches!(decision, IdempotentSend::Duplicate { .. }));
        Ok(())
    })
    .unwrap();
}

#[test]
fn resolve_duplicate_key_with_mismatched_content_is_invalid_input() {
    let db = open_db();
    let thread_id = Uuid::new_v4();
    let (user_id, _) = derive_message_ids("mismatch-content-key");
    db.write(|tx| {
        chat_messages::insert_message(
            tx,
            &sample_message(user_id, thread_id, "original", Some("mismatch-content-key")),
        )
        .unwrap();

        let decision = resolve_idempotent_send(
            tx,
            "mismatch-content-key",
            Some(thread_id),
            "different content",
        )
        .unwrap();
        assert_eq!(decision, IdempotentSend::Mismatch);
        Ok(())
    })
    .unwrap();
}

#[test]
fn resolve_duplicate_key_with_mismatched_thread_is_invalid_input() {
    let db = open_db();
    let thread_id = Uuid::new_v4();
    let other_thread_id = Uuid::new_v4();
    let (user_id, _) = derive_message_ids("mismatch-thread-key");
    db.write(|tx| {
        chat_messages::insert_message(
            tx,
            &sample_message(user_id, thread_id, "content", Some("mismatch-thread-key")),
        )
        .unwrap();

        let decision =
            resolve_idempotent_send(tx, "mismatch-thread-key", Some(other_thread_id), "content")
                .unwrap();
        assert_eq!(decision, IdempotentSend::Mismatch);
        Ok(())
    })
    .unwrap();
}

#[test]
fn a_null_thread_id_always_starts_a_new_thread() {
    let db = open_db();
    db.write(|tx| {
        let first = persist_send_transaction(
            tx,
            "new-chat-1",
            None,
            "first conversation",
            None,
            None,
            "model-a",
            1,
        )
        .unwrap();
        let second = persist_send_transaction(
            tx,
            "new-chat-2",
            None,
            "second conversation",
            None,
            None,
            "model-a",
            2,
        )
        .unwrap();

        let first_thread = match first {
            PersistedSend::Fresh { thread_id, .. } => thread_id,
            other => panic!("expected first send to be fresh, got {other:?}"),
        };
        let second_thread = match second {
            PersistedSend::Fresh { thread_id, .. } => thread_id,
            other => panic!("expected second send to be fresh, got {other:?}"),
        };
        assert_ne!(first_thread, second_thread);

        let first_messages = chat_messages::list_messages(tx, first_thread).unwrap();
        let second_messages = chat_messages::list_messages(tx, second_thread).unwrap();
        assert_eq!(first_messages.len(), 1);
        assert_eq!(second_messages.len(), 1);
        assert_eq!(first_messages[0].content, "first conversation");
        assert_eq!(second_messages[0].content, "second conversation");
        Ok(())
    })
    .unwrap();
}

#[test]
fn legacy_key_retries_keep_the_original_assistant_id() {
    let db = open_db();
    let key = "legacy-key";
    let thread_id = Uuid::new_v4();
    let legacy_user_id = Uuid::new_v5(&Uuid::NAMESPACE_OID, key.as_bytes());
    let legacy_assistant_id =
        Uuid::new_v5(&Uuid::NAMESPACE_OID, format!("{key}:assistant").as_bytes());
    db.write(|tx| {
        chat_messages::insert_message(
            tx,
            &sample_message(legacy_user_id, thread_id, "legacy content", Some(key)),
        )
        .unwrap();

        assert_eq!(
            resolve_idempotent_send(tx, key, Some(thread_id), "legacy content").unwrap(),
            IdempotentSend::Duplicate {
                thread_id,
                user_message_id: legacy_user_id,
                assistant_message_id: legacy_assistant_id,
            }
        );
        Ok(())
    })
    .unwrap();
}

#[test]
fn concurrent_same_key_sends_return_one_fresh_and_one_duplicate() {
    let db = Arc::new(open_db());
    let start = Arc::new(Barrier::new(2));
    let handles = (0..2)
        .map(|_| {
            let db = Arc::clone(&db);
            let start = Arc::clone(&start);
            std::thread::spawn(move || {
                start.wait();
                db.write(|tx| {
                    persist_send_transaction(
                        tx,
                        "concurrent-key",
                        None,
                        "same content",
                        None,
                        None,
                        "test-model",
                        1,
                    )
                })
                .unwrap()
            })
        })
        .collect::<Vec<_>>();
    let results = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect::<Vec<_>>();

    let fresh = results.iter().find_map(|result| match result {
        PersistedSend::Fresh {
            thread_id,
            user_message_id,
            assistant_message_id,
        } => Some((*thread_id, *user_message_id, *assistant_message_id)),
        _ => None,
    });
    let duplicate = results.iter().find_map(|result| match result {
        PersistedSend::Duplicate {
            thread_id,
            user_message_id,
            assistant_message_id,
        } => Some((*thread_id, *user_message_id, *assistant_message_id)),
        _ => None,
    });
    assert!(fresh.is_some(), "one concurrent send must reserve the key");
    assert_eq!(
        duplicate, fresh,
        "the loser must receive the winner's result"
    );

    db.with_connection(|conn| {
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM chat_messages WHERE idempotency_key = ?1",
            ["concurrent-key"],
            |row| row.get(0),
        )?;
        assert_eq!(count, 1);
        Ok(())
    })
    .unwrap();
}

#[test]
fn child_messages_sort_after_parents_when_clock_moves_backwards() {
    let tmp = tempfile::tempdir().unwrap();
    let db = Database::open(make_config(
        tmp.path().join("vault.db"),
        installation_id_path(tmp.path()),
    ))
    .unwrap();
    let thread = Uuid::new_v4();
    let mut parent = sample_message(Uuid::new_v4(), thread, "previous response", None);
    parent.created_at = 5000;
    let mut child = sample_message(Uuid::new_v4(), thread, "next turn", None);
    child.parent_id = Some(parent.id);
    child.created_at = 1000;
    db.write(|tx| {
        chat_messages::insert_message(tx, &parent)?;
        chat_messages::insert_message(tx, &child)?;
        let rows = chat_messages::list_messages(tx, thread)?;
        assert_eq!(
            rows.iter().map(|m| m.id).collect::<Vec<_>>(),
            vec![parent.id, child.id]
        );
        assert!(rows[1].created_at > rows[0].created_at);
        Ok(())
    })
    .unwrap();
}

#[test]
fn a_send_naming_an_unknown_thread_is_rejected_instead_of_orphaning_rows() {
    let db = open_db();
    db.write(|tx| {
        let missing_thread_id = Uuid::new_v4();
        let decision = persist_send_transaction(
            tx,
            "unknown-thread-1",
            Some(missing_thread_id),
            "message for a thread that was never created",
            None,
            None,
            "model-a",
            1,
        )
        .unwrap();

        assert_eq!(
            decision,
            PersistedSend::UnknownThread {
                thread_id: missing_thread_id
            }
        );
        assert!(
            chat_messages::list_messages(tx, missing_thread_id)
                .unwrap()
                .is_empty(),
            "a rejected send must not leave message rows under the unknown thread"
        );
        assert!(
            chat_messages::find_by_idempotency_key(tx, "unknown-thread-1")
                .unwrap()
                .is_none(),
            "a rejected send must not reserve its idempotency key"
        );
        Ok(())
    })
    .unwrap();
}
