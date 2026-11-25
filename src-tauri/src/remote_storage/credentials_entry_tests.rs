//! Entries of the password manager as credentials (spec 052 T005, T007, T011): the one rule of what
//! fits a protocol, the state of a linked entry when its values change, and the list to choose
//! from, which carries no value.

use super::credentials::{self, TAG};
use super::test_support::{credentials as placeholder, vault};
use super::{Credentials, CredentialsState, Protocol};
use crate::error::HaexError;
use crate::passwords::access::Caller;
use crate::passwords::model::{
    ItemInput, ItemPatch, KeyValueInput, KeyValuePatch, Patch, SecretItem, SecretKeyValue, Target,
    TargetKind,
};
use crate::passwords::service::PasswordsService;

const SECRET: &str = "placeholder-secret-of-entry-tests";
const KEY: &str = "-----BEGIN OPENSSH PRIVATE KEY-----\nplaceholder-key-of-entry-tests";

/// An entry with its values as the password manager reads it.
fn item(username: &str, password: &str, fields: &[(&str, &str)]) -> SecretItem {
    SecretItem {
        id: "e1".to_owned(),
        title: None,
        username: Some(username.to_owned()),
        password: Some(password.to_owned()),
        note: None,
        url: None,
        icon: None,
        color: None,
        expires_at: None,
        otp_secret: None,
        otp_digits: None,
        otp_period: None,
        otp_algorithm: None,
        autofill_aliases: None,
        tags: Vec::new(),
        key_values: fields
            .iter()
            .map(|(key, value)| SecretKeyValue {
                id: (*key).to_owned(),
                key: Some((*key).to_owned()),
                value: Some((*value).to_owned()),
            })
            .collect(),
        created_at: None,
        updated_at: None,
    }
}

/// A new entry of the user.
fn entry(title: &str, username: &str, password: &str, fields: &[(&str, &str)]) -> ItemInput {
    ItemInput {
        title: Some(title.to_owned()),
        username: (!username.is_empty()).then(|| username.to_owned()),
        password: (!password.is_empty()).then(|| password.to_owned()),
        url: Some(format!("https://{}.example/", title.to_lowercase())),
        key_values: fields
            .iter()
            .map(|(key, value)| KeyValueInput {
                key: (*key).to_owned(),
                value: Some((*value).to_owned()),
            })
            .collect(),
        ..ItemInput::default()
    }
}

async fn add(passwords: &PasswordsService, input: ItemInput) -> String {
    passwords
        .create_item(&Caller::User, &[], input, None)
        .await
        .expect("create")
}

async fn change(passwords: &PasswordsService, id: &str, patch: ItemPatch) {
    let token = passwords
        .get_item(&Caller::User, id.to_owned())
        .await
        .expect("entry")
        .header
        .updated_at
        .unwrap_or_default();
    passwords
        .update_item(&Caller::User, &[], id.to_owned(), token, patch)
        .await
        .expect("update");
}

#[test]
fn an_entry_fits_a_protocol_by_its_values() {
    let Some(Credentials::S3(s3)) = credentials::from_item(&item("AK", SECRET, &[]), Protocol::S3)
    else {
        panic!("S3 from user name and password");
    };
    assert_eq!(s3.access_key_id, "AK");
    assert_eq!(s3.secret_access_key.as_str(), SECRET);
    assert!(s3.session_token.is_none(), "no token without the field");
    let with_token = item("AK", SECRET, &[("sessionToken", "token")]);
    let Some(Credentials::S3(s3)) = credentials::from_item(&with_token, Protocol::S3) else {
        panic!("S3 with a token");
    };
    assert_eq!(
        s3.session_token.as_deref().map(String::as_str),
        Some("token")
    );

    for protocol in [Protocol::WebDav, Protocol::Sftp] {
        let Some(Credentials::Password(login)) =
            credentials::from_item(&item("anna", SECRET, &[]), protocol)
        else {
            panic!("a password login for {protocol:?}");
        };
        assert_eq!(
            (login.username.as_str(), login.password.as_str()),
            ("anna", SECRET)
        );
    }
    let key = item("anna", "", &[("privateKey", KEY), ("passphrase", "geheim")]);
    let Some(Credentials::SshKey(login)) = credentials::from_item(&key, Protocol::Sftp) else {
        panic!("a key login");
    };
    assert_eq!(login.private_key.as_str(), KEY);
    assert_eq!(
        login.passphrase.as_deref().map(String::as_str),
        Some("geheim")
    );
}

#[test]
fn an_entry_without_the_values_of_a_protocol_fits_nothing() {
    for protocol in [Protocol::S3, Protocol::WebDav, Protocol::Sftp] {
        let none = |item: SecretItem| credentials::from_item(&item, protocol).is_none();
        assert!(none(item("", SECRET, &[])), "no user name: {protocol:?}");
        assert!(none(item("anna", "", &[])), "no secret: {protocol:?}");
    }
    let key_only = item("anna", "", &[("privateKey", KEY)]);
    assert!(credentials::from_item(&key_only, Protocol::WebDav).is_none());
    assert!(credentials::from_item(&key_only, Protocol::S3).is_none());
    assert!(credentials::from_item(&key_only, Protocol::Sftp).is_some());
    let empty_key = item("anna", "", &[("privateKey", "")]);
    assert!(credentials::from_item(&empty_key, Protocol::Sftp).is_none());
    assert!(credentials::from_item(&item("anna", SECRET, &[]), Protocol::Local).is_none());
}

#[tokio::test]
async fn a_present_entry_without_its_values_is_missing_until_they_are_back() {
    let (_dir, db) = vault();
    let passwords = PasswordsService::new(db.clone());
    let state = |id: String, protocol| {
        let db = db.clone();
        async move { credentials::state(&db, &id, protocol).await.expect("state") }
    };

    let login = add(&passwords, entry("Box", "anna", SECRET, &[])).await;
    assert_eq!(
        state(login.clone(), Protocol::WebDav).await,
        CredentialsState::Present
    );
    let cleared = ItemPatch {
        password: Patch::Clear,
        ..ItemPatch::default()
    };
    change(&passwords, &login, cleared).await;
    assert_eq!(
        state(login.clone(), Protocol::WebDav).await,
        CredentialsState::Missing,
        "FR-010: the password is gone"
    );
    let restored = ItemPatch {
        password: Patch::Set(SECRET.to_owned()),
        ..ItemPatch::default()
    };
    change(&passwords, &login, restored).await;
    assert_eq!(
        state(login.clone(), Protocol::WebDav).await,
        CredentialsState::Present
    );

    let key = add(&passwords, entry("Key", "anna", "", &[("privateKey", KEY)])).await;
    assert_eq!(
        state(key.clone(), Protocol::Sftp).await,
        CredentialsState::Present
    );
    let without_key = ItemPatch {
        key_values: Some(Vec::<KeyValuePatch>::new()),
        ..ItemPatch::default()
    };
    change(&passwords, &key, without_key).await;
    assert_eq!(
        state(key.clone(), Protocol::Sftp).await,
        CredentialsState::Missing
    );

    passwords
        .trash_targets(
            &Caller::User,
            vec![Target {
                kind: TargetKind::Item,
                id: login.clone(),
            }],
        )
        .await
        .expect("trash");
    assert_eq!(
        state(login, Protocol::WebDav).await,
        CredentialsState::Missing
    );
    assert_eq!(
        state("not-arrived".to_owned(), Protocol::WebDav).await,
        CredentialsState::Syncing
    );
}

#[tokio::test]
async fn the_list_holds_the_fitting_entries_the_user_may_choose_without_a_value() {
    let (_dir, db) = vault();
    let passwords = PasswordsService::new(db.clone());
    let zeta = add(&passwords, entry("zeta", "z", SECRET, &[])).await;
    let mut tagged = entry("Alpha", "a", SECRET, &[]);
    tagged.tags = vec![TAG.to_uppercase()];
    let alpha = add(&passwords, tagged).await;
    let key = add(&passwords, entry("Key", "k", "", &[("privateKey", KEY)])).await;
    add(&passwords, entry("NoUser", "", SECRET, &[])).await;
    let trashed = add(&passwords, entry("Trashed", "t", SECRET, &[])).await;
    passwords
        .trash_targets(
            &Caller::User,
            vec![Target {
                kind: TargetKind::Item,
                id: trashed,
            }],
        )
        .await
        .expect("trash");
    passwords
        .create_owned_item(
            &Caller::Internal { feature: "mail" },
            entry("Mail", "m", SECRET, &[]),
        )
        .await
        .expect("an entry of another HaexVault function");
    let owned = credentials::create(&passwords, Protocol::S3, "RustFS", "", &placeholder())
        .await
        .expect("an entry HaexVault created for a connection");

    let ids = |protocol| {
        let db = db.clone();
        async move {
            let list = credentials::candidates(&db, protocol)
                .await
                .expect("candidates");
            let text = serde_json::to_string(&list).expect("json");
            assert!(
                !text.contains(SECRET)
                    && !text.contains("placeholder-key")
                    && !text.contains("placeholder-secret"),
                "SC-005: no value in the list"
            );
            list
        }
    };
    let webdav = ids(Protocol::WebDav).await;
    assert_eq!(
        webdav.iter().map(|c| c.id.clone()).collect::<Vec<_>>(),
        [alpha.clone(), owned.clone(), zeta.clone()],
        "tagged first, then by title regardless of case"
    );
    assert!(webdav[0].tagged && !webdav[0].owned);
    assert_eq!(webdav[0].url.as_deref(), Some("https://alpha.example/"));
    assert!(
        webdav[1].owned && webdav[1].tagged,
        "HaexVault's own entry is tagged"
    );

    let sftp = ids(Protocol::Sftp).await;
    assert_eq!(
        sftp.iter().map(|c| c.id.clone()).collect::<Vec<_>>(),
        [alpha, owned.clone(), key, zeta]
    );
    let s3 = ids(Protocol::S3).await;
    let listed = s3.iter().find(|c| c.id == owned).expect("listed");
    assert_eq!(listed.username, "AKIDEXAMPLE", "the access key id");

    assert!(matches!(
        credentials::candidates(&db, Protocol::Local).await,
        Err(HaexError::StorageInvalid { ref field }) if field == "protocol"
    ));
}

#[tokio::test]
async fn vault_open_backfills_the_tag_on_old_storage_entries() {
    let (_dir, db) = vault();
    let passwords = PasswordsService::new(db.clone());
    let id = passwords
        .create_owned_item(
            &Caller::Internal {
                feature: credentials::FEATURE,
            },
            entry("Old storage", "AKID", SECRET, &[]),
        )
        .await
        .expect("old storage entry");

    let changed = db
        .write(credentials::backfill_owned_tags)
        .await
        .expect("backfill");
    assert_eq!(changed, 1);
    let tags_of = |id: &str| {
        let id = id.to_owned();
        db.read_blocking(move |q| crate::passwords::tags::names_of_item(q, &id).map_err(Into::into))
            .expect("tags")
    };
    assert!(tags_of(&id).iter().any(|tag| tag == credentials::TAG));
    assert_eq!(
        db.write(credentials::backfill_owned_tags)
            .await
            .expect("again"),
        0,
        "a second open changes nothing"
    );
    let title = passwords
        .get_item(&Caller::User, id.clone())
        .await
        .expect("entry")
        .header
        .title;
    assert_eq!(
        title.as_deref(),
        Some("Old storage"),
        "only the tag changes"
    );

    // Entries of the user and of other haex-vault functions keep their tags.
    let users = add(&passwords, entry("Bank", "anna", SECRET, &[])).await;
    let mail = passwords
        .create_owned_item(
            &Caller::Internal { feature: "mail" },
            entry("Mail", "m", SECRET, &[]),
        )
        .await
        .expect("an entry of another HaexVault function");
    db.write(credentials::backfill_owned_tags)
        .await
        .expect("backfill");
    assert!(tags_of(&users).is_empty());
    assert!(tags_of(&mail).is_empty());
}

/// The tags of the vault as the tag manager sees them: name and color.
fn tags_of_vault(db: &crate::vault_gate::VaultDb) -> Vec<(String, Option<String>)> {
    db.read_blocking(|q| {
        Ok(crate::passwords::items::load_overview(q)?
            .tags
            .into_iter()
            .map(|tag| (tag.name, tag.color))
            .collect())
    })
    .expect("tags")
}

#[tokio::test]
async fn a_vault_without_storage_entries_gets_no_storage_tag() {
    let (_dir, db) = vault();
    assert_eq!(
        db.write(credentials::backfill_owned_tags)
            .await
            .expect("backfill"),
        0
    );
    assert!(
        tags_of_vault(&db).is_empty(),
        "no entry carries the tag, so the tag manager shows none"
    );
}

#[tokio::test]
async fn an_empty_storage_tag_of_an_earlier_version_goes_and_a_used_or_colored_one_stays() {
    let (_dir, db) = vault();
    let make = |color: Option<&'static str>| {
        db.write(move |tx| {
            let id =
                crate::passwords::tags::get_or_create(tx, TAG).map_err(haex_crdt::Error::from)?;
            crate::passwords::tags::set_color(tx, &id, color).map_err(haex_crdt::Error::from)
        })
    };

    // Made for every vault by an earlier version: empty, no color.
    make(None).await.expect("tag");
    db.write(credentials::backfill_owned_tags)
        .await
        .expect("backfill");
    assert!(tags_of_vault(&db).is_empty(), "the empty tag is removed");

    // The user colored it: it is theirs now.
    make(Some("#ff0000")).await.expect("tag");
    db.write(credentials::backfill_owned_tags)
        .await
        .expect("backfill");
    assert_eq!(
        tags_of_vault(&db),
        vec![(TAG.to_owned(), Some("#ff0000".to_owned()))]
    );
}

#[tokio::test]
async fn a_storage_tag_that_an_entry_carries_stays_without_storage_entries() {
    let (_dir, db) = vault();
    let passwords = PasswordsService::new(db.clone());
    let bank = add(&passwords, entry("Bank", "anna", SECRET, &[])).await;
    passwords
        .set_tags(&Caller::User, vec![bank], vec![TAG.to_owned()], Vec::new())
        .await
        .expect("tag the user's entry");
    db.write(credentials::backfill_owned_tags)
        .await
        .expect("backfill");
    assert_eq!(tags_of_vault(&db).len(), 1, "the user's tag stays");
}
