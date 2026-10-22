//! Shared fixtures for the sync unit tests.

use std::path::Path;
use std::sync::Arc;

use haex_crdt::Database;

use crate::identity::installation_id_path;
use crate::instances::vault_config::vault_config;
use crate::sync::change::PAGE_BUDGET;
use crate::sync::inbound::{InboundError, Inbox, Received};
use crate::sync::outbound::serve_pull_with_budget;
use crate::sync::replica::Replica;

/// A freshly migrated vault under `dir`, without the post-open sync step.
pub fn open_vault(dir: &Path) -> Database {
    open_vault_with_limit(dir, haex_crdt::MAX_CRDT_TRANSACTION_BYTES)
}

/// [`open_vault`] with a transaction size limit of `limit` bytes.
pub fn open_vault_with_limit(dir: &Path, limit: usize) -> Database {
    let mut config = vault_config(
        "sync-test-passphrase",
        &dir.join("vault.db"),
        &installation_id_path(dir),
        true,
    );
    config.max_transaction_bytes = limit;
    Database::open(config).expect("open vault")
}

/// A device: its own directory, vault file and installation.
pub struct Device {
    _dir: tempfile::TempDir,
    pub replica: Arc<Replica>,
    database: Arc<Database>,
}

impl Device {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let database = Arc::new(open_vault(dir.path()));
        let replica = Arc::new(Replica::new(Arc::clone(&database)));
        Self {
            _dir: dir,
            replica,
            database,
        }
    }

    /// A device whose replica counts its work with `gate`, as the sync service's does.
    pub fn with_gate(gate: crate::vault_gate::VaultGate) -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let database = Arc::new(open_vault(dir.path()));
        let replica = Arc::new(Replica::tracked(Arc::clone(&database), gate));
        Self {
            _dir: dir,
            replica,
            database,
        }
    }

    pub fn db(&self) -> &Database {
        self.replica.db()
    }

    /// A request handle on this device's vault, as the app's commands get one.
    pub fn vault_db(&self) -> crate::vault_gate::VaultDb {
        crate::vault_gate::VaultGate::new()
            .vault_db(Arc::clone(&self.database))
            .expect("a vault handle")
    }

    /// The directory holding this device's vault file.
    pub fn dir(&self) -> &Path {
        self._dir.path()
    }

    /// Pulls everything `from` has and this device lacks, as one pull.
    pub fn pull_from(&self, from: &Device) -> Vec<Received> {
        self.try_pull_from(from, PAGE_BUDGET).expect("pull")
    }

    /// [`Device::pull_from`] with pages of `budget` bytes.
    pub fn try_pull_from(
        &self,
        from: &Device,
        budget: usize,
    ) -> Result<Vec<Received>, InboundError> {
        pull_into(&self.replica, from, budget)
    }
}

/// Pulls everything `from` has and `replica` lacks, as one pull with pages of `budget` bytes.
pub fn pull_into(
    replica: &Replica,
    from: &Device,
    budget: usize,
) -> Result<Vec<Received>, InboundError> {
    let (theirs, floors) = replica.pull_vector()?;
    let mut outbox = serve_pull_with_budget(&from.replica, &theirs, budget)?;
    let mut inbox = Inbox::new().refetching(floors);
    let mut received = Vec::new();
    while let Some(page) = outbox.next_page() {
        received.push(inbox.receive(replica, page)?);
    }
    Ok(received)
}

/// A device of a vault together with its keys.
pub struct Member {
    pub device: Device,
    pub keys: crate::sync::keys::DeviceKeys,
    pub vault: [u8; 32],
}

impl Member {
    /// How the member presents itself in a handshake.
    pub fn local(&self) -> crate::sync::handshake::Local<'_> {
        crate::sync::handshake::Local {
            keys: &self.keys,
            vault: self.vault,
            schema: crate::sync::handshake::local_schema(),
        }
    }

    /// The first device of a fresh vault: main device on list generation 1.
    pub fn genesis() -> Self {
        let device = Device::new();
        let installation = uuid::Uuid::new_v4();
        let state = crate::sync::genesis::ensure_sync_state(device.db(), installation, true)
            .expect("genesis");
        let keys = crate::storage::query::read(device.db(), |r| {
            crate::sync::keys::load_device_keys(r, installation)
        })
        .expect("read keys")
        .expect("keys");
        Self {
            device,
            keys,
            vault: state.vault_pubkey.expect("identity"),
        }
    }

    /// A second installation of `main`'s vault: it holds `main`'s data and
    /// its own keys, but no list names it yet.
    pub fn join(main: &Member) -> Self {
        let device = Device::new();
        device.pull_from(&main.device);
        let keys = device
            .db()
            .write(|tx| crate::sync::keys::ensure_device_keys(tx, uuid::Uuid::new_v4(), 1))
            .expect("keys");
        Self {
            device,
            keys,
            vault: main.vault,
        }
    }

    /// A copy of this member's vault file opened by a new installation, as
    /// when the file is copied to another computer: the file is taken as it is
    /// (the source keeps running), the copy gets its own installation id, and
    /// genesis runs on it without permission to mint a vault. Returns the
    /// copy and what genesis found.
    pub fn copy_of(&self) -> (Member, crate::sync::genesis::SyncState) {
        #[allow(clippy::disallowed_methods)]
        self.device
            .db()
            .with_connection(|conn| {
                Ok(conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))?)
            })
            .expect("checkpoint the source");
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::copy(
            self.device.dir().join("vault.db"),
            dir.path().join("vault.db"),
        )
        .expect("copy the vault file");
        let database = Arc::new(open_vault(dir.path()));
        let replica = Arc::new(Replica::new(Arc::clone(&database)));
        let device = Device {
            _dir: dir,
            replica,
            database,
        };
        let installation =
            crate::identity::read_or_mint_installation_uuid(&installation_id_path(device.dir()))
                .expect("installation id");
        let state = crate::sync::genesis::ensure_sync_state(device.db(), installation, false)
            .expect("genesis on the copy");
        let keys = crate::storage::query::read(device.db(), |r| {
            crate::sync::keys::load_device_keys(r, installation)
        })
        .expect("read keys")
        .expect("the copy has keys of its own");
        (
            Member {
                device,
                keys,
                vault: self.vault,
            },
            state,
        )
    }

    /// Issues the next list generation on this main device, built by `edit`
    /// from the effective one.
    pub fn issue_list(
        &self,
        edit: impl FnOnce(crate::sync::device_list::DeviceList) -> crate::sync::device_list::DeviceList,
    ) {
        use crate::sync::{device_list, keys};
        self.device
            .db()
            .write(|tx| {
                let secret = keys::vault_secret(tx)?.expect("a main device");
                let valid = device_list::valid_lists(&device_list::load_all(tx)?, &self.vault);
                let effective = device_list::effective(&valid).expect("a list").clone();
                let next = device_list::DeviceList {
                    generation: effective.list.generation + 1,
                    base_list_hash: Some(effective.hash),
                    issued_by: self.keys.device_pubkey,
                    ..effective.list
                };
                let signed = device_list::sign_list(edit(next), &secret)
                    .map_err(haex_crdt::Error::consumer)?;
                device_list::insert(tx, &signed)
            })
            .expect("issue list");
    }

    /// Lists `other` as a linked device in a new generation.
    pub fn add(&self, other: &Member) {
        let entry = crate::sync::device_list::ListedDevice {
            device_pubkey: other.keys.device_pubkey,
            endpoint_id: other.keys.endpoint_id,
            role: crate::sync::device_list::Role::Linked,
            vault_device_uuid: other.device.db().device_id(),
            name_sealed: Vec::new(),
            added_at: 2,
        };
        self.issue_list(|mut list| {
            list.devices.push(entry);
            list
        });
    }
}
