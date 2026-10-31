//! Tests for the registry (spec 021, T006) against a real temporary vault.

use std::sync::Arc;

use uuid::Uuid;

use super::grants::{default_rights, OperateLevel, Rights};
use super::registry::{
    block, find_by_credential, hash, is_blocked, kept_name, list, load_rights, register,
    save_rights, touch, AgentKind, AgentMode, AgentStatus, Credential, NewAgent, Transport,
    MAX_CLIENT_NAME_CHARS,
};
use crate::identity::{installation_id_path, read_or_mint_installation_uuid, VAULT_SCOPE_UUID};
use crate::passwords::test_support::open_test_vault;
use crate::storage::known_devices;
use crate::vault_gate::{VaultDb, VaultGate};

struct Vault {
    _dir: tempfile::TempDir,
    db: VaultDb,
    device: Uuid,
}

fn vault() -> Vault {
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

fn claude() -> NewAgent {
    NewAgent {
        kind: AgentKind::External,
        slug: "claude-code".to_owned(),
        client_name: "claude-code".to_owned(),
        client_version: Some("2.1.0".to_owned()),
        transport: Transport::Ipc,
        mode: AgentMode::default(),
    }
}

#[test]
fn a_credential_is_long_random_and_kept_only_as_hash() {
    let one = Credential::generate().expect("random");
    let two = Credential::generate().expect("random");
    assert_ne!(one.as_str(), two.as_str());
    assert_eq!(
        one.as_str().len(),
        43,
        "32 bytes in base64url without padding"
    );
    assert!(!hash(one.as_str()).contains(one.as_str()));
    assert_eq!(hash(one.as_str()), hash(one.as_str()));
    assert!(!format!("{one:?}").contains(one.as_str()));
}

#[test]
fn a_reported_name_is_cut() {
    let long = "x".repeat(MAX_CLIENT_NAME_CHARS + 10);
    assert_eq!(kept_name(&long).chars().count(), MAX_CLIENT_NAME_CHARS);
    assert_eq!(kept_name("  codex  "), "codex");
}

#[tokio::test]
async fn a_registered_agent_is_found_by_its_credential_only() {
    let v = vault();
    let device = v.device;
    let credential = Credential::generate().expect("random").into_string();
    let digest = hash(&credential);
    let stored =
        v.db.write(move |tx| register(tx, device, &claude(), &digest, &default_rights(), 1_000))
            .await
            .expect("registered");
    assert_eq!(stored.status, AgentStatus::Registered);
    assert_eq!(stored.mode, AgentMode::Manual, "Manuell by default");

    let found = {
        let credential = credential.clone();
        v.db.read(move |q| find_by_credential(q, device, &credential))
            .await
            .expect("read")
    };
    assert_eq!(found.as_ref().map(|a| a.id), Some(stored.id));
    let wrong =
        v.db.read(move |q| find_by_credential(q, device, "not-the-credential"))
            .await
            .expect("read");
    assert!(wrong.is_none());
    // The credential itself is nowhere in the row.
    let rows = v.db.read(move |q| list(q, device)).await.expect("list");
    assert!(!format!("{rows:?}").contains(&credential));
}

#[tokio::test]
async fn rows_of_another_device_are_never_returned() {
    let v = vault();
    let other = VAULT_SCOPE_UUID;
    let credential = Credential::generate().expect("random").into_string();
    let digest = hash(&credential);
    v.db.write(move |tx| register(tx, other, &claude(), &digest, &default_rights(), 1))
        .await
        .expect("registered for the other device");
    let device = v.device;
    let found =
        v.db.read(move |q| find_by_credential(q, device, &credential))
            .await
            .expect("read");
    assert!(found.is_none());
    assert!(v
        .db
        .read(move |q| list(q, device))
        .await
        .expect("list")
        .is_empty());
}

#[tokio::test]
async fn rights_round_trip_and_are_replaced() {
    let v = vault();
    let device = v.device;
    let digest = hash("c");
    let agent =
        v.db.write(move |tx| register(tx, device, &claude(), &digest, &default_rights(), 1))
            .await
            .expect("registered");
    let id = agent.id;
    assert_eq!(
        v.db.read(move |q| load_rights(q, device, id))
            .await
            .expect("read"),
        default_rights()
    );
    // Spec 065 T008: the group `browser` is stored next to `operate` and reads back the same;
    // so do the groups `extension` (spec 021 US4) and `passwords` (US5).
    let site = crate::agent_browser::sites::Site::parse("https://shop.example.de").expect("site");
    let narrow = Rights {
        operate: [("settings.device".to_owned(), OperateLevel::Write)]
            .into_iter()
            .collect(),
        browser: Some(super::grants::BrowserRights {
            user_browser: true,
            sites: [(
                site,
                super::grants::SiteGrant {
                    level: super::grants::SiteLevel::Operate,
                    fill_remembered: true,
                },
            )]
            .into_iter()
            .collect(),
        }),
        passwords: Some(super::grants::PasswordRights {
            level: super::grants::PasswordLevel::Read,
            tags: Some(vec!["dev".to_owned(), "Büro".to_owned()]),
        }),
        extensions: [Uuid::from_u128(0x4d41_494c)].into_iter().collect(),
    };
    let saved = narrow.clone();
    v.db.write(move |tx| save_rights(tx, device, id, &saved))
        .await
        .expect("saved");
    assert_eq!(
        v.db.read(move |q| load_rights(q, device, id))
            .await
            .expect("read"),
        narrow
    );
}

#[tokio::test]
async fn a_block_is_remembered_by_slug_and_a_touch_moves_last_seen() {
    let v = vault();
    let device = v.device;
    assert!(!v
        .db
        .read(move |q| is_blocked(q, device, "codex"))
        .await
        .expect("read"));
    v.db.write(move |tx| block(tx, device, "codex", "codex", 5))
        .await
        .expect("blocked");
    assert!(v
        .db
        .read(move |q| is_blocked(q, device, "codex"))
        .await
        .expect("read"));
    assert!(!v
        .db
        .read(move |q| is_blocked(q, device, "claude-code"))
        .await
        .expect("read"));

    let digest = hash("c");
    let agent =
        v.db.write(move |tx| register(tx, device, &claude(), &digest, &default_rights(), 1))
            .await
            .expect("registered");
    let id = agent.id;
    v.db.write(move |tx| touch(tx, device, id, 99))
        .await
        .expect("touched");
    let rows = v.db.read(move |q| list(q, device)).await.expect("list");
    let row = rows.iter().find(|a| a.id == id).expect("the agent");
    assert_eq!(row.last_seen_at, Some(99));
}
