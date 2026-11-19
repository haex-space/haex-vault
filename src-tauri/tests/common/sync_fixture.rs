//! Shared fixture for spec-024 User Story 1 integration tests (T028): real
//! app instances — `AppState` + `VaultGate` + the actual `SyncService`
//! wiring from `instances::create_instance`/`open_instance`, over Tauri's
//! `MockRuntime` — each its own temp data directory, in one process.
//!
//! The main device is created the real way
//! (`instances::create_instance_core`). A linked device's vault is
//! fabricated directly at the storage layer instead: its own empty
//! database, filled by a raw CRDT pull from the main device plus a fresh
//! content-key generation issued to it. This stands in for spec 024 user
//! story 5's link-device flow, which does not exist yet — everything after
//! that point (binding the endpoint, presence, reconnect, handshake,
//! session) runs through the real production entry point,
//! `sync::start_for_active_instance`.
//!
//! Tauri's `AppLocalData` resolution reads `XDG_DATA_HOME` (Linux only, as
//! in `tests/vault_single_session.rs`), which is process-wide. Each
//! `create_main`/`join` call sets it, does all the path-resolving work it
//! needs (creating the instance, starting sync), and returns — nothing an
//! `Instance` starts in the background reads that variable again, so
//! interleaving two instances' setup is safe, but two `create_main`/`join`
//! calls themselves must not run concurrently. Callers serialize with the
//! same kind of `Mutex` `vault_single_session.rs` uses for the same reason.

#![cfg(target_os = "linux")]
#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::Arc;

use haex_crdt::rusqlite::params;
use haex_crdt::Database;
use tauri::test::{mock_builder, mock_context, noop_assets, MockRuntime};
use tauri::AppHandle;
use uuid::Uuid;

use haex_lib::chat::session::ChatState;
use haex_lib::identity::installation_id_path;
use haex_lib::instances::create_instance_core;
use haex_lib::instances::vault_config::vault_config;
use haex_lib::state::{ActiveInstanceHandle, AppState};
use haex_lib::storage::query::Query;
use haex_lib::sync;
use haex_lib::vault_gate::{DrainOutcome, VaultGate};

pub const PASSPHRASE: &str = "correct-horse-battery";

/// A real app instance with the real sync service running on it.
pub struct Instance {
    pub app: AppHandle<MockRuntime>,
    pub state: AppState,
    pub chat: ChatState,
    pub keys: sync::keys::DeviceKeys,
    pub vault: [u8; 32],
    /// The vault file of this instance, what copying it takes.
    pub db_path: PathBuf,
    _data_home: tempfile::TempDir,
}

impl Instance {
    /// The open vault's database handle.
    pub fn database(&self) -> Arc<Database> {
        self.state.database().expect("active instance").database()
    }

    /// Through `VaultDb` (not the raw `Database`), like a real command: only
    /// that path wakes the gate's sync-notify signal presence and the
    /// notify loop (T034/T037) wait on.
    pub async fn write_thread(&self, id: &str, title: &str) {
        let id = id.to_string();
        let title = title.to_string();
        self.state
            .database()
            .expect("active instance")
            .write(move |tx| {
                tx.execute(
                    "INSERT INTO chat_threads (id, title, created_at, updated_at) \
                     VALUES (?1, ?2, 1, 1) \
                     ON CONFLICT(id) DO UPDATE SET title = excluded.title",
                    params![id, title],
                )
            })
            .await
            .expect("write thread");
    }

    /// This installation's device id, the origin of its own changes.
    pub fn device_uuid(&self) -> Uuid {
        self.database().device_id()
    }

    /// The device a chat thread's latest change came from, read off the
    /// node in its row HLC (what a shared space would check for authorship).
    pub fn thread_origin(&self, id: &str) -> Option<Uuid> {
        let hlc: Option<String> = haex_lib::storage::query::read(&self.database(), |r| {
            r.query_row(
                "SELECT haex_hlc_no_sync FROM chat_threads WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
        })
        .expect("read hlc");
        hlc.and_then(|hlc| sync::progress::origin_of(&hlc))
    }

    /// The title of chat thread `id` as this instance currently stores it.
    pub fn thread_title(&self, id: &str) -> Option<String> {
        haex_lib::storage::query::read(&self.database(), |r| {
            r.query_row(
                "SELECT title FROM chat_threads WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
        })
        .expect("read title")
    }

    /// Stops the tracked sync service before the fixture's database is dropped.
    /// A task the drain cannot end would keep running against the dropped vault,
    /// so that fails the test.
    pub async fn shutdown(self) {
        let outcome = self.state.gate().drain().await;
        assert_ne!(
            outcome,
            DrainOutcome::Stuck,
            "a sync task outlived the drain"
        );
    }
}

/// A Tauri app on the mock runtime; its handle resolves the app data paths.
pub fn mock_app() -> AppHandle<MockRuntime> {
    mock_builder()
        .build(mock_context(noop_assets()))
        .expect("mock app")
        .handle()
        .clone()
}

/// Creates the vault fresh (genesis: this becomes the sole main device) and
/// starts the real sync service on it, pointed at `nostr_relay` (a real
/// public default relay would make this test flaky and network-dependent —
/// callers pass an in-process `MockRelay`'s URL instead).
pub async fn create_main(vault_name: &str, nostr_relay: &str) -> Instance {
    let data_home = tempfile::tempdir().expect("tempdir");
    std::env::set_var("XDG_DATA_HOME", data_home.path());
    let app = mock_app();
    let gate = VaultGate::new();
    let state = AppState::new(gate.clone());
    let chat = ChatState::with_children(gate.children());
    create_instance_core(&app, &state, &chat, vault_name, PASSPHRASE.into())
        .await
        .expect("create instance");
    let db_path =
        haex_lib::instances::paths::get_instance_path(&app, vault_name).expect("vault path");

    let db = state.database().expect("active instance").database();
    let (keys, vault) = haex_lib::storage::query::read(&db, |r| {
        let keys = sync::keys::load_device_keys(r, read_installation_uuid(&app))?
            .expect("device keys after genesis");
        let vault = sync::keys::vault_pubkey(r)?.expect("vault identity after genesis");
        Ok((keys, vault))
    })
    .expect("read genesis state");
    set_nostr_relay(&db, nostr_relay);

    sync::start_for_active_instance(&app, &state).await;
    Instance {
        app,
        state,
        chat,
        keys,
        vault,
        db_path,
        _data_home: data_home,
    }
}

/// A second device sharing `main`'s vault (see module docs for how, absent
/// a link-device flow), with the real sync service started on it too.
pub async fn join(main: &Instance, nostr_relay: &str) -> Instance {
    let data_home = tempfile::tempdir().expect("tempdir");
    std::env::set_var("XDG_DATA_HOME", data_home.path());
    let app = mock_app();
    let installation_uuid = read_installation_uuid(&app);
    let installation_id_file = installation_id_path(
        &haex_lib::instances::paths::get_app_local_data(&app).expect("app local data"),
    );

    let db_path = data_home.path().join("linked.db");
    let config = vault_config(PASSPHRASE, &db_path, &installation_id_file, true);
    let db = Arc::new(Database::open(config).expect("open empty database"));

    pull_all(main, &db);
    let keys = db
        .write(|tx| sync::keys::ensure_device_keys(tx, installation_uuid, 2))
        .expect("device keys");

    // Stand-in for linking (module docs): main issues the next device-list
    // generation naming the new device, and a content-key generation
    // wrapped for everyone still listed, then the linked device pulls and
    // unwraps its own envelope. Through `VaultDb` (not the raw `Database`),
    // like a real command: main's already-running sync service (and its
    // presence loop) only wakes for this on that path.
    let main_vault = main.vault;
    let main_keys = main.keys.clone();
    let linked_keys = keys.clone();
    let linked_vault_device_uuid = db.device_id();
    main.state
        .database()
        .expect("active instance")
        .write(move |tx| {
            let rows = sync::device_list::load_all(tx)?;
            let valid = sync::device_list::valid_lists(&rows, &main_vault);
            let effective = sync::device_list::effective(&valid)
                .expect("a list")
                .clone();
            let next = sync::device_list::DeviceList {
                generation: effective.list.generation + 1,
                base_list_hash: Some(effective.hash),
                issued_by: main_keys.device_pubkey,
                devices: {
                    let mut devices = effective.list.devices.clone();
                    devices.push(sync::device_list::ListedDevice {
                        device_pubkey: linked_keys.device_pubkey,
                        endpoint_id: linked_keys.endpoint_id,
                        role: sync::device_list::Role::Linked,
                        vault_device_uuid: linked_vault_device_uuid,
                        name_sealed: Vec::new(),
                        added_at: 2,
                    });
                    devices
                },
                ..effective.list.clone()
            };
            let vault_secret = sync::keys::vault_secret(tx)?.expect("a main device");
            let signed = sync::device_list::sign_list(next, &vault_secret)
                .map_err(haex_crdt::Error::consumer)?;
            sync::device_list::insert(tx, &signed)?;

            // The vault's current key, wrapped for the new device too, as
            // linking does (no rotation: a device linked earlier would
            // otherwise lose the mailbox the others publish to).
            let key = sync::content_keys::current_key(tx, &[])?.expect("main holds a content key");
            sync::content_keys::issue_generation(tx, &key, &signed, &main_keys, 2)
        })
        .await
        .expect("issue linked device's list and content key");

    pull_all(main, &db);
    db.write(|tx| {
        let rows = sync::device_list::load_all(tx)?;
        let valid = sync::device_list::valid_lists(&rows, &main.vault);
        sync::content_keys::unwrap_own_envelopes(tx, &keys, &valid)?;
        Ok(())
    })
    .expect("unwrap linked device's envelope");
    set_nostr_relay(&db, nostr_relay);

    let state = AppState::new(VaultGate::new());
    let chat = ChatState::with_children(state.gate().children());
    state
        .install(
            ActiveInstanceHandle {
                name: "linked".to_string(),
                database: Arc::clone(&db),
            },
            || Ok(()),
        )
        .expect("install linked instance");

    sync::start_for_active_instance(&app, &state).await;
    Instance {
        app,
        state,
        chat,
        keys,
        vault: main.vault,
        db_path,
        _data_home: data_home,
    }
}

/// The vault file of `source` opened by a new installation, as when the file
/// is copied to another computer and opened there with the same passphrase
/// (spec 024 user story 7): the file is taken as it is while `source` keeps
/// running, the copy gets its own installation id, genesis runs on it the way
/// `open_instance` runs it, and the real sync service starts on it, pointed
/// at `nostr_relay`.
pub async fn copy_of(source: &Instance, nostr_relay: &str) -> Instance {
    // The file on disk must hold everything the source has committed.
    #[allow(clippy::disallowed_methods)]
    source
        .database()
        .with_connection(|conn| {
            Ok(conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))?)
        })
        .expect("checkpoint the source");
    let data_home = tempfile::tempdir().expect("tempdir");
    std::env::set_var("XDG_DATA_HOME", data_home.path());
    let app = mock_app();
    let installation_uuid = read_installation_uuid(&app);
    let installation_id_file = installation_id_path(
        &haex_lib::instances::paths::get_app_local_data(&app).expect("app local data"),
    );
    let db_path = data_home.path().join("copy.db");
    std::fs::copy(&source.db_path, &db_path).expect("copy the vault file");

    let config = vault_config(PASSPHRASE, &db_path, &installation_id_file, false);
    let db = Arc::new(Database::open(config).expect("open the copy"));
    sync::genesis::run_after_open(&db, &installation_id_file, false);
    let (keys, vault) = haex_lib::storage::query::read(&db, |r| {
        let keys = sync::keys::load_device_keys(r, installation_uuid)?
            .expect("the copy created keys of its own");
        let vault = sync::keys::vault_pubkey(r)?.expect("vault identity");
        Ok((keys, vault))
    })
    .expect("read the copy's state");
    set_nostr_relay(&db, nostr_relay);

    let state = AppState::new(VaultGate::new());
    let chat = ChatState::with_children(state.gate().children());
    state
        .install(
            ActiveInstanceHandle {
                name: "copy".to_string(),
                database: Arc::clone(&db),
            },
            || Ok(()),
        )
        .expect("install the copy");
    sync::start_for_active_instance(&app, &state).await;
    Instance {
        app,
        state,
        chat,
        keys,
        vault,
        db_path,
        _data_home: data_home,
    }
}

/// Points this device at a single Nostr relay instead of the built-in
/// public defaults (`sync::servers`), so a test controls exactly which
/// relay presence uses. Also points the iroh side at a closed local port
/// instead of `RelayMode::Default`'s real n0 relays: `SyncNode::bind`'s own
/// doc comment warns it "can hang on resolving iroh-Relays", which a
/// sandboxed test run should not depend on reaching; a closed local port
/// refuses the connection immediately instead of timing out.
fn set_nostr_relay(db: &Database, url: &str) {
    let config = sync::servers::ServerConfig::only(
        vec![url.to_string()],
        vec!["https://127.0.0.1:1".to_string()],
    );
    db.write(|tx| sync::servers::write(tx, &config))
        .expect("set relay preferences");
}

/// This instance's installation UUID, minted on first use.
pub fn read_installation_uuid(app: &AppHandle<MockRuntime>) -> Uuid {
    let app_local_data =
        haex_lib::instances::paths::get_app_local_data(app).expect("app local data");
    haex_lib::identity::read_or_mint_installation_uuid(&installation_id_path(&app_local_data))
        .expect("installation uuid")
}

/// A raw, network-free pull of everything `from` has that `into` lacks —
/// the same primitive two real devices' sessions use (`sync::outbound`/
/// `sync::inbound`), just driven directly instead of over a connection.
pub fn pull_all(from: &Instance, into: &Arc<Database>) {
    let from_replica = sync::replica::Replica::new(from.database());
    let into_replica = sync::replica::Replica::new(Arc::clone(into));
    let theirs = into_replica.progress().expect("read progress");
    let mut outbox =
        sync::outbound::serve_pull_with_budget(&from_replica, &theirs, sync::change::PAGE_BUDGET)
            .expect("serve pull");
    let mut inbox = sync::inbound::Inbox::new();
    while let Some(page) = outbox.next_page() {
        inbox.receive(&into_replica, page).expect("receive page");
    }
}
