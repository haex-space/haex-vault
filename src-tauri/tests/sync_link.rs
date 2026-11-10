//! Integration test for spec 024 User Story 5 (T051): a real main device
//! with its running sync service, and a fresh installation that joins it
//! through the link commands' own code (`LinkHost`, `LinkJoin`), over a real
//! in-process Nostr relay for the rendezvous and real iroh endpoints on
//! loopback for the exchange.
//!
//! Linux only, like `sync_devices.rs` (storage is redirected through
//! `XDG_DATA_HOME`).
#![cfg(target_os = "linux")]

use std::net::Ipv4Addr;
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use haex_crdt::rusqlite::params;
use haex_crdt::Database;
use iroh::RelayMode;
use tokio::sync::Mutex;

use holzi_lib::identity::installation_id_path;
use holzi_lib::instances::paths::{get_app_local_data, get_instance_path};
use holzi_lib::instances::vault_config::vault_config;
use holzi_lib::storage::query::{self, Query};
use holzi_lib::sync::link::host_task::LinkHost;
use holzi_lib::sync::link::join_task::{JoinArgs, JoinConfig, LinkJoin};
use holzi_lib::sync::link::status::{LinkFailure, LinkJoinState, LinkingStage};
use holzi_lib::sync::{device_list, keys};

#[path = "common/sync_fixture.rs"]
mod sync_fixture;
#[path = "common/sync_helpers.rs"]
mod sync_helpers;

use sync_helpers::{listed, runtime_of, until};

/// `XDG_DATA_HOME` is process-wide; tests that set it take turns.
static TURN: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

const NEW_PASSPHRASE: &str = "a-different-passphrase";

/// A fresh installation: its own data directory and app.
struct NewInstallation {
    app: tauri::AppHandle<tauri::test::MockRuntime>,
    join: LinkJoin,
    home: tempfile::TempDir,
}

impl NewInstallation {
    fn new() -> Self {
        let home = tempfile::tempdir().expect("tempdir");
        std::env::set_var("XDG_DATA_HOME", home.path());
        Self {
            app: sync_fixture::mock_app(),
            join: LinkJoin::default(),
            home,
        }
    }

    fn select(&self) {
        std::env::set_var("XDG_DATA_HOME", self.home.path());
    }

    async fn start(
        &self,
        code: &str,
        relay_url: &str,
        search_timeout: Duration,
    ) -> Result<LinkJoinState, holzi_lib::error::HolziError> {
        self.select();
        let emit = Arc::new(|_: &LinkJoinState| {});
        self.join
            .start(
                &self.app,
                emit,
                JoinArgs {
                    code: code.to_string(),
                    vault_name: "linked".to_string(),
                    device_name: "Second laptop".to_string(),
                    passphrase: NEW_PASSPHRASE.into(),
                },
                JoinConfig {
                    nostr_relays: vec![relay_url.to_string()],
                    relay_mode: RelayMode::Disabled,
                    bind_addr: Some((Ipv4Addr::LOCALHOST, 0).into()),
                    search_timeout,
                },
            )
            .await
    }

    /// The linked vault, opened with the passphrase chosen for it.
    fn open_vault(&self) -> Database {
        self.select();
        let path = get_instance_path(&self.app, "linked").expect("path");
        let id_file = installation_id_path(&get_app_local_data(&self.app).expect("data dir"));
        Database::open(vault_config(NEW_PASSPHRASE, &path, &id_file, false)).expect("open linked")
    }

    fn vault_exists(&self) -> bool {
        self.select();
        get_instance_path(&self.app, "linked")
            .expect("path")
            .exists()
    }
}

async fn awaiting(host: &LinkHost, name: &str) {
    until("the main device to ask", || {
        host.status()
            .filter(|s| {
                s.stage == LinkingStage::AwaitingConfirmation
                    && s.new_device_name.as_deref() == Some(name)
            })
            .map(|_| ())
    })
    .await;
}

/// Runs a link to the point of the user's answer, then answers.
async fn link(
    main: &sync_fixture::Instance,
    fresh: &NewInstallation,
    relay_url: &str,
    answer: impl FnOnce(&LinkHost),
) -> LinkJoinState {
    let runtime = runtime_of(main).await;
    let info = runtime
        .link
        .create_code(&runtime, "Test")
        .await
        .expect("a code");
    fresh
        .start(&info.code, relay_url, Duration::from_secs(60))
        .await
        .expect("the join starts");
    awaiting(&runtime.link, "Second laptop").await;
    answer(&runtime.link);
    until("the join to end", || {
        fresh
            .join
            .status()
            .filter(|s| matches!(s, LinkJoinState::Done { .. } | LinkJoinState::Failed { .. }))
    })
    .await
}

/// Spec 085 (US3): the text of the code carries the vault's name and servers, reads back the
/// same, and its code is the one shown.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_text_of_a_code_carries_the_vaults_name_and_servers() {
    let _turn = TURN.lock().await;
    let relay = nostr_sdk::local_relay::MockRelay::run()
        .await
        .expect("mock relay");
    let relay_url = relay.url().await.to_string();
    let main = sync_fixture::create_main("main", &relay_url).await;
    let runtime = runtime_of(&main).await;

    let info = runtime
        .link
        .create_code(&runtime, "Familie")
        .await
        .expect("a code");
    let parsed = holzi_lib::sync::link::text::parse(&info.text).expect("the text reads");

    assert_eq!(parsed.code.display(), info.code);
    assert_eq!(parsed.vault_name.as_deref(), Some("Familie"));
    let servers = parsed.servers.expect("the vault's own servers");
    assert_eq!(servers.nostr_relays, vec![relay_url]);
    assert_eq!(servers.iroh_relays, vec!["https://127.0.0.1:1".to_string()]);
    // The built-in servers are switched off in this vault, and the text says so.
    assert!(!servers.disabled.is_empty());
    assert_eq!(parsed.dropped_servers, 0);
    runtime.link.cancel();
}

/// US5 scenarios 1 to 4 (spec.md), SC-009: a code, a name shown before
/// anything moves, a yes without the main device role: the new installation
/// holds the data, is a linked device on both lists and has no vault secret.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_new_installation_links_as_a_linked_device() {
    let _turn = TURN.lock().await;
    let relay = nostr_sdk::local_relay::MockRelay::run()
        .await
        .expect("mock relay");
    let relay_url = relay.url().await.to_string();
    let main = sync_fixture::create_main("main", &relay_url).await;
    main.write_thread("t1", "from the main device").await;
    let fresh = NewInstallation::new();

    let end = link(&main, &fresh, &relay_url, |host| {
        host.confirm(false).expect("confirm");
    })
    .await;

    assert_eq!(
        end,
        LinkJoinState::Done {
            vault_name: "linked".to_string()
        }
    );
    let vault = fresh.open_vault();
    let (has_secret, title, own_key, list_hash) = query::read(&vault, |r| {
        let secret = keys::vault_secret(r)?.is_some();
        let title: Option<String> = r.query_row(
            "SELECT title FROM chat_threads WHERE id = 't1'",
            params![],
            |row| row.get(0),
        )?;
        let valid = device_list::valid_lists(
            &device_list::load_all(r)?,
            &keys::vault_pubkey(r)?.expect("identity"),
        );
        let devices: Vec<_> = device_list::effective(&valid)
            .map(|s| {
                s.list
                    .devices
                    .iter()
                    .map(|d| (d.device_pubkey, d.role))
                    .collect()
            })
            .unwrap_or_default();
        Ok((
            secret,
            title,
            holzi_lib::sync::envelopes::held_keys(r)?.len(),
            devices,
        ))
    })
    .expect("read the linked vault");
    assert!(!has_secret, "SC-009: no vault secret without the role");
    assert_eq!(title.as_deref(), Some("from the main device"));
    assert!(
        own_key > 0,
        "the content key was wrapped for the new device"
    );
    assert_eq!(list_hash.len(), 2, "both devices are on the list");
    assert!(
        list_hash
            .iter()
            .any(|(_, role)| *role == device_list::Role::Linked),
        "the new device is a linked device"
    );
    assert_eq!(
        listed(&main).len(),
        2,
        "the main device published the list too"
    );

    main.shutdown().await;
}

/// US5 scenario 5, SC-012: with the role, the vault secret arrives over the
/// link and the new device is a main device.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_new_installation_can_link_as_a_main_device() {
    let _turn = TURN.lock().await;
    let relay = nostr_sdk::local_relay::MockRelay::run()
        .await
        .expect("mock relay");
    let relay_url = relay.url().await.to_string();
    let main = sync_fixture::create_main("main", &relay_url).await;
    let fresh = NewInstallation::new();

    let end = link(&main, &fresh, &relay_url, |host| {
        host.confirm(true).expect("confirm");
    })
    .await;

    assert!(matches!(end, LinkJoinState::Done { .. }), "{end:?}");
    let vault = fresh.open_vault();
    let secret = query::read(&vault, |r| keys::vault_secret(r))
        .expect("read")
        .expect("SC-012: the vault secret arrived");
    let main_secret = query::read(&main.database(), |r| keys::vault_secret(r))
        .expect("read")
        .expect("main holds it");
    assert_eq!(*secret, *main_secret);
    assert_eq!(
        listed(&main).iter().filter(|(_, is_main)| *is_main).count(),
        2
    );

    main.shutdown().await;
}

/// US5 scenario 7: a decline gives the new installation nothing, leaves the
/// device list as it was, and its vault is gone.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_declined_link_leaves_nothing_behind() {
    let _turn = TURN.lock().await;
    let relay = nostr_sdk::local_relay::MockRelay::run()
        .await
        .expect("mock relay");
    let relay_url = relay.url().await.to_string();
    let main = sync_fixture::create_main("main", &relay_url).await;
    let fresh = NewInstallation::new();

    let end = link(&main, &fresh, &relay_url, |host| {
        host.reject().expect("reject");
    })
    .await;

    assert_eq!(
        end,
        LinkJoinState::Failed {
            reason: LinkFailure::Rejected
        }
    );
    assert!(!fresh.vault_exists(), "FR-025: no vault stays");
    assert_eq!(listed(&main).len(), 1, "the device list is unchanged");
    assert_eq!(runtime_of(&main).await.link.status(), None);

    main.shutdown().await;
}

/// FR-025: cancelling a join waits for the background task to discard its
/// pending vault before the caller continues.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cancelling_a_join_waits_until_the_pending_vault_is_removed() {
    let _turn = TURN.lock().await;
    let relay = nostr_sdk::local_relay::MockRelay::run()
        .await
        .expect("mock relay");
    let relay_url = relay.url().await.to_string();
    let main = sync_fixture::create_main("main", &relay_url).await;
    let runtime = runtime_of(&main).await;
    let info = runtime
        .link
        .create_code(&runtime, "Test")
        .await
        .expect("a code");
    let fresh = NewInstallation::new();

    fresh
        .start(&info.code, &relay_url, Duration::from_secs(60))
        .await
        .expect("the join starts");
    fresh.join.cancel().await;

    assert_eq!(
        fresh.join.status(),
        Some(LinkJoinState::Failed {
            reason: LinkFailure::ConnectionLost
        })
    );
    assert!(!fresh.vault_exists(), "FR-025: no vault stays after cancel");
    runtime.link.cancel();

    // The node of a runtime held here would outlive the drain.
    drop(runtime);
    main.shutdown().await;
}

/// US5 scenario 6: a code that is mistyped, or already used, transfers
/// nothing and says so.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_wrong_or_used_code_transfers_nothing() {
    let _turn = TURN.lock().await;
    let relay = nostr_sdk::local_relay::MockRelay::run()
        .await
        .expect("mock relay");
    let relay_url = relay.url().await.to_string();
    let main = sync_fixture::create_main("main", &relay_url).await;

    // Not even a code: refused at once, without creating a vault.
    let typo = NewInstallation::new();
    let state = typo
        .start("not a code", &relay_url, Duration::from_secs(1))
        .await
        .expect("the input is accepted, the join fails");
    assert_eq!(
        state,
        LinkJoinState::Failed {
            reason: LinkFailure::WrongCode
        }
    );
    assert!(!typo.vault_exists());

    // A code that was used: link once, then try the same code again.
    let first = NewInstallation::new();
    let runtime = runtime_of(&main).await;
    let info = runtime
        .link
        .create_code(&runtime, "Test")
        .await
        .expect("a code");
    first
        .start(&info.code, &relay_url, Duration::from_secs(60))
        .await
        .expect("starts");
    awaiting(&runtime.link, "Second laptop").await;
    runtime.link.confirm(false).expect("confirm");
    until("the first join", || {
        first
            .join
            .status()
            .filter(|s| matches!(s, LinkJoinState::Done { .. }))
    })
    .await;

    let second = NewInstallation::new();
    second
        .start(&info.code, &relay_url, Duration::from_secs(3))
        .await
        .expect("starts");
    let end = until("the second join to give up", || {
        second
            .join
            .status()
            .filter(|s| matches!(s, LinkJoinState::Failed { .. }))
    })
    .await;
    assert_eq!(
        end,
        LinkJoinState::Failed {
            reason: LinkFailure::Expired
        }
    );
    assert!(
        !second.vault_exists(),
        "nothing was transferred, no vault stays"
    );
    assert_eq!(listed(&main).len(), 2, "only the first device was added");

    // The node of a runtime held here would outlive the drain.
    drop(runtime);
    main.shutdown().await;
}

/// FR-024: a device without the main role cannot show a code, and the
/// backend says so.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_linked_device_cannot_show_a_code() {
    let _turn = TURN.lock().await;
    let relay = nostr_sdk::local_relay::MockRelay::run()
        .await
        .expect("mock relay");
    let relay_url = relay.url().await.to_string();
    let main = sync_fixture::create_main("main", &relay_url).await;
    let linked = sync_fixture::join(&main, &relay_url).await;

    let runtime = runtime_of(&linked).await;
    let refused = runtime.link.create_code(&runtime, "Test").await;

    assert!(matches!(
        refused,
        Err(holzi_lib::error::HolziError::NotMainDevice)
    ));

    // The node of a runtime held here would outlive the drain.
    drop(runtime);
    tokio::join!(main.shutdown(), linked.shutdown());
}
