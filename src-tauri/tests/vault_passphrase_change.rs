//! Changing the vault passphrase (spec 042 US3, FR-012, FR-013, SC-004) against a real SQLCipher
//! file, over the mock runtime like `vault_single_session.rs`: after a change the old passphrase
//! no longer opens the vault, the new one does and the data is still there; a wrong current
//! passphrase, a too short or an unchanged new one change nothing.
//!
//! Linux only: storage is redirected through `XDG_DATA_HOME`, which Tauri's `AppLocalData`
//! honours only on Linux.
#![cfg(target_os = "linux")]

use std::sync::LazyLock;

use tauri::test::{mock_builder, mock_context, noop_assets};
use tokio::sync::Mutex;

use holzi_lib::chat::session::ChatState;
use holzi_lib::instances::{
    change_vault_passphrase_core, create_instance_core, open_instance_core,
};
use holzi_lib::state::AppState;
use holzi_lib::storage::preferences::{self, PrefScope};
use holzi_lib::vault_gate::VaultGate;
use holzi_lib::HaexError;

/// `XDG_DATA_HOME` is process-wide, so tests that set it take turns.
static TURN: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

const OLD: &str = "correct-horse-battery";
const NEW: &str = "staple-another-horse";
const MARKER: &str = "test.passphrase_change";

fn mock_app() -> tauri::AppHandle<tauri::test::MockRuntime> {
    mock_builder()
        .build(mock_context(noop_assets()))
        .expect("mock app")
        .handle()
        .clone()
}

/// One app process: its own gate, state and chat state.
struct Process {
    app: tauri::AppHandle<tauri::test::MockRuntime>,
    state: AppState,
    chat: ChatState,
}

impl Process {
    fn new() -> Self {
        let gate = VaultGate::new();
        Self {
            app: mock_app(),
            state: AppState::new(gate.clone()),
            chat: ChatState::with_children(gate.children()),
        }
    }

    async fn open(&self, passphrase: &str) -> Result<(), HaexError> {
        open_instance_core(
            &self.app,
            &self.state,
            &self.chat,
            "vault",
            passphrase.into(),
        )
        .await
        .map(|_| ())
    }

    async fn change(&self, current: &str, new: &str) -> Result<(), HaexError> {
        change_vault_passphrase_core(
            &self.app,
            &self.state,
            &self.chat,
            current.into(),
            new.into(),
        )
        .await
    }

    async fn marker(&self) -> Option<String> {
        let db = self.state.database().expect("active database");
        db.read(|r| preferences::get(r, PrefScope::Vault, MARKER))
            .await
            .expect("read the marker")
    }

    /// Releases haex-crdt's advisory file lock, as the process ending would.
    fn close(&self) {
        drop(self.state.take().expect("take the handle"));
    }
}

/// Creates the vault under `OLD` with one preference row, then closes it.
async fn create_vault() {
    let creator = Process::new();
    create_instance_core(
        &creator.app,
        &creator.state,
        &creator.chat,
        "vault",
        OLD.into(),
    )
    .await
    .expect("create the vault");
    let db = creator.state.database().expect("active database");
    db.write(|tx| preferences::insert_or_update(tx, PrefScope::Vault, MARKER, "kept").map(|_| ()))
        .await
        .expect("write the marker");
    creator.close();
}

#[tokio::test]
async fn after_a_change_only_the_new_passphrase_opens_the_vault_and_the_data_stays() {
    let _turn = TURN.lock().await;
    let data_home = tempfile::tempdir().expect("data home");
    std::env::set_var("XDG_DATA_HOME", data_home.path());
    create_vault().await;

    let process = Process::new();
    process
        .open(OLD)
        .await
        .expect("open with the old passphrase");
    process
        .change(OLD, NEW)
        .await
        .expect("change the passphrase");
    // The open session keeps working on the re-keyed file.
    assert_eq!(process.marker().await.as_deref(), Some("kept"));
    process.close();

    let old = Process::new();
    let result = old.open(OLD).await;
    assert!(
        matches!(result, Err(HaexError::WrongPassphrase)),
        "the old passphrase still opens the vault: {result:?}"
    );

    let new = Process::new();
    new.open(NEW).await.expect("open with the new passphrase");
    assert_eq!(new.marker().await.as_deref(), Some("kept"));
    new.close();
}

#[tokio::test]
async fn a_wrong_current_passphrase_changes_nothing() {
    let _turn = TURN.lock().await;
    let data_home = tempfile::tempdir().expect("data home");
    std::env::set_var("XDG_DATA_HOME", data_home.path());
    create_vault().await;

    let process = Process::new();
    process.open(OLD).await.expect("open");
    let result = process.change("not-the-passphrase", NEW).await;
    assert!(
        matches!(result, Err(HaexError::WrongPassphrase)),
        "expected WrongPassphrase, got {result:?}"
    );
    process.close();

    Process::new()
        .open(OLD)
        .await
        .expect("the old passphrase still opens the vault");
}

#[tokio::test]
async fn a_short_or_unchanged_new_passphrase_is_refused() {
    let _turn = TURN.lock().await;
    let data_home = tempfile::tempdir().expect("data home");
    std::env::set_var("XDG_DATA_HOME", data_home.path());
    create_vault().await;

    let process = Process::new();
    process.open(OLD).await.expect("open");
    for new in ["1234567", OLD] {
        let result = process.change(OLD, new).await;
        assert!(
            matches!(result, Err(HaexError::WeakPassphrase { .. })),
            "expected WeakPassphrase for {new:?}, got {result:?}"
        );
    }
    process.close();

    Process::new()
        .open(OLD)
        .await
        .expect("the old passphrase still opens the vault");
}

#[tokio::test]
async fn a_change_without_an_open_vault_is_refused() {
    let _turn = TURN.lock().await;
    let data_home = tempfile::tempdir().expect("data home");
    std::env::set_var("XDG_DATA_HOME", data_home.path());

    let result = Process::new().change(OLD, NEW).await;
    assert!(
        matches!(result, Err(HaexError::NoActiveInstance)),
        "expected NoActiveInstance, got {result:?}"
    );
}
