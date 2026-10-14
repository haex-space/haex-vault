//! Integration coverage for entries that belong to a holzi function (spec 038, rule Z14 of spec
//! 034): only the user and the owning function reach them. A caller from outside with a grant for
//! every entry does not see them in a list, cannot read, change or delete them, and a placeholder
//! pointing at one does not resolve for it; the built-in agent does not list them. Only a holzi
//! function creates them and deletes them for good.

use std::sync::Arc;

use haex_crdt::{Database, DatabaseConfig, NoopSignatureProvider, SqlCipherKey};

use holzi_lib::identity::{
    holzi_migration_source, installation_id_path, HolziBootstrap, HOLZI_TRIGGER_VERSION,
};
use holzi_lib::passwords::access::{Caller, Grant, GrantAction, Scope};
use holzi_lib::passwords::copy::{CopyOptions, CopyTitle};
use holzi_lib::passwords::model::{ItemInput, ItemPatch, Patch, SecretField, Target, TargetKind};
use holzi_lib::passwords::model_references::RefMarkKind;
use holzi_lib::passwords::service::{Headers, PasswordsService};
use holzi_lib::vault_gate::VaultGate;
use holzi_lib::HolziError;

const SECRET: &str = "s3-secret-owner-marker";

fn service() -> (tempfile::TempDir, PasswordsService) {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = Database::open(DatabaseConfig {
        path: dir.path().join("vault.db"),
        key: SqlCipherKey::new("passwords-owner"),
        create_if_missing: true,
        bootstrap: Arc::new(HolziBootstrap::new(installation_id_path(dir.path()))),
        signature_provider: Arc::new(NoopSignatureProvider),
        migration_source: holzi_migration_source(),
        trigger_version: HOLZI_TRIGGER_VERSION,
        max_transaction_bytes: haex_crdt::MAX_CRDT_TRANSACTION_BYTES,
        max_value_bytes: haex_crdt::MAX_VALUE_BYTES,
    })
    .expect("open the vault");
    let vault_db = VaultGate::new()
        .vault_db(Arc::new(db))
        .expect("open the gate");
    (dir, PasswordsService::new(vault_db))
}

const STORAGE: Caller = Caller::Internal { feature: "storage" };

fn all() -> Vec<Grant> {
    vec![Grant::new(GrantAction::ReadWrite, Scope::All)]
}

fn extension() -> Caller {
    Caller::Extension {
        id: "ext".to_string(),
    }
}

async fn create_owned(service: &PasswordsService) -> String {
    service
        .create_owned_item(
            &STORAGE,
            ItemInput {
                title: Some("S3: RustFS".to_string()),
                username: Some("access-key".to_string()),
                password: Some(SECRET.to_string()),
                ..ItemInput::default()
            },
        )
        .await
        .expect("create an owned entry")
}

fn listed(headers: Headers) -> Vec<String> {
    match headers {
        Headers::Items(items) => items.into_iter().map(|h| h.id).collect(),
        Headers::Agent(items) => items.into_iter().map(|h| h.id).collect(),
    }
}

#[tokio::test]
async fn an_owned_entry_is_out_of_reach_for_a_caller_with_a_grant_for_everything() {
    let (_dir, service) = service();
    let owned = create_owned(&service).await;
    let users = service
        .create_item(
            &Caller::User,
            &[],
            ItemInput {
                title: Some("Bank".to_string()),
                ..ItemInput::default()
            },
            None,
        )
        .await
        .expect("create a user entry");

    for caller in [
        extension(),
        Caller::ExternalAgent {
            id: "mcp".to_string(),
        },
        Caller::Internal { feature: "other" },
    ] {
        let ids = listed(service.list_headers(&caller, &all()).await.expect("list"));
        assert_eq!(
            ids,
            std::slice::from_ref(&users),
            "{caller:?} lists only the user's entry"
        );
        assert!(matches!(
            service
                .read_secret_item(&caller, &all(), owned.clone())
                .await,
            Err(HolziError::PasswordsNotFound)
        ));
        let token = "irrelevant".to_string();
        assert!(matches!(
            service
                .update_item(&caller, &all(), owned.clone(), token, ItemPatch::default())
                .await,
            Err(HolziError::PasswordsNotFound)
        ));
        assert!(matches!(
            service.delete_item(&caller, &all(), owned.clone()).await,
            Err(HolziError::PasswordsNotFound)
        ));
    }

    let agent = listed(
        service
            .list_headers(&Caller::BuiltinAgent, &[])
            .await
            .expect("agent headers"),
    );
    assert_eq!(
        agent,
        std::slice::from_ref(&users),
        "the built-in agent never lists it"
    );

    let user = listed(
        service
            .list_headers(&Caller::User, &[])
            .await
            .expect("list"),
    );
    assert!(user.contains(&owned), "the user sees it");
    let overview = service
        .load_overview(&Caller::User)
        .await
        .expect("overview");
    let header = overview
        .headers
        .iter()
        .find(|h| h.id == owned)
        .expect("in the user's overview");
    assert_eq!(
        header.owner.as_deref(),
        Some("storage"),
        "the window can mark it"
    );

    let read = service
        .read_secret_item(&STORAGE, &all(), owned.clone())
        .await
        .expect("the owner reads it");
    assert_eq!(read.password.as_deref(), Some(SECRET));
}

#[tokio::test]
async fn a_placeholder_pointing_at_an_owned_entry_resolves_only_for_the_user() {
    let (_dir, service) = service();
    let owned = create_owned(&service).await;
    let token = service
        .reference_token(&Caller::User, owned.clone(), RefMarkKind::Password, None)
        .await
        .expect("token");
    let pointing = service
        .create_item(
            &Caller::User,
            &[],
            ItemInput {
                title: Some("Backup".to_string()),
                password: Some(token),
                ..ItemInput::default()
            },
            None,
        )
        .await
        .expect("create the pointing entry");

    let read = service
        .read_secret_item(&extension(), &all(), pointing.clone())
        .await
        .expect("read");
    assert_eq!(read.password, None, "absent, with no hint why");
    assert!(!format!("{read:?}").contains(SECRET));

    let revealed = service
        .reveal(&Caller::User, pointing, SecretField::Password)
        .await
        .expect("reveal");
    assert_eq!(revealed.value.as_str(), SECRET);
}

#[tokio::test]
async fn only_a_holzi_function_creates_owned_entries_and_only_the_owner_deletes_them_for_good() {
    let (_dir, service) = service();
    for caller in [Caller::User, extension()] {
        assert!(matches!(
            service
                .create_owned_item(&caller, ItemInput::default())
                .await,
            Err(HolziError::PasswordsForbidden)
        ));
    }
    let owned = create_owned(&service).await;

    // A change through the ordinary update keeps the owner.
    let updated_at = service
        .get_item(&Caller::User, owned.clone())
        .await
        .expect("get")
        .header
        .updated_at
        .expect("token");
    service
        .update_item(
            &Caller::User,
            &[],
            owned.clone(),
            updated_at,
            ItemPatch {
                title: Patch::Set("S3: renamed".to_string()),
                ..ItemPatch::default()
            },
        )
        .await
        .expect("the user may edit it");
    let header = service
        .get_item(&Caller::User, owned.clone())
        .await
        .expect("get")
        .header;
    assert_eq!(header.owner.as_deref(), Some("storage"));

    assert!(matches!(
        service
            .delete_owned_item(&Caller::Internal { feature: "other" }, owned.clone())
            .await,
        Err(HolziError::PasswordsNotFound)
    ));
    assert!(matches!(
        service
            .delete_owned_item(&Caller::User, owned.clone())
            .await,
        Err(HolziError::PasswordsForbidden)
    ));
    service
        .delete_owned_item(&STORAGE, owned.clone())
        .await
        .expect("the owner deletes it");
    assert!(matches!(
        service.get_item(&Caller::User, owned).await,
        Err(HolziError::PasswordsNotFound)
    ));
}

#[tokio::test]
async fn the_users_copy_of_an_owned_entry_stays_out_of_reach_too() {
    let (_dir, service) = service();
    let owned = create_owned(&service).await;
    let report = service
        .copy(
            &Caller::User,
            vec![Target {
                kind: TargetKind::Item,
                id: owned.clone(),
            }],
            None,
            CopyOptions {
                title: CopyTitle::Suffix(" (Kopie)".to_string()),
                history: false,
                username_as_reference: false,
                password_as_reference: false,
                passkeys_as_links: None,
            },
        )
        .await
        .expect("the user copies");
    assert_eq!(report.items_created, 1);
    let user_ids = listed(
        service
            .list_headers(&Caller::User, &[])
            .await
            .expect("list"),
    );
    assert_eq!(user_ids.len(), 2, "the user sees original and copy");
    let ids = listed(
        service
            .list_headers(&extension(), &all())
            .await
            .expect("list"),
    );
    assert!(ids.is_empty(), "the copy keeps its owner: {ids:?}");
}

#[tokio::test]
async fn the_cloud_storage_tag_opens_no_owned_entry_but_a_chosen_one_keeps_its_grants() {
    // Spec 052 (FR-008, assumption in spec.md): holzi tags its own entries and the ones the user
    // chose for a connection. Rule Z14 still hides its own from everyone else, also from a grant
    // for that tag; an entry the user chose stays an ordinary entry that such a grant reaches.
    let (_dir, service) = service();
    let tag = holzi_lib::remote_storage::credentials::TAG.to_string();
    let entry = |title: &str| ItemInput {
        title: Some(title.to_string()),
        username: Some("access-key".to_string()),
        password: Some(SECRET.to_string()),
        tags: vec![tag.clone()],
        ..ItemInput::default()
    };
    let owned = service
        .create_owned_item(&STORAGE, entry("S3: RustFS"))
        .await
        .expect("an owned entry with the tag");
    let chosen = service
        .create_item(&Caller::User, &[], entry("IDrive"), None)
        .await
        .expect("a chosen entry with the tag");

    let by_tag = vec![Grant::new(
        GrantAction::Read,
        Scope::Tags(std::iter::once(tag.clone()).collect()),
    )];
    for grants in [all(), by_tag] {
        let ids = listed(
            service
                .list_headers(&extension(), &grants)
                .await
                .expect("list"),
        );
        assert_eq!(ids, std::slice::from_ref(&chosen), "{grants:?}");
        assert!(matches!(
            service
                .read_secret_item(&extension(), &grants, owned.clone())
                .await,
            Err(HolziError::PasswordsNotFound)
        ));
        let read = service
            .read_secret_item(&extension(), &grants, chosen.clone())
            .await
            .expect("the grant reaches the chosen entry");
        assert_eq!(read.password.as_deref(), Some(SECRET));
    }
}
