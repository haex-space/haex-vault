//! Helpers shared by the unit tests of the address book: a real temporary vault with the production
//! migrations and a service over it.

use std::sync::Arc;

use super::ContactsService;
use crate::contacts::model::{ContactInput, Head, ValueInput};
use crate::passwords::test_support::open_test_vault;
use crate::vault_gate::{VaultDb, VaultGate};

/// A service over a fresh temporary vault; the directory lives as long as the value.
pub(crate) struct TestVault {
    _dir: tempfile::TempDir,
    pub db: VaultDb,
    pub service: ContactsService,
}

pub(crate) fn test_vault() -> TestVault {
    let (dir, db) = open_test_vault();
    let db = VaultGate::new()
        .vault_db(Arc::new(db))
        .expect("a vault handle");
    TestVault {
        _dir: dir,
        service: ContactsService::new(db.clone()),
        db,
    }
}

pub(crate) fn value(kind: &str, label: &str, value: &str) -> ValueInput {
    ValueInput {
        kind: kind.to_owned(),
        label: label.to_owned(),
        value: value.to_owned(),
    }
}

/// A contact with a given and a family name and the values.
pub(crate) fn person(given: &str, family: &str, values: Vec<ValueInput>) -> ContactInput {
    ContactInput {
        head: Head {
            given_name: Some(given.to_owned()),
            family_name: Some(family.to_owned()),
            ..Head::default()
        },
        values,
        groups: vec![],
        targets: vec![],
    }
}

/// A phone account for the tests.
pub(crate) fn account(account_type: &str, name: &str) -> crate::contacts::phone::Account {
    crate::contacts::phone::Account {
        account_type: account_type.to_owned(),
        name: name.to_owned(),
        writable: true,
        is_default: false,
    }
}

/// Adds another device to the vault, a phone with the sync on (`on`) or once on and then off,
/// with its accounts and target account; returns its `vault_device_uuid`.
pub(crate) fn add_phone(
    vault: &TestVault,
    alias: &str,
    on: bool,
    accounts: Vec<crate::contacts::phone::Account>,
) -> String {
    use crate::contacts::phone::state;
    let device = uuid::Uuid::new_v4().to_string();
    let (id, alias) = (device.clone(), alias.to_owned());
    vault
        .db
        .write_blocking(move |tx| {
            tx.execute(
                "INSERT INTO known_devices (installation_uuid, vault_device_uuid, alias, first_seen) \
                 VALUES (?1, ?2, ?3, 1)",
                haex_crdt::rusqlite::params![uuid::Uuid::new_v4().to_string(), id, alias],
            )?;
            let to = |error: crate::error::HolziError| haex_crdt::Error::from(error);
            state::enable(tx, &id).map_err(to)?;
            if let Some(first) = accounts.first() {
                state::set_target(tx, &id, first).map_err(to)?;
            }
            state::set_accounts(tx, &id, &accounts).map_err(to)?;
            if !on {
                state::disable(tx, &id).map_err(to)?;
            }
            Ok(())
        })
        .expect("add a phone");
    device
}
