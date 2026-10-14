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
    }
}
