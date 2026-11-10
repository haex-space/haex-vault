//! A temporary vault and helpers shared by the tests of agent profiles.

use std::sync::Arc;

use uuid::Uuid;

use crate::identity::{installation_id_path, read_or_mint_installation_uuid};
use crate::mcp_server::registry::{hash, register, AgentKind, Credential, NewAgent, Transport};
use crate::passwords::test_support::open_test_vault;
use crate::storage::known_devices;
use crate::vault_gate::{VaultDb, VaultGate};

pub struct Vault {
    _dir: tempfile::TempDir,
    pub db: VaultDb,
    pub device: Uuid,
}

pub fn vault() -> Vault {
    let (dir, database) = open_test_vault();
    let installation =
        read_or_mint_installation_uuid(&installation_id_path(dir.path())).expect("installation");
    let device = crate::storage::query::read(&database, |q| {
        known_devices::get_vault_device_uuid(q, installation)
    })
    .expect("read")
    .expect("this device");
    let db = VaultGate::new()
        .vault_db(Arc::new(database))
        .expect("a vault handle");
    Vault {
        _dir: dir,
        db,
        device,
    }
}

/// An agent of this vault in "Standard"; no profile row is needed for that.
pub async fn registered(v: &Vault, name: &str) -> Uuid {
    let device = v.device;
    let new = NewAgent {
        kind: AgentKind::External,
        slug: name.to_owned(),
        client_name: name.to_owned(),
        client_version: None,
        transport: Transport::Ipc,
    };
    let digest = hash(Credential::generate().expect("random").as_str());
    v.db.write(move |tx| register(tx, device, &new, &digest, super::STANDARD_PROFILE_ID, 1_000))
        .await
        .expect("registered")
        .id
}

/// Edits the rights of "Standard", where the chat's agent outside a turn is judged, as the user
/// would in the profile.
pub fn edit_standard(
    db: &VaultDb,
    change: impl FnOnce(&mut crate::mcp_server::grants::Rights) + Send + 'static,
) {
    db.write_blocking(move |tx| {
        let mut rights = super::resolve::standard_rights(tx)?;
        change(&mut rights);
        super::store::set_rights(tx, super::STANDARD_PROFILE_ID, &rights)
    })
    .expect("rights of Standard");
}
