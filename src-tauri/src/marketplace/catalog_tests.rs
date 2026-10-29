//! The common catalog against scripted marketplaces (T019): several sources at once, per-source
//! errors, no merging across sources, versions and compatibility, installed state and updates.

use std::time::{Duration, Instant};

use serde_json::json;

use super::*;
use crate::marketplace::test_support::{entry, page, version, TestMarketplace};

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

fn query(page: u32) -> CatalogQuery {
    CatalogQuery {
        page,
        search: None,
        category: None,
        source_id: None,
    }
}

async fn serve(market: &TestMarketplace, entries: Vec<serde_json::Value>, pages: u32, total: u64) {
    market
        .json("/extensions", 200, page(entries, 1, pages, total))
        .await;
}

fn slugs(page: &CatalogPage) -> Vec<(String, String)> {
    page.entries
        .iter()
        .map(|e| (e.source.id.clone(), e.slug.clone()))
        .collect()
}

#[tokio::test]
async fn every_source_is_asked_and_their_entries_follow_in_the_order_of_the_list() {
    let a = TestMarketplace::start().await;
    let b = TestMarketplace::start().await;
    serve(
        &a,
        vec![
            entry("tetris", 50, vec![version("0.1.0", 1)]),
            entry("doom", 10, vec![]),
        ],
        3,
        45,
    )
    .await;
    serve(&b, vec![entry("notes", 99, vec![])], 2, 21).await;
    let sources = [source("a", &a), source("b", &b)];

    let result = fetch_catalog(Http::default(), &sources, &query(1), &[], &app()).await;
    assert_eq!(
        slugs(&result),
        [
            ("a".into(), "tetris".into()),
            ("a".into(), "doom".into()),
            ("b".into(), "notes".into())
        ],
        "a before b although b has the more popular entry: the list order is the rank"
    );
    assert_eq!(result.total_pages, 3, "the largest of the sources");
    assert_eq!(result.total, 66, "the sum");
    assert_eq!(result.page, 1);
    assert!(result.source_errors.is_empty());
}

#[tokio::test]
async fn the_same_slug_in_two_sources_gives_two_entries_each_with_its_source() {
    let a = TestMarketplace::start().await;
    let b = TestMarketplace::start().await;
    serve(&a, vec![entry("notes", 5, vec![version("1.0.0", 1)])], 1, 1).await;
    serve(&b, vec![entry("notes", 5, vec![version("9.9.9", 2)])], 1, 1).await;
    let sources = [source("a", &a), source("b", &b)];
    let result = fetch_catalog(Http::default(), &sources, &query(1), &[], &app()).await;
    assert_eq!(result.entries.len(), 2, "nothing is merged (FR-012)");
    assert_eq!(result.entries[0].source.id, "a");
    assert_eq!(result.entries[1].source.id, "b");
    assert_eq!(
        result.entries[0].extension_id,
        result.entries[1].extension_id
    );
    assert_ne!(
        result.entries[0].latest_compatible,
        result.entries[1].latest_compatible
    );
}

#[tokio::test]
async fn a_failing_source_adds_an_error_line_and_the_others_still_answer() {
    let good = TestMarketplace::start().await;
    let bad = TestMarketplace::start().await;
    serve(&good, vec![entry("tetris", 1, vec![])], 1, 1).await;
    bad.json("/extensions", 500, json!({})).await;
    let sources = [source("bad", &bad), source("good", &good)];
    let result = fetch_catalog(Http::default(), &sources, &query(1), &[], &app()).await;
    assert_eq!(slugs(&result), [("good".into(), "tetris".into())]);
    assert_eq!(result.source_errors.len(), 1);
    let error = &result.source_errors[0];
    assert_eq!(
        (error.source_id.as_str(), error.kind),
        ("bad", SourceErrorKind::Unreachable)
    );
    assert_eq!(error.name, "Market bad");
    assert_eq!(error.detail.as_deref(), Some("HTTP 500"));
}

#[tokio::test]
async fn a_source_that_never_answers_holds_up_none_of_the_others() {
    let fast = TestMarketplace::start().await;
    let slow = TestMarketplace::start().await;
    serve(&fast, vec![entry("tetris", 1, vec![])], 1, 1).await;
    slow.delayed(
        "/extensions",
        page(vec![], 1, 0, 0),
        Duration::from_secs(30),
    )
    .await;
    let sources = [source("slow", &slow), source("fast", &fast)];
    let started = Instant::now();
    let result = fetch_catalog(
        Http::with_timeout(Duration::from_millis(300)),
        &sources,
        &query(1),
        &[],
        &app(),
    )
    .await;
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the limit of one source, not the sum"
    );
    assert_eq!(slugs(&result), [("fast".into(), "tetris".into())]);
    assert_eq!(result.source_errors[0].kind, SourceErrorKind::Timeout);
}

#[tokio::test]
async fn the_request_carries_page_limit_sort_and_the_filters() {
    let a = TestMarketplace::start().await;
    serve(&a, vec![], 0, 0).await;
    let sources = [source("a", &a)];
    let mut q = query(3);
    q.search = Some("  calendar ".into());
    q.category = Some("productivity".into());
    fetch_catalog(Http::default(), &sources, &q, &[], &app()).await;
    let request = a.requests().await.remove(0);
    let pairs: Vec<(String, String)> = request
        .url
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    for wanted in [
        ("page", "3"),
        ("limit", "20"),
        ("sort", "downloads"),
        ("search", "calendar"),
        ("category", "productivity"),
    ] {
        assert!(
            pairs.contains(&(wanted.0.into(), wanted.1.into())),
            "{wanted:?} in {pairs:?}"
        );
    }
    let mut plain = query(0);
    plain.search = Some("   ".into());
    let a2 = TestMarketplace::start().await;
    serve(&a2, vec![], 0, 0).await;
    fetch_catalog(Http::default(), &[source("a", &a2)], &plain, &[], &app()).await;
    let sent = a2.requests().await.remove(0);
    let keys: Vec<String> = sent
        .url
        .query_pairs()
        .map(|(k, _)| k.into_owned())
        .collect();
    assert!(
        !keys.contains(&"search".to_owned()),
        "a blank search is no search"
    );
    assert!(
        sent.url.query_pairs().any(|(k, v)| k == "page" && v == "1"),
        "page is at least 1"
    );
}

#[tokio::test]
async fn only_the_chosen_source_is_asked_with_a_source_filter() {
    let a = TestMarketplace::start().await;
    let b = TestMarketplace::start().await;
    serve(&a, vec![entry("one", 1, vec![])], 1, 1).await;
    serve(&b, vec![entry("two", 1, vec![])], 1, 1).await;
    let mut q = query(1);
    q.source_id = Some("b".into());
    let result = fetch_catalog(
        Http::default(),
        &[source("a", &a), source("b", &b)],
        &q,
        &[],
        &app(),
    )
    .await;
    assert_eq!(slugs(&result), [("b".into(), "two".into())]);
    assert_eq!(
        a.hits("/extensions").await,
        0,
        "the other source gets no request"
    );
}

#[tokio::test]
async fn an_entry_without_a_usable_slug_is_left_out() {
    let a = TestMarketplace::start().await;
    let mut broken = entry("x", 1, vec![]);
    broken.as_object_mut().expect("object").remove("slug");
    serve(
        &a,
        vec![
            broken,
            entry("../etc", 1, vec![]),
            entry("fine-1.0", 1, vec![]),
        ],
        1,
        3,
    )
    .await;
    let result = fetch_catalog(Http::default(), &[source("a", &a)], &query(1), &[], &app()).await;
    assert_eq!(slugs(&result), [("a".into(), "fine-1.0".into())]);
}

#[tokio::test]
async fn a_source_that_cannot_be_asked_is_reported_without_a_request() {
    let a = TestMarketplace::start().await;
    serve(&a, vec![entry("tetris", 1, vec![])], 1, 1).await;
    let mut closed = source("a", &a);
    closed.unavailable = Some(SourceErrorKind::CredentialsMissing);
    let result = fetch_catalog(Http::default(), &[closed], &query(1), &[], &app()).await;
    assert!(result.entries.is_empty());
    assert_eq!(
        result.source_errors[0].kind,
        SourceErrorKind::CredentialsMissing
    );
    assert!(a.requests().await.is_empty());
}

#[tokio::test]
async fn versions_are_judged_against_this_haexvault_and_the_newest_installable_wins() {
    let a = TestMarketplace::start().await;
    let mut too_old = version("2.0.0", 1);
    too_old["maxAppVersion"] = json!("0.0.9");
    let mut too_new = version("3.0.0", 2);
    too_new["minAppVersion"] = json!("1.0.0");
    let mut no_hash = version("4.0.0", 3);
    no_hash["bundleHash"] = json!(null);
    let fine_old = version("0.9.0", 4);
    let fine_new = version("1.5.0", 5);
    // The live service sends the newest first, but nothing says it must.
    let shuffled = vec![fine_old, fine_new, too_new, no_hash, too_old];
    serve(&a, vec![entry("tetris", 1, shuffled)], 1, 1).await;
    let result = fetch_catalog(Http::default(), &[source("a", &a)], &query(1), &[], &app()).await;
    let item = &result.entries[0];
    let order: Vec<&str> = item.versions.iter().map(|v| v.version.as_str()).collect();
    assert_eq!(
        order,
        ["4.0.0", "3.0.0", "2.0.0", "1.5.0", "0.9.0"],
        "newest first by semver"
    );
    let reasons: Vec<Option<&str>> = item
        .versions
        .iter()
        .map(|v| v.incompatible_reason.as_deref())
        .collect();
    assert_eq!(
        reasons,
        [
            Some("no_checksum"),
            Some("needs_newer"),
            Some("needs_older"),
            None,
            None
        ]
    );
    assert_eq!(item.versions[1].bound.as_deref(), Some("1.0.0"));
    assert_eq!(item.versions[2].bound.as_deref(), Some("0.0.9"));
    assert!(
        !item.versions[0].compatible
            && !item.versions[1].compatible
            && !item.versions[2].compatible
    );
    assert_eq!(item.latest_compatible.as_deref(), Some("1.5.0"));
}

#[tokio::test]
async fn an_entry_without_versions_has_nothing_to_install() {
    let a = TestMarketplace::start().await;
    serve(&a, vec![entry("empty", 1, vec![])], 1, 1).await;
    let result = fetch_catalog(Http::default(), &[source("a", &a)], &query(1), &[], &app()).await;
    assert!(result.entries[0].versions.is_empty());
    assert_eq!(result.entries[0].latest_compatible, None);
    assert_eq!(result.entries[0].update_to, None);
}

#[tokio::test]
async fn an_installed_extension_is_marked_and_an_update_names_the_newest_compatible_version() {
    let a = TestMarketplace::start().await;
    serve(
        &a,
        vec![
            entry("tetris", 1, vec![version("0.1.4", 1), version("0.1.3", 2)]),
            entry("doom", 1, vec![version("0.1.1", 3)]),
            entry("notes", 1, vec![version("0.1.0", 4)]),
        ],
        1,
        3,
    )
    .await;
    let installed = [
        InstalledExtension {
            name: "tetris".into(),
            version: Some("0.1.3".into()),
            enabled: true,
            publisher_fingerprint: "k".into(),
        },
        InstalledExtension {
            name: "doom".into(),
            version: Some("0.1.1".into()),
            enabled: false,
            publisher_fingerprint: "k".into(),
        },
        InstalledExtension {
            name: "notes".into(),
            version: None,
            enabled: true,
            publisher_fingerprint: "k".into(),
        },
    ];
    let result = fetch_catalog(
        Http::default(),
        &[source("a", &a)],
        &query(1),
        &installed,
        &app(),
    )
    .await;
    let by = |slug: &str| result.entries.iter().find(|e| e.slug == slug).expect(slug);
    assert_eq!(
        by("tetris").installed.as_ref().map(|i| i.version.as_str()),
        Some("0.1.3")
    );
    assert_eq!(by("tetris").update_to.as_deref(), Some("0.1.4"));
    assert_eq!(
        by("doom").installed.as_ref().map(|i| i.enabled),
        Some(false)
    );
    assert_eq!(by("doom").update_to, None, "up to date");
    assert_eq!(
        by("notes").installed,
        None,
        "no live bundle counts as not installed"
    );
}

#[test]
fn no_update_when_the_installed_version_is_newer_or_not_a_version() {
    let installed = CatalogInstalled {
        version: "2.0.0".into(),
        enabled: true,
    };
    assert_eq!(update_to(Some(&installed), Some("1.0.0")), None);
    assert_eq!(update_to(Some(&installed), Some("2.0.0")), None);
    assert_eq!(
        update_to(Some(&installed), Some("2.0.1")).as_deref(),
        Some("2.0.1")
    );
    let odd = CatalogInstalled {
        version: "dev".into(),
        enabled: true,
    };
    assert_eq!(update_to(Some(&odd), Some("9.9.9")), None);
    assert_eq!(update_to(None, Some("1.0.0")), None);
    assert_eq!(update_to(Some(&installed), None), None);
}

#[test]
fn slugs_are_checked_before_they_go_into_a_path() {
    for good in ["haex-tetris", "a", "x_y.z-1"] {
        assert!(valid_slug(good), "{good}");
    }
    for bad in ["", "../x", "a/b", "a b", "a?b", "ä", &"x".repeat(101)] {
        assert!(!valid_slug(bad), "{bad}");
    }
}

#[test]
fn a_bound_that_is_not_a_version_does_not_apply() {
    let mut v = crate::marketplace::wire::WireVersion {
        version: "1.0.0".into(),
        changelog: None,
        bundle_size: Some(1),
        bundle_hash: Some("ab".repeat(32)),
        min_app_version: Some("soon".into()),
        max_app_version: Some("".into()),
    };
    assert_eq!(installability(&v, &app()), Installability::Installable);
    v.bundle_hash = Some("xyz".into());
    assert_eq!(installability(&v, &app()), Installability::NoChecksum);
    v.bundle_hash = Some("AB".repeat(32));
    v.bundle_size = None;
    assert_eq!(
        installability(&v, &app()),
        Installability::NoChecksum,
        "a size is needed too"
    );
}
