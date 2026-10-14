//! A vault, the fake phone and this device for the tests of whole runs (spec 051 T036).

use super::test_support::FakePhoneBook;
use crate::error::Result;

/// A vault, a fake phone and this device, for the tests of whole runs.
pub(crate) struct Rig {
    pub vault: crate::contacts::service::test_support::TestVault,
    pub book: FakePhoneBook,
    pub device: String,
}

impl Rig {
    pub fn new() -> Self {
        let vault = crate::contacts::service::test_support::test_vault();
        let device = vault.db.device_id().to_string();
        Self {
            vault,
            book: FakePhoneBook::new(),
            device,
        }
    }

    /// Switches the sync on: the overview waits.
    pub fn enable(&self) {
        let device = self.device.clone();
        self.vault
            .db
            .write_blocking(move |tx| {
                super::state::enable(tx, &device).map_err(haex_crdt::Error::from)
            })
            .expect("enable");
    }

    pub fn disable(&self) {
        let device = self.device.clone();
        self.vault
            .db
            .write_blocking(move |tx| {
                super::state::disable(tx, &device).map_err(haex_crdt::Error::from)
            })
            .expect("disable");
    }

    pub fn run(&self, mode: super::Mode<'_>) -> Result<super::RunSummary> {
        super::run(
            &self.vault.db,
            &self.book,
            &self.device,
            mode,
            &crate::passwords::clock::now(),
        )
    }

    /// Switches on and confirms the overview without merging an uncertain pair.
    pub fn start(&self) -> super::RunSummary {
        self.enable();
        self.run(super::Mode::First(&[])).expect("the first run")
    }

    pub fn review(&self) -> super::ReviewPlan {
        super::review(&self.vault.db, &self.book, &self.device).expect("review")
    }

    pub fn status(&self) -> super::state::PhoneStatus {
        let device = self.device.clone();
        self.vault
            .db
            .read_blocking(move |q| {
                super::state::status(q, &device).map_err(haex_crdt::Error::from)
            })
            .expect("status")
    }

    pub fn headers(&self) -> Vec<crate::contacts::model::ContactHeader> {
        self.vault
            .db
            .read_blocking(|q| crate::contacts::store::headers(q).map_err(haex_crdt::Error::from))
            .expect("headers")
    }

    pub fn contact(&self, id: &str) -> crate::contacts::model::Contact {
        let id = id.to_owned();
        self.vault
            .db
            .read_blocking(move |q| {
                crate::contacts::store::load(q, &id).map_err(haex_crdt::Error::from)
            })
            .expect("the contact")
    }

    /// The contact with this name.
    pub fn named(&self, name: &str) -> crate::contacts::model::Contact {
        let header = self
            .headers()
            .into_iter()
            .find(|header| header.name == name);
        self.contact(
            &header
                .unwrap_or_else(|| panic!("a contact named {name}"))
                .id,
        )
    }

    pub fn targets(&self) -> Vec<super::store::Target> {
        let device = self.device.clone();
        self.vault
            .db
            .read_blocking(move |q| {
                super::store::targets(q, &device).map_err(haex_crdt::Error::from)
            })
            .expect("targets")
    }

    pub fn links(&self) -> Vec<super::store::Link> {
        let device = self.device.clone();
        self.vault
            .db
            .read_blocking(move |q| super::store::links(q, &device).map_err(haex_crdt::Error::from))
            .expect("links")
    }
}
