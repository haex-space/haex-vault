use std::sync::Arc;

use super::*;
use crate::error::HolziError;
use crate::marketplace::kinds;
use crate::marketplace::model::MarketplaceAuthType;
use crate::marketplace::store::{PRESET_ID, PRESET_NAME};
use crate::marketplace::test_support::{entry, page, version, TestMarketplace};
use crate::passwords::test_support::open_test_vault;
use crate::vault_gate::VaultGate;

fn vault() -> (tempfile::TempDir, VaultDb) {
    let (dir, db) = open_test_vault();
    let vault = VaultGate::new().vault_db(Arc::new(db)).unwrap();
    (dir, vault)
}

fn input(name: &str, url: &str) -> MarketplaceInput {
    MarketplaceInput {
        id: None,
        name: name.to_owned(),
        base_url: url.to_owned(),
        auth_type: MarketplaceAuthType::None,
        credentials_item_id: None,
    }
}

fn reason(error: HolziError) -> String {
    match error {
        HolziError::Marketplace { reason } => reason,
        other => panic!("not a marketplace error: {other:?}"),
    }
}

async fn order(db: &VaultDb) -> Vec<String> {
    views(&rows(db).await.unwrap())
        .into_iter()
        .map(|view| view.id)
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_marketplace_is_saved_with_a_normalized_address_and_can_be_changed() {
    let (_dir, db) = vault();

    let saved = save(
        &db,
        input("  My own  ", "https://Market.Example/team/ "),
        10,
    )
    .await
    .unwrap();
    assert_eq!(saved.name, "My own");
    assert_eq!(saved.base_url, "https://market.example/team");
    assert!(saved.enabled);
    assert_eq!(saved.rank, 1);

    let mut change = input("Renamed", "http://192.168.1.5:3000");
    change.id = Some(saved.id.clone());
    let changed = save(&db, change, 20).await.unwrap();
    assert_eq!(changed.id, saved.id, "an update keeps the row");
    assert_eq!(changed.name, "Renamed");
    assert_eq!(changed.base_url, "http://192.168.1.5:3000");
    assert_eq!(order(&db).await, [saved.id]);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_address_that_is_no_http_address_is_refused() {
    let (_dir, db) = vault();
    for bad in ["ftp://market.example", "market.example", "https://", ""] {
        let error = save(&db, input("x", bad), 1).await.unwrap_err();
        assert_eq!(reason(error), kinds::INVALID_URL, "{bad}");
    }
    assert!(matches!(
        save(&db, input("   ", "https://market.example"), 1).await,
        Err(HolziError::InvalidInput { .. })
    ));
    assert!(order(&db).await.is_empty(), "nothing was written");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_login_needs_https_and_an_entry() {
    let (_dir, db) = vault();
    let mut with_login = input("Private", "http://market.example");
    with_login.auth_type = MarketplaceAuthType::Bearer;
    with_login.credentials_item_id = Some("item".into());
    assert_eq!(
        reason(save(&db, with_login.clone(), 1).await.unwrap_err()),
        kinds::INSECURE_CREDENTIALS
    );
    with_login.base_url = "https://market.example".into();
    with_login.credentials_item_id = None;
    assert_eq!(
        reason(save(&db, with_login, 1).await.unwrap_err()),
        kinds::CREDENTIALS_REQUIRED
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn listing_keeps_the_credentials_entry_id_available_to_the_edit_form() {
    let (_dir, db) = vault();
    let mut with_login = input("Private", "https://market.example");
    with_login.auth_type = MarketplaceAuthType::Bearer;
    with_login.credentials_item_id = Some("password-item".into());

    let saved = save(&db, with_login.clone(), 1).await.unwrap();
    assert_eq!(saved.credentials_item_id.as_deref(), Some("password-item"));

    with_login.id = Some(saved.id.clone());
    with_login.name = "Renamed".into();
    with_login.credentials_item_id = saved.credentials_item_id.clone();
    let updated = save(&db, with_login, 2).await.unwrap();
    assert_eq!(updated.name, "Renamed");
    assert_eq!(
        updated.credentials_item_id.as_deref(),
        Some("password-item")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_order_of_the_list_is_the_order_the_user_chose() {
    let (_dir, db) = vault();
    let a = save(&db, input("A", "https://a.example"), 1)
        .await
        .unwrap()
        .id;
    let b = save(&db, input("B", "https://b.example"), 2)
        .await
        .unwrap()
        .id;
    let c = save(&db, input("C", "https://c.example"), 3)
        .await
        .unwrap()
        .id;
    assert_eq!(order(&db).await, [a.clone(), b.clone(), c.clone()]);

    reorder(&db, vec![c.clone(), a.clone(), b.clone()], 4)
        .await
        .unwrap();

    let ranked = views(&rows(&db).await.unwrap());
    assert_eq!(
        ranked
            .iter()
            .map(|v| (v.id.as_str(), v.rank))
            .collect::<Vec<_>>(),
        [(c.as_str(), 1), (a.as_str(), 2), (b.as_str(), 3)]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_marketplace_can_be_switched_off_and_on() {
    let (_dir, db) = vault();
    let id = save(&db, input("A", "https://a.example"), 1)
        .await
        .unwrap()
        .id;

    set_enabled(&db, id.clone(), false, 2).await.unwrap();
    assert!(!views(&rows(&db).await.unwrap())[0].enabled);
    set_enabled(&db, id.clone(), true, 3).await.unwrap();
    assert!(views(&rows(&db).await.unwrap())[0].enabled);

    assert_eq!(
        reason(
            set_enabled(&db, "unknown".into(), false, 4)
                .await
                .unwrap_err()
        ),
        kinds::NOT_FOUND
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_preset_is_deleted_like_any_row_and_comes_back_only_when_asked_for() {
    let (_dir, db) = vault();
    let preset = add_preset(&db, 1).await.unwrap();
    assert_eq!(
        (preset.id.as_str(), preset.name.as_str()),
        (PRESET_ID, PRESET_NAME)
    );
    let own = save(&db, input("Own", "https://own.example"), 2)
        .await
        .unwrap()
        .id;

    delete(&db, PRESET_ID.into()).await.unwrap();

    assert_eq!(order(&db).await, std::slice::from_ref(&own));
    // Listing, saving another one, switching: nothing brings it back.
    set_enabled(&db, own.clone(), false, 3).await.unwrap();
    save(&db, input("Third", "https://third.example"), 4)
        .await
        .unwrap();
    assert!(!order(&db).await.contains(&PRESET_ID.to_owned()));

    let again = add_preset(&db, 5).await.unwrap();
    assert_eq!(again.id, PRESET_ID);
    assert_eq!(
        order(&db)
            .await
            .iter()
            .filter(|id| *id == PRESET_ID)
            .count(),
        1
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn adding_the_preset_twice_changes_nothing_even_after_it_was_switched_off() {
    let (_dir, db) = vault();
    add_preset(&db, 1).await.unwrap();
    set_enabled(&db, PRESET_ID.into(), false, 2).await.unwrap();

    let second = add_preset(&db, 3).await.unwrap();

    assert!(!second.enabled, "the user's switch stays");
    assert_eq!(order(&db).await, [PRESET_ID.to_owned()]);
}

// ---- the test of a marketplace ----

fn source(base_url: String) -> Source {
    Source {
        id: "t".into(),
        name: "t".into(),
        base_url,
        auth: None,
        unavailable: None,
    }
}

#[tokio::test]
async fn a_reachable_marketplace_reports_its_number_of_entries() {
    let market = TestMarketplace::start().await;
    market.healthy().await;
    market
        .json(
            "/extensions",
            200,
            page(vec![entry("a", 1, vec![version("1.0.0", 1)])], 1, 1, 7),
        )
        .await;

    let result = test_marketplace(Http::default(), &source(market.base_url())).await;

    assert_eq!(
        result,
        MarketplaceTestResult {
            ok: true,
            count: Some(7),
            error: None,
            detail: None
        }
    );
}

#[tokio::test]
async fn a_marketplace_that_refuses_the_login_says_so() {
    let market = TestMarketplace::start().await;
    market.raw("/health", 401, "login required").await;

    let result = test_marketplace(Http::default(), &source(market.base_url())).await;

    assert!(!result.ok);
    assert_eq!(result.error, Some(SourceErrorKind::AuthRejected));
}

#[tokio::test]
async fn an_unreachable_marketplace_is_reported_as_such() {
    let result = test_marketplace(Http::default(), &source("http://127.0.0.1:1".to_owned())).await;

    assert!(!result.ok);
    assert_eq!(result.error, Some(SourceErrorKind::Unreachable));
}

#[tokio::test]
async fn an_answer_that_is_no_marketplace_is_a_bad_response_and_is_not_quoted() {
    let market = TestMarketplace::start().await;
    market.healthy().await;
    market
        .raw("/extensions", 200, "SECRET-ECHO <html>not json</html>")
        .await;

    let result = test_marketplace(Http::default(), &source(market.base_url())).await;

    assert_eq!(result.error, Some(SourceErrorKind::BadResponse));
    assert!(!format!("{result:?}").contains("SECRET-ECHO"));
}

#[tokio::test]
async fn a_marketplace_that_needs_a_login_is_not_asked_without_one() {
    let market = TestMarketplace::start().await;
    let mut login = input("Private", &market.base_url());
    login.auth_type = MarketplaceAuthType::Bearer;
    login.credentials_item_id = Some("item".into());
    let asked = source_of_input(&login).unwrap();

    let result = test_marketplace(Http::default(), &asked).await;

    assert_eq!(result.error, Some(SourceErrorKind::CredentialsMissing));
    assert!(market.requests().await.is_empty());
}

#[tokio::test]
async fn the_test_of_an_invalid_address_is_an_error_of_the_command() {
    let error = marketplace_test(input("x", "ftp://market.example"))
        .await
        .unwrap_err();
    assert_eq!(reason(error), kinds::INVALID_URL);
}
