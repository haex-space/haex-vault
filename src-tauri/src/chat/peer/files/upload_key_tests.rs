//! The Blossom upload key (spec 072 FR-081): made as an entry of the password manager that holds
//! the only copy of the secret, chosen from the user's own entries, removed without touching an
//! entry of the user, reported as present, missing or not yet arrived, and named in the delete
//! warning of the password manager.

use std::sync::Arc;

use super::*;
use crate::passwords::model::{Target, TargetKind};
use crate::passwords::test_support::open_test_vault;
use crate::passwords::usage::UsageRegistry;
use crate::vault_gate::VaultGate;

/// A vault with the tables of the password manager and the preferences; keep the directory alive.
fn vault() -> (tempfile::TempDir, VaultDb) {
    let (dir, db) = open_test_vault();
    let vault = VaultGate::new().vault_db(Arc::new(db)).unwrap();
    (dir, vault)
}

fn passwords(db: &VaultDb) -> PasswordsService {
    PasswordsService::new(db.clone())
}

/// A new entry of the user.
async fn user_entry(passwords: &PasswordsService, title: &str, password: Option<&str>) -> String {
    passwords
        .create_item(
            &Caller::User,
            &[],
            ItemInput {
                title: Some(title.to_owned()),
                password: password.map(str::to_owned),
                ..ItemInput::default()
            },
            None,
        )
        .await
        .expect("a user entry")
}

async fn password_of(passwords: &PasswordsService, id: &str) -> String {
    passwords
        .read_secret_item(&CALLER, &grants(), id.to_owned())
        .await
        .expect("the entry")
        .password
        .expect("a password")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_created_key_is_an_entry_that_holds_the_secret_and_uploads_find_it() {
    let (_dir, db) = vault();
    let pw = passwords(&db);
    assert!(matches!(
        resolve(&pw, &db).await.expect("resolve"),
        Resolved::Unset
    ));
    assert_eq!(
        describe(&pw, &db).await.expect("describe").status,
        UploadKeyStatus::None
    );

    let id = create(&pw, &db).await.expect("create");
    assert_eq!(entry_id(&db).await.expect("setting"), Some(id.clone()));
    let secret = password_of(&pw, &id).await;
    assert!(secret.starts_with("nsec1"), "the secret is an nsec");

    let Resolved::Ready(keys, entry) = resolve(&pw, &db).await.expect("resolve") else {
        panic!("the key is ready");
    };
    assert_eq!(entry, id);
    assert_eq!(keys.secret_key().to_bech32().expect("nsec"), secret);

    let shown = describe(&pw, &db).await.expect("describe");
    assert_eq!(shown.status, UploadKeyStatus::Present);
    assert_eq!(
        shown.npub,
        Some(keys.public_key().to_bech32().expect("npub"))
    );
    assert!(shown.entry.is_some_and(|entry| entry.owned));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_second_key_is_refused_while_one_is_set() {
    let (_dir, db) = vault();
    let pw = passwords(&db);
    create(&pw, &db).await.expect("create");
    assert!(create(&pw, &db).await.is_err());
    let other = user_entry(
        &pw,
        "Mein Schlüssel",
        Some(&Keys::generate().secret_key().to_bech32().unwrap()),
    )
    .await;
    assert!(choose(&pw, &db, &other).await.is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn removing_deletes_the_entry_the_module_made_and_keeps_one_of_the_user() {
    let (_dir, db) = vault();
    let pw = passwords(&db);
    let made = create(&pw, &db).await.expect("create");
    remove(&pw, &db).await.expect("remove");
    assert_eq!(entry_id(&db).await.expect("setting"), None);
    assert_eq!(
        state(&db, &made).await.expect("state"),
        CredentialsState::Missing,
        "the entry is gone for good"
    );

    let secret = Keys::generate().secret_key().to_bech32().unwrap();
    let mine = user_entry(&pw, "Mein Schlüssel", Some(&secret)).await;
    choose(&pw, &db, &mine).await.expect("choose");
    remove(&pw, &db).await.expect("remove");
    assert_eq!(entry_id(&db).await.expect("setting"), None);
    assert_eq!(
        state(&db, &mine).await.expect("state"),
        CredentialsState::Present,
        "an entry of the user stays"
    );
    assert_eq!(password_of(&pw, &mine).await, secret);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn only_entries_that_hold_a_key_can_be_chosen_and_choosing_tags_the_entry() {
    let (_dir, db) = vault();
    let pw = passwords(&db);
    let nsec = Keys::generate().secret_key().to_bech32().unwrap();
    let hex = Keys::generate().secret_key().to_secret_hex();
    let good = user_entry(&pw, "nsec", Some(&nsec)).await;
    let as_hex = user_entry(&pw, "hex", Some(&hex)).await;
    let word = user_entry(&pw, "Passwort", Some("correct horse battery staple")).await;
    let empty = user_entry(&pw, "leer", None).await;

    let listed: Vec<String> = candidates(&db)
        .await
        .expect("candidates")
        .into_iter()
        .map(|candidate| candidate.id)
        .collect();
    assert!(listed.contains(&good) && listed.contains(&as_hex));
    assert!(!listed.contains(&word) && !listed.contains(&empty));
    assert!(choose(&pw, &db, &word).await.is_err());
    assert_eq!(entry_id(&db).await.expect("setting"), None);

    choose(&pw, &db, &good).await.expect("choose");
    assert_eq!(entry_id(&db).await.expect("setting"), Some(good.clone()));
    let tagged = candidates(&db).await.expect("candidates");
    assert!(tagged
        .iter()
        .any(|candidate| candidate.id == good && candidate.tagged));
    let Resolved::Ready(keys, _) = resolve(&pw, &db).await.expect("resolve") else {
        panic!("the key is ready");
    };
    assert_eq!(keys.secret_key().to_bech32().expect("nsec"), nsec);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_set_key_whose_entry_is_gone_is_missing_not_unset() {
    let (_dir, db) = vault();
    let pw = passwords(&db);
    let id = create(&pw, &db).await.expect("create");
    pw.trash_targets(
        &Caller::User,
        vec![Target {
            kind: TargetKind::Item,
            id: id.clone(),
        }],
    )
    .await
    .expect("the user moves it to the trash");
    assert!(
        matches!(resolve(&pw, &db).await.expect("resolve"), Resolved::Missing),
        "uploads must not fall back to a fresh key"
    );
    assert_eq!(
        describe(&pw, &db).await.expect("describe").status,
        UploadKeyStatus::Missing
    );

    // An entry the sync has not brought yet.
    db.write(|tx| {
        preferences::insert_or_update(
            tx,
            PrefScope::Vault,
            settings::KEY_BLOSSOM_KEY_ENTRY,
            "00000000-0000-4000-8000-000000000000",
        )?;
        Ok(())
    })
    .await
    .expect("set");
    assert!(matches!(
        resolve(&pw, &db).await.expect("resolve"),
        Resolved::Syncing
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_password_manager_warns_before_the_key_entry_is_deleted() {
    let (_dir, db) = vault();
    let pw = passwords(&db);
    let id = create(&pw, &db).await.expect("create");
    let registry = UsageRegistry::new();
    let database = db.clone();
    registry.register(std::sync::Arc::new(ChatUsage::new(move || {
        Some(database.clone())
    })));
    assert_eq!(registry.used_by(&id), vec![FEATURE]);
    assert!(registry.used_by("another").is_empty());
}
