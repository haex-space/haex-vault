//! No secret of a marketplace login ends up where it must not (spec 069 SC-004, SC-005, T062): the
//! row of the table holds only the id of the entry, no log line and no error shows the token or the
//! password even when the marketplace echoes them, `Debug` of every type that touches a login
//! hides it, and the redirect and `http` rules hold with a real header made from an entry.

use semver::Version;
use wiremock::matchers::{method, path};
use wiremock::{Mock, Request, ResponseTemplate};

use super::catalog::{fetch_catalog, Source};
use super::client::{AuthHeader, Http};
use super::credentials::{self, header_of_typed};
use super::manage::{save, test_marketplace};
use super::model::{
    CatalogQuery, MarketplaceAuthType, MarketplaceInput, MarketplaceNewCredentials, SourceErrorKind,
};
use super::test_support::{entry, page, vault, TestMarketplace};
use crate::passwords::service::PasswordsService;
use crate::storage::query::Query;

const TOKEN: &str = "tok-5f1c0d8e-never-in-a-log";
const PASSWORD: &str = "pw-9a2b7c4d-never-in-a-log";

fn typed(username: Option<&str>, secret: &str) -> MarketplaceNewCredentials {
    MarketplaceNewCredentials {
        username: username.map(str::to_owned),
        secret: secret.to_owned().into(),
    }
}

fn source(market: &TestMarketplace, auth: Option<AuthHeader>) -> Source {
    Source {
        id: "private".to_owned(),
        name: "Private".to_owned(),
        base_url: market.base_url(),
        auth,
        unavailable: None,
    }
}

fn query() -> CatalogQuery {
    CatalogQuery {
        page: 1,
        search: None,
        category: None,
        source_id: None,
    }
}

/// Every route answers with the `Authorization` header it received, as a badly written server may.
async fn echo_everything_as_an_error(market: &TestMarketplace) {
    Mock::given(method("GET"))
        .and(path("/health"))
        .respond_with(|request: &Request| {
            let received = request
                .headers
                .get("authorization")
                .and_then(|value| value.to_str().ok())
                .unwrap_or("none")
                .to_owned();
            ResponseTemplate::new(500).set_body_string(format!("bad login: {received}"))
        })
        .mount(&market.server)
        .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_row_holds_only_the_id_of_the_entry() {
    let (_dir, db) = vault();
    let passwords = PasswordsService::new(db.clone());
    let mut form = MarketplaceInput {
        id: None,
        name: "Private".to_owned(),
        base_url: "https://market.example".to_owned(),
        auth_type: MarketplaceAuthType::Basic,
        credentials_item_id: None,
        new_credentials: Some(typed(Some("ada-the-user"), PASSWORD)),
    };
    let saved = save(&db, &passwords, form.clone(), 1).await.unwrap();
    form.id = Some(saved.id.clone());
    form.new_credentials = Some(typed(Some("ada-the-user"), TOKEN));
    let saved = save(&db, &passwords, form, 2).await.unwrap();
    let item = saved.credentials_item_id.clone().expect("an entry id");

    let columns = [
        "id",
        "name",
        "base_url",
        "enabled",
        "sort_order",
        "auth_type",
        "credentials_item_id",
        "created_at",
        "updated_at",
    ];
    let text = db
        .read(move |q| {
            let mut text = String::new();
            for column in columns {
                let sql = format!(
                    "SELECT COALESCE(group_concat(CAST({column} AS TEXT), '|'), '') \
                     FROM haex_marketplaces"
                );
                text.push_str(
                    &q.query_row(&sql, &[], |row| row.get::<_, String>(0))?
                        .unwrap_or_default(),
                );
                text.push('\n');
            }
            Ok(text)
        })
        .await
        .expect("the row");

    // The statement that creates the table, without the comments around it.
    let source = include_str!("../identity/migrations_marketplaces.rs");
    let start = source
        .find("CREATE TABLE haex_marketplaces")
        .expect("the table");
    let schema = source[start..].split("\"#;").next().unwrap().to_lowercase();
    for word in ["token", "password", "secret", "username"] {
        assert!(
            !schema.contains(word),
            "the table has a column that would hold a secret ({word})"
        );
    }
    assert!(text.contains(&item), "the id of the entry is there");
    for secret in [TOKEN, PASSWORD, "ada-the-user"] {
        assert!(!text.contains(secret), "{secret} is in the table:\n{text}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn neither_a_log_nor_an_error_shows_the_login_of_a_failed_call() {
    crate::test_log::install();
    let market = TestMarketplace::start().await;
    echo_everything_as_an_error(&market).await;
    let header = header_of_typed(&typed(None, TOKEN), MarketplaceAuthType::Bearer).unwrap();
    let asked = source(&market, Some(header));

    let result = test_marketplace(Http::default(), &asked).await;

    assert!(!result.ok);
    assert_eq!(result.error, Some(SourceErrorKind::Unreachable));
    let sent = market.requests().await;
    assert_eq!(sent.len(), 1, "the login call really went out");
    assert_eq!(
        sent[0]
            .headers
            .get("authorization")
            .unwrap()
            .to_str()
            .unwrap(),
        format!("Bearer {TOKEN}"),
        "and carried the header, so the echo had something to leak"
    );
    let shown = format!("{result:?}");
    assert!(
        !shown.contains(TOKEN) && !shown.contains("bad login"),
        "{shown}"
    );

    let page = fetch_catalog(
        Http::default(),
        &[asked],
        &query(),
        &[],
        &Version::new(0, 1, 0),
    )
    .await;
    let shown = format!("{page:?}");
    assert!(!shown.contains(TOKEN), "{shown}");

    assert_no_secret_logged(&[TOKEN, PASSWORD]);
}

/// The log of the process has no line with one of `secrets`, except the ones of the scripted
/// marketplace itself (`wiremock` records what it receives; a real server is not our code).
fn assert_no_secret_logged(secrets: &[&str]) {
    let logs = crate::test_log::all_but(&["wiremock", "hyper"]);
    for secret in secrets {
        assert!(!logs.contains(secret), "a log line shows {secret}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_good_call_with_a_login_logs_nothing_secret_either() {
    crate::test_log::install();
    let market = TestMarketplace::start().await;
    market.healthy().await;
    market
        .json(
            "/extensions",
            200,
            page(vec![entry("tetris", 1, Vec::new())], 1, 1, 1),
        )
        .await;
    let header =
        header_of_typed(&typed(Some("ada"), PASSWORD), MarketplaceAuthType::Basic).unwrap();

    let result = test_marketplace(Http::default(), &source(&market, Some(header))).await;

    assert!(result.ok, "{result:?}");
    assert_no_secret_logged(&[PASSWORD, "YWRhOnB"]);
}

#[test]
fn debug_of_every_type_that_touches_a_login_hides_it() {
    let new = typed(Some("ada"), TOKEN);
    let form = MarketplaceInput {
        id: None,
        name: "Private".to_owned(),
        base_url: "https://market.example".to_owned(),
        auth_type: MarketplaceAuthType::Bearer,
        credentials_item_id: None,
        new_credentials: Some(new.clone()),
    };
    let header = header_of_typed(&new, MarketplaceAuthType::Bearer).unwrap();
    let market = Source {
        id: "x".to_owned(),
        name: "X".to_owned(),
        base_url: "https://market.example".to_owned(),
        auth: Some(header.clone()),
        unavailable: None,
    };
    let shown = [
        format!("{new:?}"),
        format!("{form:?}"),
        format!("{header:?}"),
        format!("{market:?}"),
        format!("{:?}", credentials::Login::Ready(header)),
    ];
    for text in shown {
        assert!(!text.contains(TOKEN), "{text}");
        assert!(!text.contains("Bearer "), "{text}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_login_is_not_forwarded_when_a_redirect_leaves_the_origin() {
    let elsewhere = TestMarketplace::start().await;
    elsewhere.healthy().await;
    let market = TestMarketplace::start().await;
    market
        .redirect("/health", &format!("{}/health", elsewhere.origin()))
        .await;
    let header = header_of_typed(&typed(None, TOKEN), MarketplaceAuthType::Bearer).unwrap();

    let result = test_marketplace(Http::default(), &source(&market, Some(header))).await;

    assert!(!result.ok, "the redirect was not followed with a login");
    assert_eq!(
        elsewhere.requests().await.len(),
        0,
        "the other origin never saw the request, let alone the header"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_login_over_http_is_refused_before_a_request_goes_out() {
    let header = header_of_typed(&typed(None, TOKEN), MarketplaceAuthType::Bearer).unwrap();
    let asked = Source {
        id: "plain".to_owned(),
        name: "Plain".to_owned(),
        base_url: "http://marketplace.invalid".to_owned(),
        auth: Some(header),
        unavailable: None,
    };

    let result = test_marketplace(Http::default(), &asked).await;

    assert_eq!(result.error, Some(SourceErrorKind::InsecureCredentials));
    assert_eq!(result.detail.as_deref(), Some("login over http"));
}
