//! The setup of the remote storage tests of the bridge (spec 038): a vault with an identity, the
//! probe extension installed, the fake provider in the host for S3 and holzi's WebDAV client for
//! WebDAV (spec 050), two S3 storages the user made in the settings, and events that answer
//! storage dialogs as a test sets them. [`Setup::add_webdav`] adds a storage on a WebDAV server
//! of the test process ([`WebdavServer`]).

use std::path::Path;
use std::sync::{Arc, Mutex};

use serde_json::Value;
use tokio::sync::oneshot;
use zeroize::Zeroizing;

use crate::extensions::bridge::blocking::block_on;
use crate::extensions::bridge::dispatch::{call as bridge_call, CallContext, Emit};
use crate::extensions::commands::permissions::{set, PermissionSetArgs};
use crate::extensions::error::BridgeError;
use crate::extensions::host::ExtensionHost;
use crate::extensions::ids::storage_vault_id;
use crate::extensions::registry::effective::effective_bundle;
use crate::extensions::registry::install::install;
use crate::extensions::remote_storage_dialog::{StorageAnswer, StorageTrial};
use crate::passwords::service::PasswordsService;
use crate::remote_storage::address::Resolver;
use crate::remote_storage::model::{ConnectionInput, CredentialsInput, StorageInput};
use crate::remote_storage::protocol::ProtocolStore;
use crate::remote_storage::service::StorageService;
use crate::remote_storage::sftp::{test_server as sftp_server, SftpStore};
use crate::remote_storage::test_support::{resolver, vault, FakeStore};
use crate::remote_storage::webdav::{test_server, WebDavStore};
use crate::remote_storage::{Addressing, ObjectStore, Protocol, ProviderKind};
use crate::storage::known_devices;
use crate::storage::query::Query;
use crate::vault_gate::VaultDb;

pub(crate) const ACCESS_KEY: &str = "AKIDLEAKMARKER";
pub(crate) const SECRET: &str = "secret-leak-marker-5f1e";
pub(crate) const ENDPOINT: &str = "http://127.0.0.1:9000";
pub(crate) const REGION: &str = "region-leak-marker";

/// Records events and answers storage dialogs with the answers set for the test, one after the
/// other, as holzi's windows would after a failed test.
pub(crate) struct Events {
    pub host: Arc<ExtensionHost>,
    pub seen: Mutex<Vec<(String, Value)>>,
    pub answers: Mutex<Vec<StorageAnswer>>,
    /// What holzi's window learned of each answer with credentials.
    pub trials: Mutex<Vec<oneshot::Receiver<StorageTrial>>>,
    /// Close the frame instead of answering.
    pub close_frame: Mutex<bool>,
}

impl Emit for Events {
    fn emit(&self, event: &str, payload: Value) {
        self.seen
            .lock()
            .expect("lock")
            .push((event.to_owned(), payload.clone()));
        if event != "extension-storage-request" {
            return;
        }
        let request_id = payload["requestId"].as_str().unwrap_or_default().to_owned();
        if *self.close_frame.lock().expect("lock") {
            self.host
                .drop_dialogs_of(payload["frame"].as_str().unwrap_or_default());
            return;
        }
        let mut answers = self.answers.lock().expect("lock").clone();
        if answers.is_empty() {
            answers.push(StorageAnswer::Cancel);
        }
        for answer in answers {
            if let Some(trial) = self.host.storage.resolve(&request_id, answer) {
                self.trials.lock().expect("lock").push(trial);
            }
        }
    }
}

impl Events {
    pub fn dialogs(&self) -> Vec<Value> {
        self.seen
            .lock()
            .expect("lock")
            .iter()
            .filter(|(event, _)| event == "extension-storage-request")
            .map(|(_, payload)| payload.clone())
            .collect()
    }

    pub fn answer_with(&self, answer: StorageAnswer) {
        self.answer_in_turn(vec![answer]);
    }

    pub fn answer_in_turn(&self, answers: Vec<StorageAnswer>) {
        *self.answers.lock().expect("lock") = answers;
    }

    /// What holzi's window learned of each answer with credentials so far.
    pub fn trials(&self) -> Vec<Option<StorageTrial>> {
        self.trials
            .lock()
            .expect("lock")
            .drain(..)
            .map(|mut trial| trial.try_recv().ok())
            .collect()
    }
}

pub(crate) struct Setup {
    _dir: tempfile::TempDir,
    pub vault: VaultDb,
    pub fake: Arc<FakeStore>,
    /// The fake for S3 and holzi's WebDAV client, as the host's storage calls see them.
    pub store: Arc<dyn ObjectStore>,
    pub events: Arc<Events>,
    pub ctx: CallContext,
    /// Two storages the user made in the settings, on one connection.
    pub photos: String,
    pub notes: String,
}

pub(crate) fn credentials() -> CredentialsInput {
    CredentialsInput::S3 {
        access_key_id: ACCESS_KEY.to_owned(),
        secret_access_key: Zeroizing::new(SECRET.to_owned()),
        session_token: None,
    }
}

pub(crate) fn setup() -> Setup {
    setup_with(resolver(&[]))
}

/// The setup with `names` as the resolver of the host's storage calls.
pub(crate) fn setup_with(names: Arc<dyn Resolver>) -> Setup {
    let (dir, vault) = vault();
    vault
        .write_blocking(|tx| crate::sync::keys::ensure_vault_identity(tx, true).map(|_| ()))
        .expect("vault identity");
    let device = vault
        .read_blocking(|q| known_devices::list_devices(q))
        .expect("devices")[0]
        .vault_device_uuid;
    let bytes = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/extension_bundles/good-minimal.xt"),
    )
    .expect("fixture");
    let extension = install(&vault, &bytes, vec![], false, device, 1)
        .expect("install")
        .ids
        .extension_id;
    let bundle = vault
        .read_blocking(move |q| effective_bundle(q, extension).map_err(Into::into))
        .expect("bundle")
        .expect("effective")
        .bundle_id;
    let fake = Arc::new(FakeStore::new());
    let store: Arc<dyn ObjectStore> = Arc::new(ProtocolStore::new(
        fake.clone(),
        Some(Arc::new(WebDavStore::new(resolver(&[])))),
        Some(Arc::new(SftpStore::new(resolver(&[])))),
    ));
    let host = Arc::new(ExtensionHost::default());
    host.storage.set(store.clone(), names);
    let events = Arc::new(Events {
        host: Arc::clone(&host),
        seen: Mutex::new(Vec::new()),
        answers: Mutex::new(Vec::new()),
        trials: Mutex::new(Vec::new()),
        close_frame: Mutex::new(false),
    });
    let service = StorageService::new(
        vault.clone(),
        PasswordsService::new(vault.clone()),
        store.clone(),
        resolver(&[]),
    );
    let connection = block_on(service.save_connection(ConnectionInput {
        id: None,
        protocol: Protocol::S3,
        provider_name: "RustFS".to_owned(),
        provider_kind: ProviderKind::Rustfs,
        endpoint: Some(ENDPOINT.to_owned()),
        region: REGION.to_owned(),
        addressing: Addressing::Path,
        credentials: Some(credentials()),
        bucket_for_test: "photos".to_owned(),
        create_folder: false,
        host_key: None,
    }))
    .expect("connection");
    let storage = |name: &str, bucket: &str| {
        block_on(service.save_storage(StorageInput {
            id: None,
            connection_id: connection.id.clone(),
            name: name.to_owned(),
            bucket: bucket.to_owned(),
            create_folder: false,
        }))
        .expect("storage")
        .id
    };
    let photos = storage("Fotos", "photos");
    let notes = storage("Notizen", "notes");
    Setup {
        _dir: dir,
        ctx: CallContext {
            db: vault.clone(),
            session: host.frames.open(extension, bundle, "tab"),
            host,
            device,
            emitter: events.clone(),
        },
        vault,
        fake,
        store,
        events,
        photos,
        notes,
    }
}

/// A WebDAV server of the test process (spec 050 research R14), on a runtime of its own so it
/// keeps serving between the bridge's calls; it ends with this value.
pub(crate) struct WebdavServer {
    _runtime: tokio::runtime::Runtime,
    pub endpoint: String,
}

impl WebdavServer {
    /// A server with the empty folder `folder`.
    pub fn start(folder: &str) -> Self {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("runtime");
        let access = runtime.block_on(test_server::start(folder));
        Self {
            _runtime: runtime,
            endpoint: access.location.endpoint,
        }
    }
}

pub(crate) const WEBDAV_USER: &str = "user-leak-marker";
pub(crate) const WEBDAV_PASSWORD: &str = "password-leak-marker-9c2d";

/// An SFTP server of the test process (spec 050 research R14) with the empty folder `folder`, its
/// own runtime and a login whose parts are leak markers.
pub(crate) struct SftpServer {
    _runtime: tokio::runtime::Runtime,
    pub server: sftp_server::TestServer,
}

impl SftpServer {
    pub fn start(folder: &str) -> Self {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("runtime");
        let mut options = sftp_server::Options::new(42);
        options.username = WEBDAV_USER.to_owned();
        options.password = Some(WEBDAV_PASSWORD.to_owned());
        let server = runtime.block_on(sftp_server::start(options));
        std::fs::create_dir_all(server.path(folder)).expect("folder");
        Self {
            _runtime: runtime,
            server,
        }
    }
}

impl Setup {
    /// A WebDAV connection to `server` and a storage `name` on its folder `folder`, made as the
    /// user would in the settings; returns the storage's id.
    pub fn add_webdav(&self, server: &WebdavServer, name: &str, folder: &str) -> String {
        let service = StorageService::new(
            self.vault.clone(),
            PasswordsService::new(self.vault.clone()),
            self.store.clone(),
            resolver(&[]),
        );
        let connection = block_on(service.save_connection(ConnectionInput {
            id: None,
            protocol: Protocol::WebDav,
            provider_name: "Server".to_owned(),
            provider_kind: ProviderKind::Other,
            endpoint: Some(server.endpoint.clone()),
            region: String::new(),
            addressing: Addressing::Path,
            credentials: Some(CredentialsInput::Password {
                username: WEBDAV_USER.to_owned(),
                password: Zeroizing::new(WEBDAV_PASSWORD.to_owned()),
            }),
            bucket_for_test: folder.to_owned(),
            create_folder: false,
            host_key: None,
        }))
        .expect("webdav connection");
        block_on(service.save_storage(StorageInput {
            id: None,
            connection_id: connection.id,
            name: name.to_owned(),
            bucket: folder.to_owned(),
            create_folder: false,
        }))
        .expect("webdav storage")
        .id
    }

    /// An SFTP connection to `server` with its confirmed host key and a storage `name` on its
    /// folder `folder`; returns the storage's id.
    pub fn add_sftp(&self, server: &SftpServer, name: &str, folder: &str) -> String {
        let service = StorageService::new(
            self.vault.clone(),
            PasswordsService::new(self.vault.clone()),
            self.store.clone(),
            resolver(&[]),
        );
        let connection = block_on(service.save_connection(ConnectionInput {
            id: None,
            protocol: Protocol::Sftp,
            provider_name: "SFTP".to_owned(),
            provider_kind: ProviderKind::Other,
            endpoint: Some(server.server.endpoint()),
            region: String::new(),
            addressing: Addressing::Path,
            credentials: Some(CredentialsInput::Password {
                username: WEBDAV_USER.to_owned(),
                password: Zeroizing::new(WEBDAV_PASSWORD.to_owned()),
            }),
            bucket_for_test: folder.to_owned(),
            create_folder: false,
            host_key: Some(server.server.host_key.clone()),
        }))
        .expect("sftp connection");
        block_on(service.save_storage(StorageInput {
            id: None,
            connection_id: connection.id,
            name: name.to_owned(),
            bucket: folder.to_owned(),
            create_folder: false,
        }))
        .expect("sftp storage")
        .id
    }

    pub fn call(&self, method: &str, params: Value) -> Result<Value, BridgeError> {
        bridge_call(&self.ctx, method, &params)
    }

    pub fn code(&self, method: &str, params: Value) -> u16 {
        self.call(method, params)
            .map_or_else(|e| e.code.as_u16(), |_| 0)
    }

    pub fn permit(&self, action: &str, target: &str, status: &str) {
        set(
            &self.vault,
            self.ctx.device,
            PermissionSetArgs {
                extension_id: self.ctx.session.extension_id.to_string(),
                kind: "remoteStorage".into(),
                action: action.into(),
                target: target.into(),
                status: status.into(),
                replaces: None,
            },
            0,
        )
        .expect("permit");
    }

    /// The extension's area in every bucket.
    pub fn prefix(&self) -> String {
        let pubkey = self
            .vault
            .read_blocking(|q| crate::sync::keys::vault_pubkey(q))
            .expect("read")
            .expect("identity");
        format!(
            "holzi-ext/{}/{}/",
            storage_vault_id(&pubkey),
            self.ctx.session.extension_id
        )
    }

    pub fn count(&self, sql: &str) -> i64 {
        let sql = sql.to_owned();
        self.vault
            .read_blocking(move |q| Ok(q.query_row(&sql, &[], |r| r.get(0))?.unwrap_or(0)))
            .expect("count")
    }
}
