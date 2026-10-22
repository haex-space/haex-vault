//! Passkeys across devices (spec 036, US5, T063, research R5, R6): two replicas exchange their
//! changes through the pull of spec 024, as `passwords_sync.rs` does. After both confirm with the
//! same passkey and sync, the counter is 0 on both; a link made on one device appears on the other,
//! and goes there when its passkey or the passkey's entry is deleted on the other device.
//! (`passwords_sync.rs` is over the 500-line boundary, so these cases live here.)

// These tests read raw vault state (rows) that the CRDT write path does not expose.
#![allow(clippy::disallowed_methods)]

use std::sync::Arc;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use haex_crdt::Database;

use holzi_lib::identity::installation_id_path;
use holzi_lib::instances::vault_config::vault_config;
use holzi_lib::passwords::access::Caller;
use holzi_lib::passwords::model::{ItemInput, Target, TargetKind};
use holzi_lib::passwords::model_passkeys::{
    PasskeyConfirmRequest, PasskeyCreateRequest, PasskeyTarget, UserPresence,
};
use holzi_lib::passwords::passkey_links;
use holzi_lib::passwords::service::PasswordsService;
use holzi_lib::sync::inbound::Inbox;
use holzi_lib::sync::outbound::serve_pull_with_budget;
use holzi_lib::sync::replica::Replica;
use holzi_lib::vault_gate::VaultGate;

const BUDGET: usize = 8 * 1024 * 1024;

struct Dev {
    _dir: tempfile::TempDir,
    replica: Replica,
    service: PasswordsService,
}

impl Dev {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let config = vault_config(
            "passkeys-sync-passphrase",
            &dir.path().join("vault.db"),
            &installation_id_path(dir.path()),
            true,
        );
        let db = Arc::new(Database::open(config).expect("open vault"));
        let vault_db = VaultGate::new()
            .vault_db(Arc::clone(&db))
            .expect("open the gate");
        Self {
            _dir: dir,
            replica: Replica::new(db),
            service: PasswordsService::new(vault_db),
        }
    }

    fn count(&self, sql: &str) -> i64 {
        self.replica
            .db()
            .with_connection(|c| Ok(c.query_row(sql, [], |r| r.get(0))?))
            .expect("count")
    }

    async fn entry(&self, title: &str) -> String {
        self.service
            .create_item(
                &Caller::User,
                &[],
                ItemInput {
                    title: Some(title.to_string()),
                    ..ItemInput::default()
                },
                None,
            )
            .await
            .expect("entry")
    }

    /// A new passkey at `item_id`; returns its row id.
    async fn passkey(&self, item_id: &str) -> String {
        self.service
            .passkey_create(
                &Caller::User,
                &[],
                PasskeyCreateRequest {
                    target: PasskeyTarget::Existing(item_id.to_string()),
                    rp_id: "example.com".to_string(),
                    rp_name: "Example".to_string(),
                    origin: "https://example.com".to_string(),
                    user_handle: URL_SAFE_NO_PAD.encode(b"user"),
                    user_name: "anna".to_string(),
                    user_display_name: None,
                    challenge: URL_SAFE_NO_PAD.encode(b"c"),
                    exclude_credentials: Vec::new(),
                    pub_key_cred_params: vec![-7],
                    discoverable: true,
                    presence: UserPresence::None,
                },
            )
            .await
            .expect("create");
        self.replica
            .db()
            .with_connection(|c| {
                Ok(c.query_row(
                    "SELECT id FROM haex_passwords_passkeys WHERE item_id = ?1",
                    [item_id],
                    |r| r.get(0),
                )?)
            })
            .expect("passkey id")
    }

    fn link(&self, item_id: &str, passkey_id: &str) {
        self.replica
            .db()
            .write(|tx| {
                passkey_links::link(tx, item_id, passkey_id)
                    .map(|_| ())
                    .map_err(haex_crdt::Error::from)
            })
            .expect("link");
    }

    /// Confirms once and returns the counter of the authenticator data.
    async fn confirm(&self) -> [u8; 4] {
        let assertion = self
            .service
            .passkey_confirm(
                &Caller::User,
                &[],
                PasskeyConfirmRequest {
                    rp_id: "example.com".to_string(),
                    origin: "https://example.com".to_string(),
                    challenge: URL_SAFE_NO_PAD.encode(b"get"),
                    allow_credentials: Vec::new(),
                    item_id: None,
                    presence: UserPresence::None,
                },
            )
            .await
            .expect("confirm");
        let data = URL_SAFE_NO_PAD
            .decode(assertion.authenticator_data)
            .expect("authenticator data");
        data[33..37].try_into().expect("4 bytes")
    }
}

fn pull(from: &Dev, to: &Dev) {
    let theirs = to.replica.progress().expect("progress");
    let mut outbox = serve_pull_with_budget(&from.replica, &theirs, BUDGET).expect("serve");
    let mut inbox = Inbox::new();
    while let Some(page) = outbox.next_page() {
        inbox.receive(&to.replica, page).expect("receive a page");
    }
}

fn settle(a: &Dev, b: &Dev) {
    for _ in 0..2 {
        pull(a, b);
        pull(b, a);
    }
}

const LINKS: &str = "SELECT COUNT(*) FROM haex_passwords_passkey_links";

#[tokio::test]
async fn two_devices_that_confirm_and_sync_both_report_a_zero_counter() {
    let (a, b) = (Dev::new(), Dev::new());
    let item = a.entry("Konto").await;
    a.passkey(&item).await;
    settle(&a, &b);

    assert_eq!(a.confirm().await, [0, 0, 0, 0]);
    assert_eq!(b.confirm().await, [0, 0, 0, 0]);
    settle(&a, &b);
    for dev in [&a, &b] {
        assert_eq!(
            dev.count("SELECT sign_count FROM haex_passwords_passkeys"),
            0
        );
        assert_eq!(dev.confirm().await, [0, 0, 0, 0]);
        assert_eq!(
            dev.count(
                "SELECT COUNT(*) FROM haex_passwords_passkeys WHERE last_used_at IS NOT NULL"
            ),
            1
        );
    }
}

#[tokio::test]
async fn a_link_travels_and_goes_with_its_passkey_deleted_on_the_other_device() {
    let (a, b) = (Dev::new(), Dev::new());
    let source = a.entry("Konto").await;
    let target = a.entry("Kopie").await;
    let passkey = a.passkey(&source).await;
    a.link(&target, &passkey);
    settle(&a, &b);
    assert_eq!(b.count(LINKS), 1, "the link arrived");

    b.service
        .passkey_delete(&Caller::User, passkey)
        .await
        .expect("delete the passkey");
    settle(&a, &b);
    assert_eq!(a.count(LINKS), 0, "the link went with its passkey");
    assert_eq!(a.count("SELECT COUNT(*) FROM haex_passwords_passkeys"), 0);
}

#[tokio::test]
async fn a_link_goes_when_the_source_entry_is_deleted_for_good_on_the_other_device() {
    let (a, b) = (Dev::new(), Dev::new());
    let source = a.entry("Konto").await;
    let target = a.entry("Kopie").await;
    let passkey = a.passkey(&source).await;
    a.link(&target, &passkey);
    settle(&a, &b);

    let targets = vec![Target {
        kind: TargetKind::Item,
        id: source,
    }];
    b.service
        .trash_targets(&Caller::User, targets.clone())
        .await
        .expect("trash");
    b.service
        .delete_permanently(&Caller::User, targets, false)
        .await
        .expect("delete for good");
    settle(&a, &b);
    assert_eq!(a.count(LINKS), 0);
    assert_eq!(
        a.count("SELECT COUNT(*) FROM haex_passwords_item_details"),
        1,
        "the target stays"
    );
}
