//! The move into vault folders (spec 084, SC-001, SC-002, SC-005), with the profile and chat
//! files folders that follow their vault with real vaults: a data folder
//! of the old layout with five of them, one named like a device-wide folder, is moved at start and
//! every vault opens afterwards with its passphrase and its identity intact.
//!
//! Linux only: storage is redirected through `XDG_DATA_HOME`, which Tauri's `AppLocalData`
//! honours only on Linux.
#![cfg(target_os = "linux")]

use std::path::Path;

use tauri::test::{mock_builder, mock_context, noop_assets};

use holzi_lib::chat::session::ChatState;
use holzi_lib::instances::layout_move;
use holzi_lib::instances::paths::vault_paths;
use holzi_lib::instances::vault_id;
use holzi_lib::instances::vault_paths::VaultPaths;
use holzi_lib::instances::{create_instance_core, open_instance_core};
use holzi_lib::state::AppState;
use holzi_lib::vault_gate::VaultGate;

const NAMES: [&str; 5] = ["anna", "ben", "carla", "filters", "models"];

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

fn passphrase(name: &str) -> String {
    format!("passphrase of {name}")
}

/// Puts the files of the vault `name` where the old layout had them.
fn make_it_old(paths: &VaultPaths, name: &str) {
    let folder = paths.folder(name);
    std::fs::create_dir_all(paths.legacy_dir()).expect("instances folder");
    for entry in std::fs::read_dir(&folder).expect("vault folder").flatten() {
        let file = entry.file_name().into_string().expect("utf-8");
        let rest = file
            .strip_prefix("vault.db")
            .expect("a sqlite or marker file");
        std::fs::rename(
            entry.path(),
            paths.legacy_dir().join(format!("{name}.db{rest}")),
        )
        .expect("move back");
    }
    std::fs::remove_dir(&folder).expect("empty folder");
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

#[tokio::test]
async fn five_vaults_of_the_old_layout_open_after_the_move() {
    let here = tempfile::tempdir().expect("data folder");
    std::env::set_var("XDG_DATA_HOME", here.path());
    let app = mock_app();
    let paths = vault_paths(&app).expect("paths");

    // Five vaults on disk, then written back into the old layout.
    let mut identities = Vec::new();
    for name in NAMES {
        let (state, chat) = session();
        create_instance_core(&app, &state, &chat, name, passphrase(name).as_str().into())
            .await
            .expect("create");
        let db = state.database().expect("active");
        identities.push(vault_id::fingerprint(&db).expect("identity"));
        drop(db);
        drop(state.take().expect("take"));
        make_it_old(&paths, name);
    }
    std::fs::remove_dir(paths.vaults_dir()).expect("no vault folder is left");
    assert!(names_in(&paths.legacy_dir()).len() >= NAMES.len());

    // The profile of one vault and the chat files of another lie at their old places, and so do
    // the tracker lists, which lay in the folder `agent-browser/filters/` of the vault `filters`.
    let put = |path: std::path::PathBuf| {
        std::fs::create_dir_all(path.parent().expect("parent")).expect("folder");
        std::fs::write(path, "x").expect("file");
    };
    put(paths
        .legacy_agent_browser("anna")
        .join("Profil/profile.zip"));
    put(paths.legacy_chat_files("models").join("Chat/file"));
    put(paths.legacy_agent_browser("filters").join("settings.json"));
    put(paths
        .legacy_agent_browser("filters")
        .join("Profil/profile.zip"));

    let outcome = layout_move::run(&paths).expect("move");

    assert_eq!(outcome.moved, NAMES);
    assert!(outcome.left.is_empty(), "{:?}", outcome.left);
    assert!(
        !paths.legacy_dir().exists(),
        "no database lies in instances/"
    );
    assert_eq!(names_in(&paths.vaults_dir()), NAMES);
    assert!(paths
        .agent_browser("anna")
        .join("Profil/profile.zip")
        .is_file());
    assert!(paths.chat_files("models").join("Chat/file").is_file());
    assert!(paths
        .agent_browser("filters")
        .join("Profil/profile.zip")
        .is_file());
    assert!(!paths
        .agent_browser("filters")
        .join("settings.json")
        .exists());
    assert!(
        paths.filter_lists().join("settings.json").is_file(),
        "the tracker lists went to their own folder"
    );
    assert!(!paths
        .legacy_agent_browser("filters")
        .join("settings.json")
        .exists());
    assert!(!paths.legacy_agent_browser("anna").exists());
    for name in NAMES {
        assert!(paths.database(name).is_file(), "{name}");
    }

    for (name, identity) in NAMES.iter().zip(&identities) {
        let (state, chat) = session();
        open_instance_core(&app, &state, &chat, name, passphrase(name).as_str().into())
            .await
            .unwrap_or_else(|error| panic!("{name} opens: {error:?}"));
        let db = state.database().expect("active");
        assert_eq!(
            vault_id::fingerprint(&db).as_ref(),
            Some(identity),
            "{name} keeps its identity"
        );
        drop(db);
        drop(state.take().expect("take"));
    }

    // A second start has nothing left to do.
    assert_eq!(layout_move::run(&paths).expect("again"), Default::default());
}
