//! The login entry of a marketplace (spec 069 T057): the three states, the headers an entry
//! makes, what the form creates, what it leaves alone, what the password manager warns about, and
//! how a row becomes a source with or without its login.

use std::sync::Arc;

use super::*;
use crate::marketplace::catalog::Source;
use crate::marketplace::commands::{list_views, source_of, sources_of};
use crate::marketplace::credentials;
use crate::marketplace::manage::{delete, save};
use crate::marketplace::model::MarketplaceInput;
use crate::marketplace::store::MarketplaceRow;
use crate::marketplace::test_support::vault;
use crate::marketplace::{kinds, model::SourceErrorKind};
use crate::passwords::items;
use crate::passwords::model::{ItemHeader, Target, TargetKind};
use crate::passwords::usage::UsageRegistry;

const TOKEN: &str = "placeholder-token-of-credentials-tests";

fn passwords(db: &VaultDb) -> PasswordsService {
    PasswordsService::new(db.clone())
}

fn typed(username: Option<&str>, secret: &str) -> MarketplaceNewCredentials {
    MarketplaceNewCredentials {
        username: username.map(str::to_owned),
        secret: secret.to_owned().into(),
    }
}

fn input(name: &str, auth: MarketplaceAuthType) -> MarketplaceInput {
    MarketplaceInput {
        id: None,
        name: name.to_owned(),
        base_url: "https://market.example".to_owned(),
        auth_type: auth,
        credentials_item_id: None,
        new_credentials: None,
    }
}

/// A new entry of the user.
async fn user_entry(
    passwords: &PasswordsService,
    title: &str,
    username: Option<&str>,
    password: Option<&str>,
    tags: &[&str],
) -> String {
    passwords
        .create_item(
            &Caller::User,
            &[],
            ItemInput {
                title: Some(title.to_owned()),
                username: username.map(str::to_owned),
                password: password.map(str::to_owned),
                tags: tags.iter().map(|tag| (*tag).to_owned()).collect(),
                ..ItemInput::default()
            },
            None,
        )
        .await
        .expect("a user entry")
}

async fn header(db: &VaultDb, id: &str) -> Option<ItemHeader> {
    let id = id.to_owned();
    db.read(move |q| {
        Ok(items::headers_in_scope(q, &Scope::All, &Caller::User)?
            .into_iter()
            .find(|header| header.id == id))
    })
    .await
    .expect("headers")
}

fn row(id: &str, auth: MarketplaceAuthType, item: Option<&str>, url: &str) -> MarketplaceRow {
    MarketplaceRow {
        id: id.to_owned(),
        name: format!("Name {id}"),
        base_url: url.to_owned(),
        enabled: true,
        sort_order: 1,
        auth_type: auth,
        credentials_item_id: item.map(str::to_owned),
    }
}

fn reason(error: HolziError) -> String {
    match error {
        HolziError::Marketplace { reason } => reason,
        other => panic!("not a marketplace error: {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn an_entry_is_present_missing_or_not_yet_arrived() {
    let (_dir, db) = vault();
    let pw = passwords(&db);
    let bearer = MarketplaceAuthType::Bearer;

    let good = user_entry(&pw, "Token", None, Some(TOKEN), &[]).await;
    assert_eq!(
        credentials::state(&db, bearer, &good).await.unwrap(),
        MarketplaceCredentialsState::Present
    );
    assert_eq!(
        credentials::state(&db, bearer, "not-arrived")
            .await
            .unwrap(),
        MarketplaceCredentialsState::Syncing
    );

    let empty = user_entry(&pw, "No token", Some("someone"), None, &[]).await;
    assert_eq!(
        credentials::state(&db, bearer, &empty).await.unwrap(),
        MarketplaceCredentialsState::Missing,
        "an entry without a password cannot log in"
    );

    pw.trash_targets(
        &Caller::User,
        vec![Target {
            kind: TargetKind::Item,
            id: good.clone(),
        }],
    )
    .await
    .expect("the user moves it to the trash");
    assert_eq!(
        credentials::state(&db, bearer, &good).await.unwrap(),
        MarketplaceCredentialsState::Missing
    );

    let created = credentials::create(
        &pw,
        "Team",
        "https://market.example",
        bearer,
        &typed(None, TOKEN),
    )
    .await
    .unwrap();
    credentials::delete(&pw, &created).await.unwrap();
    assert_eq!(
        credentials::state(&db, bearer, &created).await.unwrap(),
        MarketplaceCredentialsState::Missing,
        "the delete marker says it was deleted, not that it is on its way"
    );
    credentials::delete(&pw, &created)
        .await
        .expect("deleting it again is fine");
}

#[tokio::test(flavor = "multi_thread")]
async fn bearer_and_basic_headers_are_built_from_entries() {
    let (_dir, db) = vault();
    let pw = passwords(&db);
    let ready = |login: Login| match login {
        Login::Ready(header) => header.expose().to_owned(),
        other => panic!("not ready: {other:?}"),
    };

    let token = user_entry(&pw, "Token", None, Some(TOKEN), &[]).await;
    let login = credentials::read(&pw, &db, MarketplaceAuthType::Bearer, &token)
        .await
        .unwrap();
    assert_eq!(ready(login), format!("Bearer {TOKEN}"));

    let account = user_entry(&pw, "Account", Some("ada"), Some("pass"), &[]).await;
    let login = credentials::read(&pw, &db, MarketplaceAuthType::Basic, &account)
        .await
        .unwrap();
    assert_eq!(ready(login), "Basic YWRhOnBhc3M=");

    let nameless = credentials::read(&pw, &db, MarketplaceAuthType::Basic, &token)
        .await
        .unwrap();
    assert!(
        matches!(nameless, Login::Missing),
        "basic needs a user name"
    );

    let unknown = credentials::read(&pw, &db, MarketplaceAuthType::Bearer, "later")
        .await
        .unwrap();
    assert!(matches!(unknown, Login::Syncing));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_form_creates_a_tagged_entry_only_the_user_and_this_module_see() {
    let (_dir, db) = vault();
    let pw = passwords(&db);
    let mut form = input("Private", MarketplaceAuthType::Bearer);
    form.new_credentials = Some(typed(None, TOKEN));

    let saved = save(&db, &pw, form, 1).await.unwrap();
    let item = saved
        .credentials_item_id
        .clone()
        .expect("the row points at an entry");
    assert_eq!(saved.credentials, MarketplaceCredentialsState::Present);
    let entry = saved.credentials_entry.expect("its title shows");
    assert_eq!(entry.title.as_deref(), Some("Marketplace: Private"));
    assert!(entry.owned);

    let header = header(&db, &item).await.expect("the user sees it");
    assert_eq!(header.owner.as_deref(), Some(FEATURE));
    assert!(header.tags.iter().any(|tag| tag.name == TAG));

    let other = Caller::Internal { feature: "storage" };
    let hidden = pw
        .read_secret_item(&other, &grants(), item.clone())
        .await
        .unwrap_err();
    assert!(
        matches!(hidden, HolziError::PasswordsNotFound),
        "rule Z14: another holzi function does not see it"
    );
    assert!(matches!(
        credentials::read(&pw, &db, MarketplaceAuthType::Bearer, &item)
            .await
            .unwrap(),
        Login::Ready(_)
    ));
}

#[tokio::test(flavor = "multi_thread")]
async fn deleting_the_entry_in_the_password_manager_warns_and_names_the_marketplace() {
    let (_dir, db) = vault();
    let usage = Arc::new(UsageRegistry::new());
    let handle = db.clone();
    usage.register(Arc::new(MarketplaceUsage::new(move || {
        Some(handle.clone())
    })));
    let pw = PasswordsService::with_usage(db.clone(), usage);

    let mut form = input("Private", MarketplaceAuthType::Bearer);
    form.new_credentials = Some(typed(None, TOKEN));
    let created = save(&db, &pw, form, 1)
        .await
        .unwrap()
        .credentials_item_id
        .unwrap();
    let chosen = user_entry(&pw, "Mine", None, Some(TOKEN), &[]).await;
    let unused = user_entry(&pw, "Unused", None, Some(TOKEN), &[]).await;
    let mut form = input("Other", MarketplaceAuthType::Bearer);
    form.credentials_item_id = Some(chosen.clone());
    save(&db, &pw, form, 2).await.unwrap();

    let features = tokio::task::spawn_blocking(move || {
        [created, chosen, unused].map(|id| pw.item_usage(&Caller::User, &id).expect("usage"))
    })
    .await
    .expect("join");
    let named = vec![FEATURE.to_owned()];
    assert_eq!(features, [named.clone(), named, Vec::<String>::new()]);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_entry_the_user_chose_keeps_its_owner_and_tags_and_outlives_the_marketplace() {
    let (_dir, db) = vault();
    let pw = passwords(&db);
    let mine = user_entry(&pw, "Mine", None, Some(TOKEN), &["work"]).await;
    let mut form = input("Private", MarketplaceAuthType::Bearer);
    form.credentials_item_id = Some(mine.clone());

    let saved = save(&db, &pw, form, 1).await.unwrap();

    let header_after = header(&db, &mine).await.expect("still there");
    assert_eq!(header_after.owner, None, "it stays the user's own entry");
    let tags: Vec<&str> = header_after.tags.iter().map(|t| t.name.as_str()).collect();
    assert!(tags.contains(&"work") && tags.contains(&TAG), "{tags:?}");
    assert!(!saved.credentials_entry.expect("title").owned);

    delete(&db, &pw, saved.id).await.unwrap();
    assert!(
        header(&db, &mine).await.is_some(),
        "an entry of the user is never deleted with a marketplace"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_entry_the_form_created_goes_when_the_marketplace_goes_or_stops_needing_it() {
    let (_dir, db) = vault();
    let pw = passwords(&db);
    let mut form = input("Private", MarketplaceAuthType::Basic);
    form.new_credentials = Some(typed(Some("ada"), "pass"));
    let first = save(&db, &pw, form.clone(), 1).await.unwrap();
    let first_item = first.credentials_item_id.clone().unwrap();

    // Typing a new login replaces the entry.
    let mut change = form.clone();
    change.id = Some(first.id.clone());
    change.new_credentials = Some(typed(Some("ada"), "other"));
    let changed = save(&db, &pw, change, 2).await.unwrap();
    let second_item = changed.credentials_item_id.clone().unwrap();
    assert_ne!(first_item, second_item);
    assert!(
        header(&db, &first_item).await.is_none(),
        "the old entry went"
    );

    // Switching the login off removes the entry too.
    let mut off = input("Private", MarketplaceAuthType::None);
    off.id = Some(first.id.clone());
    let off = save(&db, &pw, off, 3).await.unwrap();
    assert_eq!(off.credentials_item_id, None);
    assert!(header(&db, &second_item).await.is_none());

    // And deleting the marketplace.
    let third = save(&db, &pw, form, 4).await.unwrap();
    let third_item = third.credentials_item_id.clone().unwrap();
    delete(&db, &pw, third.id).await.unwrap();
    assert!(header(&db, &third_item).await.is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_refused_form_leaves_no_entry_behind() {
    let (_dir, db) = vault();
    let pw = passwords(&db);

    let mut http = input("Plain", MarketplaceAuthType::Bearer);
    http.base_url = "http://market.example".to_owned();
    http.new_credentials = Some(typed(None, TOKEN));
    assert_eq!(
        reason(save(&db, &pw, http, 1).await.unwrap_err()),
        kinds::INSECURE_CREDENTIALS
    );

    let mut nameless = input("Basic", MarketplaceAuthType::Basic);
    nameless.new_credentials = Some(typed(None, "pass"));
    assert!(matches!(
        save(&db, &pw, nameless, 2).await.unwrap_err(),
        HolziError::InvalidInput { .. }
    ));

    let mut missing = input("Nothing", MarketplaceAuthType::Bearer);
    missing.new_credentials = None;
    assert_eq!(
        reason(save(&db, &pw, missing, 3).await.unwrap_err()),
        kinds::CREDENTIALS_REQUIRED
    );

    let listed = credentials::candidates(&db, MarketplaceAuthType::Bearer)
        .await
        .unwrap();
    assert!(listed.is_empty(), "nothing was created: {listed:?}");
    assert!(list_views(&db)
        .await
        .unwrap()
        .iter()
        .all(|v| v.name != "Plain"));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_list_to_choose_from_fits_the_auth_type_and_shows_no_secret() {
    let (_dir, db) = vault();
    let pw = passwords(&db);
    let beta = user_entry(&pw, "beta", None, Some(TOKEN), &[]).await;
    let alpha = user_entry(&pw, "Alpha", Some("ada"), Some("pass"), &[]).await;
    let tagged = user_entry(&pw, "Zulu", Some("zed"), Some("pass"), &[TAG]).await;
    user_entry(&pw, "No password", Some("ada"), None, &[]).await;
    pw.create_owned_item(
        &Caller::Internal { feature: "storage" },
        ItemInput {
            title: Some("S3: other".to_owned()),
            username: Some("AKID".to_owned()),
            password: Some("secret".to_owned()),
            ..ItemInput::default()
        },
    )
    .await
    .unwrap();

    let ids = |list: Vec<_>| -> Vec<String> {
        list.into_iter()
            .map(|c: crate::remote_storage::model::CredentialsCandidate| c.id)
            .collect()
    };
    let basic = credentials::candidates(&db, MarketplaceAuthType::Basic)
        .await
        .unwrap();
    assert!(!format!("{basic:?}").contains(TOKEN));
    assert_eq!(
        ids(basic),
        [tagged.clone(), alpha.clone()],
        "tagged first, no token-only entry"
    );
    let bearer = credentials::candidates(&db, MarketplaceAuthType::Bearer)
        .await
        .unwrap();
    assert_eq!(
        ids(bearer),
        [tagged, alpha, beta],
        "a token needs no user name; another function's entry is no choice"
    );
    assert!(credentials::candidates(&db, MarketplaceAuthType::None)
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_row_is_a_source_with_its_login_or_without_a_request() {
    let (_dir, db) = vault();
    let pw = passwords(&db);
    let item = user_entry(&pw, "Token", None, Some(TOKEN), &[]).await;
    let ask = |row: MarketplaceRow| {
        let (pw, db) = (passwords(&db), db.clone());
        async move { source_of(&pw, &db, &row).await.unwrap() }
    };

    let open = ask(row(
        "a",
        MarketplaceAuthType::None,
        None,
        "https://a.example",
    ))
    .await;
    assert!(open.auth.is_none() && open.unavailable.is_none());

    let login = ask(row(
        "b",
        MarketplaceAuthType::Bearer,
        Some(&item),
        "https://b.example",
    ))
    .await;
    assert!(login.auth.is_some() && login.unavailable.is_none());

    let later = ask(row(
        "c",
        MarketplaceAuthType::Bearer,
        Some("later"),
        "https://c.example",
    ))
    .await;
    assert_eq!(later.unavailable, Some(SourceErrorKind::CredentialsSyncing));
    assert!(later.auth.is_none());

    let none = ask(row(
        "d",
        MarketplaceAuthType::Basic,
        None,
        "https://d.example",
    ))
    .await;
    assert_eq!(none.unavailable, Some(SourceErrorKind::CredentialsMissing));

    let wrong_kind = ask(row(
        "e",
        MarketplaceAuthType::Basic,
        Some(&item),
        "https://e.example",
    ))
    .await;
    assert_eq!(
        wrong_kind.unavailable,
        Some(SourceErrorKind::CredentialsMissing)
    );

    let plain = ask(row(
        "f",
        MarketplaceAuthType::Bearer,
        Some(&item),
        "http://f.example",
    ))
    .await;
    assert_eq!(
        plain.unavailable,
        Some(SourceErrorKind::InsecureCredentials)
    );
    assert!(plain.auth.is_none(), "the login is not even read for http");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_disabled_marketplace_is_not_a_source() {
    let (_dir, db) = vault();
    let mut off = row("b", MarketplaceAuthType::None, None, "https://b.example");
    off.enabled = false;
    let rows = [
        row("a", MarketplaceAuthType::None, None, "https://a.example"),
        off,
        row("c", MarketplaceAuthType::Basic, None, "https://c.example"),
    ];

    let sources: Vec<Source> = sources_of(&passwords(&db), &db, &rows, None)
        .await
        .unwrap();

    let ids: Vec<&str> = sources.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, ["a", "c"]);
    assert_eq!(
        sources[1].unavailable,
        Some(SourceErrorKind::CredentialsMissing)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_list_shows_the_state_of_each_login() {
    let (_dir, db) = vault();
    let pw = passwords(&db);
    let item = user_entry(&pw, "Token", None, Some(TOKEN), &[]).await;
    let mut form = input("Private", MarketplaceAuthType::Bearer);
    form.credentials_item_id = Some(item.clone());
    save(&db, &pw, form, 1).await.unwrap();
    let mut form = input("Open", MarketplaceAuthType::None);
    form.base_url = "https://open.example".to_owned();
    save(&db, &pw, form, 2).await.unwrap();

    let states = |views: Vec<crate::marketplace::model::MarketplaceView>| {
        views.into_iter().map(|v| v.credentials).collect::<Vec<_>>()
    };
    let before = states(list_views(&db).await.unwrap());
    assert!(before.contains(&MarketplaceCredentialsState::Present));
    assert!(before.contains(&MarketplaceCredentialsState::None));

    pw.trash_targets(
        &Caller::User,
        vec![Target {
            kind: TargetKind::Item,
            id: item,
        }],
    )
    .await
    .unwrap();
    let after = states(list_views(&db).await.unwrap());
    assert!(after.contains(&MarketplaceCredentialsState::Missing));
    assert!(!after.contains(&MarketplaceCredentialsState::Present));
}
