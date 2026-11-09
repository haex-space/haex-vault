//! Persistence contract tests for chat thread management (spec 006).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use haex_crdt::{Database, DatabaseConfig, NoopSignatureProvider, SqlCipherKey};
use uuid::Uuid;

use holzi_lib::identity::{holzi_migration_source, installation_id_path, HolziBootstrap};
use holzi_lib::storage::chat_messages::{self, ChatMessage, FinishReason, MessageRole};
use holzi_lib::storage::chat_threads::{self, ChatThread};
use holzi_lib::storage::query;

const PASSPHRASE: &str = "chat-thread-management";

fn open_vault(dir: &Path) -> Database {
    Database::open(DatabaseConfig {
        path: PathBuf::from(dir).join("vault.db"),
        key: SqlCipherKey::new(PASSPHRASE),
        create_if_missing: true,
        bootstrap: Arc::new(HolziBootstrap::new(installation_id_path(dir)).with_alias("test")),
        signature_provider: Arc::new(NoopSignatureProvider),
        migration_source: holzi_migration_source(),
        trigger_version: haex_crdt::DEFAULT_TRIGGER_VERSION,
        max_transaction_bytes: haex_crdt::MAX_CRDT_TRANSACTION_BYTES,
        max_value_bytes: haex_crdt::MAX_VALUE_BYTES,
    })
    .expect("vault open")
}

fn thread(id: Uuid) -> ChatThread {
    ChatThread {
        id,
        title: "Original title".to_string(),
        last_provider_id: None,
        last_model_id: None,
        created_at: 1_000,
        updated_at: 2_000,
        origin: None,
    }
}

fn message(thread_id: Uuid) -> ChatMessage {
    ChatMessage {
        id: Uuid::new_v4(),
        thread_id,
        parent_id: None,
        role: MessageRole::User,
        content: "message".to_string(),
        provider_id: None,
        model_id: None,
        prompt_tokens: None,
        completion_tokens: None,
        finish_reason: Some(FinishReason::Complete),
        created_at: 1_001,
        idempotency_key: None,
        tool_name: None,
        tool_call_id: None,
        tool_input: None,
        tool_is_error: None,
        tool_source: None,
        autonomy_mode: None,
        tool_origin: None,
    }
}

/// Installs a trigger that makes one kind of write fail. A trigger is schema, which the CRDT
/// write path does not create.
fn reject_with_trigger(db: &Database, sql: &str) {
    #[allow(clippy::disallowed_methods)]
    db.with_connection(|conn| Ok(conn.execute_batch(sql)?))
        .expect("install trigger");
}

#[test]
fn renaming_changes_only_the_title() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = open_vault(dir.path());
    let id = Uuid::new_v4();

    db.write(|tx| {
        chat_threads::insert_thread(tx, &thread(id))?;
        assert_eq!(chat_threads::rename_title(tx, id, "Renamed")?, 1);
        let saved = chat_threads::get_thread(tx, id)?.expect("thread remains");
        assert_eq!(saved.title, "Renamed");
        assert_eq!(saved.created_at, 1_000);
        assert_eq!(saved.updated_at, 2_000);
        Ok(())
    })
    .expect("rename succeeds");
}

#[test]
fn deleting_a_thread_removes_all_messages_as_one_action() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = open_vault(dir.path());
    let id = Uuid::new_v4();
    let other_id = Uuid::new_v4();

    db.write(|tx| {
        chat_threads::insert_thread(tx, &thread(id))?;
        chat_messages::insert_message(tx, &message(id))?;
        chat_threads::insert_thread(tx, &thread(other_id))?;
        chat_messages::insert_message(tx, &message(other_id))?;
        assert!(chat_threads::delete_thread_and_messages(tx, id)?);
        assert!(chat_threads::get_thread(tx, id)?.is_none());
        assert!(chat_messages::list_messages(tx, id)?.is_empty());
        assert!(chat_threads::get_thread(tx, other_id)?.is_some());
        assert_eq!(chat_messages::list_messages(tx, other_id)?.len(), 1);
        Ok(())
    })
    .expect("delete succeeds");
}

#[test]
fn renaming_a_missing_thread_is_non_mutating() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = open_vault(dir.path());

    db.write(|tx| {
        assert_eq!(chat_threads::rename_title(tx, Uuid::new_v4(), "New")?, 0);
        Ok(())
    })
    .expect("missing rename is handled");
}

#[test]
fn failed_thread_rename_preserves_the_original_title() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = open_vault(dir.path());
    let id = Uuid::new_v4();

    db.write(|tx| chat_threads::insert_thread(tx, &thread(id)))
        .expect("seed thread");
    reject_with_trigger(
        &db,
        "CREATE TRIGGER reject_thread_update BEFORE UPDATE ON chat_threads
         BEGIN SELECT RAISE(ABORT, 'injected update failure'); END;",
    );

    assert!(db
        .write(|tx| chat_threads::rename_title(tx, id, "New"))
        .is_err());
    let saved = query::read(&db, |r| chat_threads::get_thread(r, id))
        .expect("read thread")
        .expect("thread remains");
    assert_eq!(saved.title, "Original title");
    assert_eq!(saved.created_at, 1_000);
}

#[test]
fn deleting_a_missing_thread_is_non_mutating() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = open_vault(dir.path());

    db.write(|tx| {
        assert!(!chat_threads::delete_thread_and_messages(
            tx,
            Uuid::new_v4()
        )?);
        Ok(())
    })
    .expect("missing delete is handled");
}

#[test]
fn failed_thread_delete_rolls_back_message_removal() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = open_vault(dir.path());
    let id = Uuid::new_v4();

    db.write(|tx| {
        chat_threads::insert_thread(tx, &thread(id))?;
        chat_messages::insert_message(tx, &message(id))
    })
    .expect("seed thread and message");
    reject_with_trigger(
        &db,
        "CREATE TRIGGER reject_thread_delete BEFORE DELETE ON chat_threads
         BEGIN SELECT RAISE(ABORT, 'injected delete failure'); END;",
    );

    assert!(db
        .write(|tx| chat_threads::delete_thread_and_messages(tx, id))
        .is_err());
    query::read(&db, |r| {
        assert!(chat_threads::get_thread(r, id)?.is_some());
        assert_eq!(chat_messages::list_messages(r, id)?.len(), 1);
        Ok(())
    })
    .expect("rollback preserves the thread");
}
