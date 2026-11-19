//! Tests for `wm_session_commands`'s core logic (spec 022-session-restore,
//! contracts/wm-session.md §1; one vault value since spec 023, FR-024). Declared as a child module (`#[path]` in that
//! file) so it can call the private `async fn`s directly. A real `VaultDb`
//! comes from `AppState::install`, the same route a live command takes.

use std::sync::Arc;

use haex_crdt::{Database, DatabaseConfig, NoopSignatureProvider, SqlCipherKey};
use serde_json::json;
use uuid::Uuid;

use super::*;
use crate::identity::{haex_migration_source, HaexBootstrap, HAEX_TRIGGER_VERSION};
use crate::state::{ActiveInstanceHandle, AppState};
use crate::storage::query;

fn open_test_state(passphrase: &str) -> (tempfile::TempDir, AppState, Uuid) {
    let dir = tempfile::tempdir().expect("tempdir");
    let install_path = installation_id_path(dir.path());
    let db = Arc::new(
        Database::open(DatabaseConfig {
            path: dir.path().join("vault.db"),
            key: SqlCipherKey::new(passphrase),
            create_if_missing: true,
            bootstrap: Arc::new(HaexBootstrap::new(install_path.clone())),
            signature_provider: Arc::new(NoopSignatureProvider),
            migration_source: haex_migration_source(),
            trigger_version: HAEX_TRIGGER_VERSION,
            max_transaction_bytes: haex_crdt::MAX_CRDT_TRANSACTION_BYTES,
            max_value_bytes: haex_crdt::MAX_VALUE_BYTES,
        })
        .expect("open test vault"),
    );
    let installation_uuid =
        read_or_mint_installation_uuid(&install_path).expect("installation uuid");
    let device = query::read(&db, |r| {
        known_devices::get_vault_device_uuid(r, installation_uuid)
    })
    .expect("read vault_device_uuid")
    .expect("bootstrap registered this installation");

    let state = AppState::default();
    state
        .install(
            ActiveInstanceHandle {
                name: "wm-session-commands-test".to_string(),
                database: Arc::clone(&db),
            },
            || Ok(()),
        )
        .expect("install the test vault");
    (dir, state, device)
}

fn db_of(state: &AppState) -> VaultDb {
    state.database().expect("active vault")
}

fn snapshot(marker: &str) -> serde_json::Value {
    json!({ "version": 1, "workspaces": [], "windows": [], "activeWorkspaceId": marker })
}

async fn turn(state: &AppState, device: Uuid, enabled: bool) -> SessionRestoreState {
    restore_set(db_of(state), device, SessionRestoreSetArgs { enabled })
        .await
        .expect("set")
}

async fn stored_session(state: &AppState, device: Uuid) -> Option<serde_json::Value> {
    let db = db_of(state);
    tauri::async_runtime::spawn_blocking(move || {
        query::read(&db, |r| Ok(wm_session::load(r, device).expect("load"))).expect("load")
    })
    .await
    .expect("join")
}

#[tokio::test]
async fn restore_is_off_on_a_fresh_vault() {
    let (_dir, state, _device) = open_test_state("wm-session-cmd-default");
    let restore = restore_get(db_of(&state)).await.expect("get");
    assert_eq!(restore, SessionRestoreState { enabled: false });
}

#[tokio::test]
async fn turning_restore_on_and_off_writes_the_vault_value() {
    let (_dir, state, device) = open_test_state("wm-session-cmd-vault");
    assert_eq!(
        turn(&state, device, true).await,
        SessionRestoreState { enabled: true }
    );
    assert_eq!(
        restore_get(db_of(&state)).await.expect("get"),
        SessionRestoreState { enabled: true }
    );
    assert_eq!(
        turn(&state, device, false).await,
        SessionRestoreState { enabled: false }
    );
    assert_eq!(
        restore_get(db_of(&state)).await.expect("get"),
        SessionRestoreState { enabled: false }
    );
}

#[tokio::test]
async fn an_old_device_value_no_longer_applies() {
    let (_dir, state, device) = open_test_state("wm-session-cmd-old-device-value");
    let db = db_of(&state);
    tauri::async_runtime::spawn_blocking(move || {
        db.write_blocking(|tx| {
            preferences::insert_or_update(
                tx,
                PrefScope::Device(device),
                SESSION_RESTORE_KEY,
                "true",
            )
        })
        .expect("set device value")
    })
    .await
    .expect("join");
    let restore = restore_get(db_of(&state)).await.expect("get");
    assert!(!restore.enabled);
}

#[tokio::test]
async fn turning_restore_off_deletes_the_saved_session_in_the_same_call() {
    let (_dir, state, device) = open_test_state("wm-session-cmd-off");
    turn(&state, device, true).await;
    let saved = session_save(db_of(&state), device, snapshot("a"))
        .await
        .expect("save");
    assert!(saved.saved);
    assert!(stored_session(&state, device).await.is_some());

    turn(&state, device, false).await;
    assert_eq!(stored_session(&state, device).await, None);
}

#[tokio::test]
async fn load_with_restore_off_deletes_a_leftover_session() {
    let (_dir, state, device) = open_test_state("wm-session-cmd-load-off");
    let db = db_of(&state);
    tauri::async_runtime::spawn_blocking(move || {
        db.write_blocking(|tx| {
            wm_session::save(tx, device, &snapshot("leftover")).expect("save");
            Ok(())
        })
        .expect("save leftover")
    })
    .await
    .expect("join");

    let loaded = session_load(db_of(&state), device).await.expect("load");
    assert!(!loaded.restore.enabled);
    assert_eq!(loaded.session, None);
    assert_eq!(stored_session(&state, device).await, None);
}

#[tokio::test]
async fn load_with_restore_on_returns_the_saved_session() {
    let (_dir, state, device) = open_test_state("wm-session-cmd-load-on");
    turn(&state, device, true).await;
    session_save(db_of(&state), device, snapshot("kept"))
        .await
        .expect("save");

    let loaded = session_load(db_of(&state), device).await.expect("load");
    assert!(loaded.restore.enabled);
    assert_eq!(loaded.session, Some(snapshot("kept")));
}

#[tokio::test]
async fn save_with_restore_off_writes_nothing() {
    let (_dir, state, device) = open_test_state("wm-session-cmd-save-off");
    let saved = session_save(db_of(&state), device, snapshot("late"))
        .await
        .expect("save");
    assert!(!saved.saved);
    assert_eq!(stored_session(&state, device).await, None);
}

#[tokio::test]
async fn an_oversized_session_is_rejected_as_too_large() {
    let (_dir, state, device) = open_test_state("wm-session-cmd-too-large");
    turn(&state, device, true).await;
    let big = json!({ "version": 1, "padding": "x".repeat(wm_session::MAX_SESSION_BYTES) });
    let result = session_save(db_of(&state), device, big).await;
    assert!(matches!(result, Err(HaexError::SessionTooLarge { .. })));
}

#[tokio::test]
async fn two_devices_keep_separate_sessions_under_one_vault_value() {
    let (_dir, state, device) = open_test_state("wm-session-cmd-two-devices");
    let other = Uuid::new_v4();
    turn(&state, device, true).await;
    session_save(db_of(&state), device, snapshot("first"))
        .await
        .expect("save first");
    session_save(db_of(&state), other, snapshot("second"))
        .await
        .expect("save second");

    let first = session_load(db_of(&state), device)
        .await
        .expect("load first");
    let second = session_load(db_of(&state), other)
        .await
        .expect("load second");
    assert_eq!(first.session, Some(snapshot("first")));
    assert_eq!(second.session, Some(snapshot("second")));

    // Turned off on one device: its session goes now, the other's on its next load.
    turn(&state, device, false).await;
    assert_eq!(stored_session(&state, device).await, None);
    let second = session_load(db_of(&state), other)
        .await
        .expect("load second again");
    assert_eq!(second.session, None);
    assert_eq!(stored_session(&state, other).await, None);
}
