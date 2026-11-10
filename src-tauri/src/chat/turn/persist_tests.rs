use super::*;

use crate::adapters::cli_delegate::autonomy::AutonomyMode;
use crate::storage::chat_threads::ChatThread;

#[test]
fn autonomy_mode_label_is_none_for_standard() {
    assert_eq!(autonomy_mode_label(AutonomyMode::Standard), None);
}

#[test]
fn autonomy_mode_label_is_some_for_non_standard_modes() {
    assert_eq!(
        autonomy_mode_label(AutonomyMode::Ungated),
        Some("ungated".to_string())
    );
    assert_eq!(
        autonomy_mode_label(AutonomyMode::GatedPermissive),
        Some("gated_permissive".to_string())
    );
}

#[tokio::test]
async fn failed_thread_update_rolls_back_final_assistant_message() {
    let (tmp, db, thread_id) = tokio::task::spawn_blocking(|| {
            let tmp = tempfile::tempdir().unwrap();
            let db = haex_crdt::Database::open(crate::instances::vault_config::vault_config(
                "test-passphrase", &tmp.path().join("vault.db"),
                &crate::identity::installation_id_path(tmp.path()), true,
            )).unwrap();
            let thread_id = Uuid::new_v4();
            db.write(|tx| {
                thread_store::insert_thread(tx, &ChatThread {
                    id: thread_id, title: "Keep me".into(), last_provider_id: None,
                    last_model_id: None, created_at: 0, updated_at: 0, origin: None,
                })
            }).unwrap();
            // A trigger is schema, which the CRDT write path does not create.
            #[allow(clippy::disallowed_methods)]
            db.with_connection(|conn| {
                conn.execute_batch("CREATE TRIGGER reject_thread_update BEFORE UPDATE ON chat_threads BEGIN SELECT RAISE(ABORT, 'injected update failure'); END;")?;
                Ok(())
            }).unwrap();
            let db = crate::vault_gate::VaultGate::new()
                .vault_db(std::sync::Arc::new(db))
                .unwrap();
            (tmp, db, thread_id)
        }).await.unwrap();
    let message = ChatMessage {
        role: MessageRole::Assistant,
        finish_reason: Some(FinishReason::Complete),
        content: "answer".into(),
        ..empty_tool_message(Uuid::new_v4(), thread_id, None)
    };
    assert!(persist_final_message(&db, message, None, "test".into())
        .await
        .is_err());
    tokio::task::spawn_blocking(move || {
        db.read_blocking(|r| {
            assert!(msg_store::list_messages(r, thread_id)?.is_empty());
            assert_eq!(
                thread_store::get_thread(r, thread_id)?.unwrap().title,
                "Keep me"
            );
            Ok(())
        })
        .unwrap();
        drop(db);
        drop(tmp);
    })
    .await
    .unwrap();
}
