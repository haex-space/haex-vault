//! The tracker and DNS choices belong to the vault (spec 084, T055, SC-004) with real vaults: two
//! vaults opened in turn each keep their own choices, the old files of the device go into the first
//! vault that is opened, and the DNS row is not read by another device.
//!
//! Linux only: storage is redirected through `XDG_DATA_HOME`, which Tauri's `AppLocalData`
//! honours only on Linux.
#![cfg(target_os = "linux")]

use std::sync::Arc;

use async_trait::async_trait;
use tauri::test::{mock_builder, mock_context, noop_assets};
use tauri::Manager;
use uuid::Uuid;

use holzi_lib::agent_browser::choices::{
    apply_vault_choices, dns, read_dns, read_trackers, set_dns, set_trackers,
};
use holzi_lib::agent_browser::dns::{DnsMode, DnsProvider, DnsSettings};
use holzi_lib::agent_browser::filter_fetch::ListFetcher;
use holzi_lib::agent_browser::filter_lists::{FilterLists, TrackerLists};
use holzi_lib::agent_browser::filter_settings::{CustomList, TrackerSettings};
use holzi_lib::chat::session::ChatState;
use holzi_lib::identity::VAULT_SCOPE_UUID;
use holzi_lib::instances::paths::vault_paths;
use holzi_lib::instances::{create_instance_core, open_instance_core};
use holzi_lib::state::AppState;
use holzi_lib::storage::query::Query;
use holzi_lib::vault_gate::{VaultDb, VaultGate};

/// Never reaches the network: every list is unavailable.
struct Offline;

#[async_trait]
impl ListFetcher for Offline {
    async fn get(&self, _url: &str) -> Result<String, String> {
        Err("offline".to_owned())
    }
}

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

fn tracker_choice(name: &str) -> TrackerSettings {
    TrackerSettings {
        enabled: false,
        off: ["easylist".to_owned()].into(),
        custom: vec![CustomList {
            id: name.to_owned(),
            name: name.to_owned(),
            url: format!("https://lists.example/{name}.txt"),
            enabled: true,
        }],
    }
}

fn dns_choice(provider: DnsProvider) -> DnsSettings {
    DnsSettings {
        mode: DnsMode::Secure,
        provider,
        custom: String::new(),
    }
}

/// Opens `name` as the only vault of a fresh session and lets it apply its choices, as the open
/// path does.
async fn open(app: &tauri::AppHandle<tauri::test::MockRuntime>, name: &str) -> (AppState, VaultDb) {
    let (state, chat) = session();
    open_instance_core(app, &state, &chat, name, passphrase(name).as_str().into())
        .await
        .unwrap_or_else(|error| panic!("{name} opens: {error:?}"));
    let db = state.database().expect("active");
    apply_vault_choices(app, &db).await;
    (state, db)
}

fn close(state: AppState, db: VaultDb) {
    drop(db);
    drop(state.take().expect("take"));
}

#[tokio::test]
async fn two_vaults_keep_their_own_choices_and_the_old_files_go_to_the_first() {
    let here = tempfile::tempdir().expect("data folder");
    std::env::set_var("XDG_DATA_HOME", here.path());
    let app = mock_app();
    let paths = vault_paths(&app).expect("paths");
    let lists = FilterLists::new(paths.filter_lists(), Arc::new(Offline));
    app.manage(TrackerLists(Some(lists.clone())));

    for name in ["anna", "ben"] {
        let (state, chat) = session();
        create_instance_core(&app, &state, &chat, name, passphrase(name).as_str().into())
            .await
            .expect("create");
        drop(state.take().expect("take"));
    }

    // The files of the time before the vault held the choices.
    let old_trackers = paths.filter_lists().join("settings.json");
    let old_dns = paths.agent_browser_root().join("dns.json");
    std::fs::create_dir_all(old_trackers.parent().unwrap()).unwrap();
    std::fs::create_dir_all(old_dns.parent().unwrap()).unwrap();
    std::fs::write(
        &old_trackers,
        serde_json::to_string(&tracker_choice("old")).unwrap(),
    )
    .unwrap();
    std::fs::write(
        &old_dns,
        serde_json::to_string(&dns_choice(DnsProvider::Mullvad)).unwrap(),
    )
    .unwrap();

    // The first vault opened takes them and the files are gone; the second starts from defaults.
    let (state, anna) = open(&app, "anna").await;
    assert_eq!(lists.settings(), tracker_choice("old"));
    assert_eq!(dns(&anna).await.unwrap(), dns_choice(DnsProvider::Mullvad));
    assert!(!old_trackers.exists() && !old_dns.exists());
    close(state, anna);

    let (state, ben) = open(&app, "ben").await;
    assert_eq!(lists.settings(), TrackerSettings::default());
    assert_eq!(dns(&ben).await.unwrap(), DnsSettings::default());

    // Ben chooses differently; Anna's choices stay as they were.
    set_trackers(&lists, &ben, tracker_choice("ben"))
        .await
        .unwrap();
    set_dns(&ben, dns_choice(DnsProvider::Google))
        .await
        .unwrap();
    assert_eq!(lists.settings(), tracker_choice("ben"));
    close(state, ben);

    let (state, anna) = open(&app, "anna").await;
    assert_eq!(lists.settings(), tracker_choice("old"), "Anna's own choice");
    assert_eq!(dns(&anna).await.unwrap(), dns_choice(DnsProvider::Mullvad));
    close(state, anna);

    let (state, ben) = open(&app, "ben").await;
    assert_eq!(lists.settings(), tracker_choice("ben"), "Ben's own choice");
    assert_eq!(dns(&ben).await.unwrap(), dns_choice(DnsProvider::Google));

    // The tracker row is vault-wide, the DNS row is read by the device that wrote it only.
    let device = ben.device_id();
    let (scope, other_device_dns, own_dns) = ben
        .read(move |q| {
            let scope = q.query_row(
                "SELECT vault_device_uuid FROM preferences WHERE key = 'agentBrowser.trackers'",
                &[],
                |row| row.get::<_, String>(0),
            )?;
            Ok((scope, read_dns(q, Uuid::new_v4())?, read_dns(q, device)?))
        })
        .await
        .unwrap();
    assert_eq!(
        scope.as_deref(),
        Some(VAULT_SCOPE_UUID.to_string().as_str())
    );
    assert_eq!(other_device_dns, None);
    assert_eq!(own_dns, Some(dns_choice(DnsProvider::Google)));
    assert_eq!(
        ben.read(|q| read_trackers(q)).await.unwrap(),
        Some(tracker_choice("ben"))
    );
    close(state, ben);
}
