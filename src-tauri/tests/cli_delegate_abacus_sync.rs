//! The Abacus credential across devices (spec 096, T026): two real replicas exchange their changes
//! through the pull of spec 024, each with the vault of its own. A credential saved on one device
//! is read on the other without a connect step; concurrent saves in one scope settle on the newest
//! entry; a disconnect arrives; and one device's entry survives the other's save. No real account
//! is used.

// These tests read raw vault state that the CRDT write path does not expose.
#![allow(clippy::disallowed_methods)]

use std::sync::Arc;

use haex_crdt::Database;

use haex_lib::identity::installation_id_path;
use haex_lib::instances::vault_config::vault_config;
use haex_lib::providers::delegate_credential::{self, Read, KIND_DEVICE_CODE, VAULT_SCOPE};
use haex_lib::sync::inbound::Inbox;
use haex_lib::sync::outbound::serve_pull_with_budget;
use haex_lib::sync::replica::Replica;
use haex_lib::vault_gate::{VaultDb, VaultGate};

const BUDGET: usize = 8 * 1024 * 1024;
const VENDOR: &str = "abacusai";

struct Dev {
    _dir: tempfile::TempDir,
    replica: Replica,
    db: VaultDb,
}

impl Dev {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let config = vault_config(
            "abacus-sync-passphrase",
            &dir.path().join("vault.db"),
            &installation_id_path(dir.path()),
            true,
        );
        let database = Arc::new(Database::open(config).expect("open vault"));
        let db = VaultGate::new()
            .vault_db(Arc::clone(&database))
            .expect("open the gate");
        Self {
            _dir: dir,
            replica: Replica::new(database),
            db,
        }
    }

    async fn secret(&self, scope: &str) -> Option<String> {
        match delegate_credential::read(&self.db, VENDOR, scope)
            .await
            .expect("read")
        {
            Read::Ready(credential) => Some(credential.secret().to_owned()),
            Read::Missing => None,
        }
    }

    async fn save(&self, secret: &str, scope: &str) {
        delegate_credential::save(&self.db, VENDOR, KIND_DEVICE_CODE, secret, scope)
            .await
            .expect("save");
    }
}

/// What `to` pulls from `from`, whole.
fn pull(from: &Dev, to: &Dev) {
    let theirs = to.replica.progress().expect("progress");
    let mut outbox = serve_pull_with_budget(&from.replica, &theirs, BUDGET).expect("serve");
    let mut inbox = Inbox::new();
    while let Some(page) = outbox.next_page() {
        inbox.receive(&to.replica, page).expect("receive a page");
    }
}

/// Both directions, twice, so everything has settled.
fn settle(a: &Dev, b: &Dev) {
    for _ in 0..2 {
        pull(a, b);
        pull(b, a);
    }
}

#[tokio::test]
async fn a_credential_saved_on_one_device_is_read_on_the_other_without_connecting() {
    let (a, b) = (Dev::new(), Dev::new());
    assert_eq!(b.secret(VAULT_SCOPE).await, None, "nothing before the sync");

    a.save("code-from-a", VAULT_SCOPE).await;
    settle(&a, &b);

    assert_eq!(b.secret(VAULT_SCOPE).await.as_deref(), Some("code-from-a"));
}

#[tokio::test]
async fn a_disconnect_on_one_device_arrives_on_the_other() {
    let (a, b) = (Dev::new(), Dev::new());
    a.save("code-from-a", VAULT_SCOPE).await;
    settle(&a, &b);
    assert!(b.secret(VAULT_SCOPE).await.is_some());

    delegate_credential::delete(&a.db, VENDOR)
        .await
        .expect("delete");
    settle(&a, &b);

    assert_eq!(b.secret(VAULT_SCOPE).await, None);
}

#[tokio::test]
async fn saves_in_one_scope_on_two_devices_settle_on_one_entry_after_the_next_save() {
    let (a, b) = (Dev::new(), Dev::new());
    a.save("code-from-a", VAULT_SCOPE).await;
    // The entries must differ in `updated_at`, which has a resolution of a second.
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    b.save("code-from-b", VAULT_SCOPE).await;
    settle(&a, &b);

    // Both entries exist on both devices; the newest is the one that is read.
    assert_eq!(a.secret(VAULT_SCOPE).await.as_deref(), Some("code-from-b"));
    assert_eq!(b.secret(VAULT_SCOPE).await.as_deref(), Some("code-from-b"));

    // A save on one device removes the older entry there, and that removal travels.
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    a.save("code-after", VAULT_SCOPE).await;
    settle(&a, &b);
    assert_eq!(a.secret(VAULT_SCOPE).await.as_deref(), Some("code-after"));
    assert_eq!(b.secret(VAULT_SCOPE).await.as_deref(), Some("code-after"));
}

#[tokio::test]
async fn one_devices_entry_survives_the_other_devices_save() {
    let (a, b) = (Dev::new(), Dev::new());
    a.save("code-of-a", "installation-a").await;
    b.save("code-of-b", "installation-b").await;
    settle(&a, &b);

    for dev in [&a, &b] {
        assert_eq!(
            dev.secret("installation-a").await.as_deref(),
            Some("code-of-a")
        );
        assert_eq!(
            dev.secret("installation-b").await.as_deref(),
            Some("code-of-b")
        );
    }

    // B connects again: A's entry is still there on both.
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    b.save("new-code-of-b", "installation-b").await;
    settle(&a, &b);
    for dev in [&a, &b] {
        assert_eq!(
            dev.secret("installation-a").await.as_deref(),
            Some("code-of-a")
        );
        assert_eq!(
            dev.secret("installation-b").await.as_deref(),
            Some("new-code-of-b")
        );
    }
}
