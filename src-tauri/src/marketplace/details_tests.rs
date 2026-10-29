//! The categories and the details of an entry (T020).

use std::time::Duration;

use serde_json::json;

use super::*;
use crate::marketplace::test_support::{entry, version, TestMarketplace};

fn app() -> Version {
    Version::new(0, 1, 0)
}

fn source(id: &str, market: &TestMarketplace) -> Source {
    Source {
        id: id.to_owned(),
        name: format!("Market {id}"),
        base_url: market.base_url(),
        auth: None,
        unavailable: None,
    }
}

fn key(byte: char) -> String {
    byte.to_string().repeat(64)
}

#[tokio::test]
async fn categories_are_the_union_by_slug_with_the_counts_added_up() {
    let a = TestMarketplace::start().await;
    let b = TestMarketplace::start().await;
    let bad = TestMarketplace::start().await;
    a.json(
        "/categories",
        200,
        json!({ "categories": [
            { "id": "1", "name": "Notes", "slug": "notes", "extensionCount": 3 },
            { "id": "2", "name": "Media", "slug": "media", "extensionCount": 1 }
        ]}),
    )
    .await;
    b.json(
        "/categories",
        200,
        json!({ "categories": [ { "name": "Notizen", "slug": "notes", "count": 2 }, { "name": "Games", "slug": "games", "count": 4 } ]}),
    )
    .await;
    bad.json("/categories", 500, json!({})).await;
    let sources = [source("a", &a), source("b", &b), source("bad", &bad)];
    let result = fetch_categories(Http::default(), &sources, None).await;
    let rows: Vec<(&str, &str, u32)> = result
        .categories
        .iter()
        .map(|c| (c.slug.as_str(), c.name.as_str(), c.count))
        .collect();
    assert_eq!(
        rows,
        [
            ("notes", "Notes", 5),
            ("media", "Media", 1),
            ("games", "Games", 4)
        ]
    );
    assert_eq!(result.source_errors.len(), 1);
    assert_eq!(result.source_errors[0].source_id, "bad");

    let only_b = fetch_categories(Http::default(), &sources, Some("b")).await;
    assert_eq!(only_b.categories.len(), 2);
    assert_eq!(
        a.hits("/categories").await,
        1,
        "only the first call asked a"
    );
}

#[tokio::test]
async fn details_name_the_publisher_key_and_compare_it_with_the_installed_extension() {
    let a = TestMarketplace::start().await;
    let mut detail = entry("tetris", 1, vec![version("0.1.4", 1)]);
    detail["publicKey"] = json!(key('b'));
    detail["description"] = json!("<b>Classic</b> Tetris");
    detail["author"] = json!("haex");
    a.json("/extensions/tetris", 200, detail).await;

    let fingerprint = publisher_fingerprint(&key('b'));
    let same = [InstalledExtension {
        name: "tetris".into(),
        version: Some("0.1.3".into()),
        enabled: true,
        publisher_fingerprint: fingerprint.clone(),
    }];
    let other = [InstalledExtension {
        publisher_fingerprint: publisher_fingerprint(&key('c')),
        ..same[0].clone()
    }];
    let src = source("a", &a);

    let found = fetch_details(Http::default(), &src, "tetris", &same, &app())
        .await
        .expect("details");
    assert_eq!(
        found.publisher_fingerprint.as_deref(),
        Some(fingerprint.as_str())
    );
    assert_eq!(found.publisher_matches_installed, Some(true));
    assert_eq!(
        found.description, "<b>Classic</b> Tetris",
        "plain text, kept as it came"
    );
    assert_eq!(found.author.as_deref(), Some("haex"));
    assert_eq!(found.entry.update_to.as_deref(), Some("0.1.4"));

    let differs = fetch_details(Http::default(), &src, "tetris", &other, &app())
        .await
        .expect("details");
    assert_eq!(
        differs.publisher_matches_installed,
        Some(false),
        "same name, another publisher"
    );

    let none = fetch_details(Http::default(), &src, "tetris", &[], &app())
        .await
        .expect("details");
    assert_eq!(
        none.publisher_matches_installed, None,
        "nothing installed to compare"
    );
}

#[tokio::test]
async fn details_without_a_key_name_no_fingerprint() {
    let a = TestMarketplace::start().await;
    let mut detail = entry("tetris", 1, vec![]);
    detail["publicKey"] = json!("not a key");
    a.json("/extensions/tetris", 200, detail).await;
    let found = fetch_details(Http::default(), &source("a", &a), "tetris", &[], &app())
        .await
        .expect("details");
    assert_eq!(found.publisher_fingerprint, None);
    assert_eq!(found.description, "");
}

#[tokio::test]
async fn a_slug_that_is_not_a_slug_never_reaches_the_network() {
    let a = TestMarketplace::start().await;
    let failure = fetch_details(Http::default(), &source("a", &a), "../secret", &[], &app())
        .await
        .expect_err("refused");
    assert_eq!(failure.kind, SourceErrorKind::BadResponse);
    assert!(a.requests().await.is_empty());
}

#[tokio::test]
async fn a_missing_or_slow_entry_is_an_error_of_its_source() {
    let a = TestMarketplace::start().await;
    a.json("/extensions/gone", 404, json!({})).await;
    a.delayed("/extensions/slow", json!({}), Duration::from_secs(30))
        .await;
    let src = source("a", &a);
    let gone = fetch_details(Http::default(), &src, "gone", &[], &app())
        .await
        .expect_err("404");
    assert_eq!(gone.kind, SourceErrorKind::Unreachable);
    let slow = fetch_details(
        Http::with_timeout(Duration::from_millis(150)),
        &src,
        "slow",
        &[],
        &app(),
    )
    .await
    .expect_err("timeout");
    assert_eq!(slow.kind, SourceErrorKind::Timeout);
}
