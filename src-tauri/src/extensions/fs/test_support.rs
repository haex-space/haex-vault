//! Test support of the filesystem tests: a vault, an extension host with a fake dialog and viewer,
//! and a granted folder that holds one of holzi's protected places.

// The setup sets permissions and creates files directly.
#![allow(clippy::disallowed_methods)]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde_json::Value;
use uuid::Uuid;

use super::dialogs::DialogRequest;
use super::{lock, FileDialogs, FsEnvironment};
use crate::extensions::bridge::dispatch::{CallContext, Emit};
use crate::extensions::bridge::events::FRAME_EVENT;
use crate::extensions::host::ExtensionHost;
use crate::extensions::permissions::store::{self as permission_store, NewPermission};
use crate::extensions::permissions::VAULT_WIDE;
use crate::extensions::registry::effective::effective_bundle;
use crate::extensions::registry::install::install;
use crate::files::picked::PickedFile;
use crate::files::test_support::{picked, PathOpener};
use crate::passwords::test_support::open_test_vault;
use crate::storage::known_devices;
use crate::vault_gate::{VaultDb, VaultGate};

#[derive(Default)]
pub(crate) struct Recorded(pub(crate) Mutex<Vec<Value>>);

impl Emit for Recorded {
    fn emit(&self, event: &str, payload: Value) {
        if event == FRAME_EVENT {
            lock(&self.0).push(payload);
        }
    }
}

/// Dialogs that answer with what the test put in, and a viewer that remembers what it opened. A
/// `document` (an address of a provider, as on Android) is answered instead of `choice` by the
/// open and save dialogs.
#[derive(Default)]
pub(crate) struct FakeDialogs {
    pub(crate) choice: Mutex<Option<PathBuf>>,
    pub(crate) document: Mutex<Option<String>>,
    pub(crate) opened: Mutex<Vec<PathBuf>>,
}

impl FakeDialogs {
    fn picked(&self) -> Option<PickedFile> {
        if let Some(address) = lock(&self.document).clone() {
            return Some(PickedFile(address));
        }
        lock(&self.choice).as_deref().map(picked)
    }
}

impl FileDialogs for FakeDialogs {
    fn save(&self, _request: DialogRequest) -> Option<PickedFile> {
        self.picked()
    }
    fn pick_folder(&self, _request: DialogRequest) -> Option<PathBuf> {
        lock(&self.choice).clone()
    }
    fn pick_files(&self, _request: DialogRequest) -> Option<Vec<PickedFile>> {
        self.picked().map(|file| vec![file])
    }
    fn open(&self, path: &Path) -> Result<(), String> {
        lock(&self.opened).push(path.to_path_buf());
        Ok(())
    }
}

pub(crate) struct Setup {
    _vault_dir: tempfile::TempDir,
    _files: tempfile::TempDir,
    /// A folder the tests grant, holding a protected folder of holzi.
    pub(crate) root: PathBuf,
    pub(crate) outside: PathBuf,
    pub(crate) protected: PathBuf,
    pub(crate) scratch: PathBuf,
    /// Where the documents of the fake provider live (`content://provider/<name>`).
    pub(crate) documents: PathBuf,
    pub(crate) dialogs: Arc<FakeDialogs>,
    pub(crate) recorded: Arc<Recorded>,
    pub(crate) host: Arc<ExtensionHost>,
    pub(crate) vault: VaultDb,
    pub(crate) device: Uuid,
}

pub(crate) fn setup() -> Setup {
    let (vault_dir, db) = open_test_vault();
    let vault = VaultGate::new().vault_db(Arc::new(db)).unwrap();
    let device = vault
        .read_blocking(|q| known_devices::list_devices(q))
        .unwrap()[0]
        .vault_device_uuid;
    let files = tempfile::tempdir().unwrap();
    let base = std::fs::canonicalize(files.path()).unwrap();
    let (root, outside) = (base.join("granted"), base.join("outside"));
    let (protected, scratch) = (root.join("haex-data"), base.join("scratch"));
    let documents = base.join("provider");
    for dir in [&root, &outside, &protected, &documents] {
        std::fs::create_dir_all(dir).unwrap();
    }
    std::fs::write(root.join("note.txt"), "inside").unwrap();
    std::fs::write(outside.join("secret.txt"), "outside").unwrap();
    std::fs::write(protected.join("vault.db"), "vault").unwrap();
    let dialogs = Arc::new(FakeDialogs::default());
    let host = Arc::new(ExtensionHost::default());
    host.fs.set_environment(FsEnvironment {
        denied: crate::files::local::OwnPlaces::new(vec![protected.clone()]),
        known: vec![("home", base.clone())],
        dialogs: Arc::clone(&dialogs) as Arc<dyn FileDialogs>,
        scratch: scratch.clone(),
        free_paths: true,
        opener: Arc::new(PathOpener {
            roots: None,
            documents: Some(documents.clone()),
        }),
    });
    Setup {
        _vault_dir: vault_dir,
        _files: files,
        root,
        outside,
        protected,
        scratch,
        documents,
        dialogs,
        recorded: Arc::new(Recorded::default()),
        host,
        vault,
        device,
    }
}

impl Setup {
    /// A frame of the fixture extension `name` (installed on first use).
    pub(crate) fn frame(&self, name: &str) -> CallContext {
        let bytes = std::fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/extension_bundles")
                .join(name),
        )
        .unwrap();
        let id = install(&self.vault, &bytes, vec![], false, self.device, 1)
            .unwrap()
            .ids
            .extension_id;
        let bundle = self
            .vault
            .read_blocking(move |q| effective_bundle(q, id).map_err(Into::into))
            .unwrap()
            .unwrap()
            .bundle_id;
        CallContext {
            db: self.vault.clone(),
            host: Arc::clone(&self.host),
            session: self.host.frames.open(id, bundle, "tab"),
            device: self.device,
            emitter: Arc::clone(&self.recorded) as Arc<dyn Emit>,
        }
    }

    pub(crate) fn grant(&self, ctx: &CallContext, action: &str, target: &Path) {
        self.remember(ctx, action, target, "granted");
    }

    /// Remembers a `filesystem` permission of the frame's extension with `status` for every
    /// device; the same key again changes its status.
    pub(crate) fn remember(&self, ctx: &CallContext, action: &str, target: &Path, status: &str) {
        let (id, target) = (
            ctx.session.extension_id,
            target.to_string_lossy().into_owned(),
        );
        let (action, status) = (action.to_owned(), status.to_owned());
        self.vault
            .write_blocking(move |tx| {
                permission_store::put(
                    tx,
                    id,
                    &NewPermission {
                        kind: "filesystem",
                        action: &action,
                        target: &target,
                        status: &status,
                        declared: false,
                        vault_device_uuid: VAULT_WIDE,
                    },
                    1,
                )
                .map(drop)
                .map_err(Into::into)
            })
            .unwrap();
    }
}
