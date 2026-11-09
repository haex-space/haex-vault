//! Taking over a vault file (spec 043 FR-002a, contract `import-instance.md`), over the mock runtime
//! so the vault folders are the real ones: the copy opens under a name from the file name, a
//! second copy of the same vault is refused, a wrong passphrase or a file that is no vault leaves
//! no file behind, a taken name gets a number, and the chosen file is only read.
//!
//! Linux only: storage is redirected through `XDG_DATA_HOME`, which Tauri's `AppLocalData`
//! honours only on Linux.
#![cfg(target_os = "linux")]

#[path = "common/chosen_files.rs"]
mod chosen_files;

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use sha2::{Digest, Sha256};
use tauri::test::{mock_builder, mock_context, noop_assets};
use tokio::sync::Mutex;

use chosen_files::{chosen, Paths};
use holzi_lib::chat::session::ChatState;
use holzi_lib::instances::{create_instance_core, import_instance_core};
use holzi_lib::state::AppState;
use holzi_lib::vault_gate::VaultGate;
use holzi_lib::HolziError;

/// `XDG_DATA_HOME` is process-wide, so the tests take turns.
static TURN: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

const PASSPHRASE: &str = "correct-horse-battery";

fn mock_app() -> tauri::AppHandle<tauri::test::MockRuntime> {
    mock_builder()
        .build(mock_context(noop_assets()))
        .expect("mock app")
        .handle()
        .clone()
}

fn session() -> (AppState, ChatState) {
    let gate = VaultGate::new();
    let chat = ChatState::with_children(gate.children());
    (AppState::new(gate), chat)
}

fn vaults_dir(app: &tauri::AppHandle<tauri::test::MockRuntime>) -> PathBuf {
    holzi_lib::instances::paths::vault_paths(app)
        .expect("vault paths")
        .vaults_dir()
}

fn names_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.file_name().into_string().expect("utf-8"))
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

fn checksum(path: &Path) -> Vec<u8> {
    Sha256::digest(std::fs::read(path).expect("read")).to_vec()
}

/// Creates a vault on "another device" (its own data folder) and copies its file to `target`.
async fn vault_file_from_elsewhere(name: &str, target: &Path) {
    let elsewhere = tempfile::tempdir().expect("elsewhere");
    std::env::set_var("XDG_DATA_HOME", elsewhere.path());
    let app = mock_app();
    let (state, chat) = session();
    create_instance_core(&app, &state, &chat, name, PASSPHRASE.into())
        .await
        .expect("create on the other device");
    drop(state.take().expect("take"));
    std::fs::copy(vaults_dir(&app).join(name).join("vault.db"), target).expect("copy out");
}

async fn import(
    app: &tauri::AppHandle<tauri::test::MockRuntime>,
    file: &Path,
    passphrase: &str,
) -> Result<String, HolziError> {
    let (state, chat) = session();
    let result = import_instance_core(app, &state, &chat, Paths, chosen(file), passphrase.into())
        .await
        .map(|result| result.info.name);
    drop(state.take());
    result
}

#[tokio::test]
async fn a_vault_file_opens_once_and_a_second_copy_is_refused() {
    let _turn = TURN.lock().await;
    let files = tempfile::tempdir().expect("files");
    let source = files.path().join("Annas Tresor.db");
    vault_file_from_elsewhere("anna", &source).await;
    let before = checksum(&source);

    let here = tempfile::tempdir().expect("here");
    std::env::set_var("XDG_DATA_HOME", here.path());
    let app = mock_app();

    assert_eq!(
        import(&app, &source, PASSPHRASE).await.expect("import"),
        "Annas-Tresor"
    );
    let dir = vaults_dir(&app);
    assert_eq!(names_in(&dir), ["Annas-Tresor"]);
    let folder = dir.join("Annas-Tresor");
    assert!(names_in(&folder).contains(&"vault.db".to_string()));
    assert!(names_in(&folder).contains(&"vault.db.vault-id".to_string()));
    assert!(!names_in(&folder)
        .iter()
        .any(|name| name.ends_with(".pending")));

    let after_first = names_in(&folder);
    match import(&app, &source, PASSPHRASE).await {
        Err(HolziError::AlreadyOnThisDevice { name }) => assert_eq!(name, "Annas-Tresor"),
        other => panic!("{other:?}"),
    }
    assert_eq!(
        names_in(&dir),
        ["Annas-Tresor"],
        "no folder of the refused copy"
    );
    assert_eq!(
        names_in(&folder),
        after_first,
        "nothing left of the refused copy"
    );
    assert_eq!(checksum(&source), before, "the chosen file is only read");
}

#[tokio::test]
async fn a_wrong_passphrase_or_no_vault_leaves_nothing() {
    let _turn = TURN.lock().await;
    let files = tempfile::tempdir().expect("files");
    let source = files.path().join("vault.db");
    vault_file_from_elsewhere("bert", &source).await;
    let junk = files.path().join("notes.db");
    std::fs::write(&junk, vec![0x42u8; 8192]).expect("junk");

    let here = tempfile::tempdir().expect("here");
    std::env::set_var("XDG_DATA_HOME", here.path());
    let app = mock_app();

    assert!(matches!(
        import(&app, &source, "not-the-passphrase").await,
        Err(HolziError::WrongPassphrase)
    ));
    assert!(matches!(
        import(&app, &junk, PASSPHRASE).await,
        Err(HolziError::WrongPassphrase)
    ));
    assert!(matches!(
        import(&app, &files.path().join("missing.db"), PASSPHRASE).await,
        Err(HolziError::Unreadable)
    ));
    assert_eq!(names_in(&vaults_dir(&app)), Vec::<String>::new());
}

#[tokio::test]
async fn a_taken_name_gets_a_number() {
    let _turn = TURN.lock().await;
    let files = tempfile::tempdir().expect("files");
    let first = files.path().join("first");
    let second = files.path().join("second");
    std::fs::create_dir_all(&first).expect("dir");
    std::fs::create_dir_all(&second).expect("dir");
    vault_file_from_elsewhere("carla", &first.join("work.db")).await;
    vault_file_from_elsewhere("dora", &second.join("work.db")).await;

    let here = tempfile::tempdir().expect("here");
    std::env::set_var("XDG_DATA_HOME", here.path());
    let app = mock_app();

    assert_eq!(
        import(&app, &first.join("work.db"), PASSPHRASE)
            .await
            .expect("first"),
        "work"
    );
    assert_eq!(
        import(&app, &second.join("work.db"), PASSPHRASE)
            .await
            .expect("second"),
        "work-2"
    );
}
