use std::time::{Duration, Instant};

use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::marketplace::model::{CatalogInstalled, CatalogSource, SourceErrorKind};
use crate::marketplace::test_support::{entry, page, version, TestMarketplace};

fn known(name: &str, installed: &str, latest: &str, source: &str, slug: &str) -> KnownUpdate {
    KnownUpdate {
        installed_name: name.to_owned(),
        installed: installed.to_owned(),
        latest_compatible: latest.to_owned(),
        source_id: source.to_owned(),
        slug: slug.to_owned(),
    }
}

fn catalog_entry(
    source: &str,
    name: &str,
    slug: &str,
    installed: Option<&str>,
    update_to: Option<&str>,
) -> CatalogEntry {
    CatalogEntry {
        source: CatalogSource {
            id: source.to_owned(),
            name: source.to_owned(),
        },
        extension_id: format!("e/{slug}"),
        name: name.to_owned(),
        slug: slug.to_owned(),
        short_description: String::new(),
        icon_url: None,
        verified: false,
        total_downloads: 0,
        average_rating: None,
        review_count: 0,
        tags: Vec::new(),
        publisher: None,
        category: None,
        versions: Vec::new(),
        installed: installed.map(|version| CatalogInstalled {
            version: version.to_owned(),
            enabled: true,
        }),
        latest_compatible: update_to.map(str::to_owned),
        update_to: update_to.map(str::to_owned),
    }
}

fn extension(name: &str, version: Option<&str>) -> InstalledExtension {
    InstalledExtension {
        name: name.to_owned(),
        version: version.map(str::to_owned),
        enabled: true,
        publisher_fingerprint: "ab".repeat(16),
    }
}

fn ids(updates: &[KnownUpdate]) -> Vec<(&str, &str, &str)> {
    updates
        .iter()
        .map(|u| {
            (
                u.installed_name.as_str(),
                u.source_id.as_str(),
                u.slug.as_str(),
            )
        })
        .collect()
}

#[test]
fn a_catalog_page_sets_the_update_of_its_entry_and_clears_it_when_none_is_left() {
    let known_updates = KnownUpdates::default();
    known_updates.record_page(&[catalog_entry(
        "m1",
        "demo",
        "demo",
        Some("1.0.0"),
        Some("1.1.0"),
    )]);
    assert_eq!(
        known_updates.candidates(),
        [known("demo", "1.0.0", "1.1.0", "m1", "demo")]
    );

    known_updates.record_page(&[catalog_entry("m1", "demo", "demo", Some("1.1.0"), None)]);
    assert!(known_updates.candidates().is_empty());
}

#[test]
fn a_page_touches_only_the_entries_it_contains() {
    let known_updates = KnownUpdates::default();
    known_updates.record_page(&[
        catalog_entry("m1", "a", "a", Some("1.0.0"), Some("2.0.0")),
        catalog_entry("m1", "b", "b", Some("1.0.0"), Some("1.5.0")),
        catalog_entry("m2", "a", "a", Some("1.0.0"), Some("1.2.0")),
    ]);

    // A filtered page: only `b` of m1, and an entry that is not installed.
    known_updates.record_page(&[
        catalog_entry("m1", "b", "b", Some("1.5.0"), None),
        catalog_entry("m1", "other", "other", None, None),
    ]);

    assert_eq!(
        ids(&known_updates.candidates()),
        [("a", "m1", "a"), ("a", "m2", "a")]
    );
}

#[test]
fn an_answer_replaces_only_the_candidates_of_its_marketplace_and_name() {
    let known_updates = KnownUpdates::default();
    known_updates.replace_for(
        "a",
        "m1",
        vec![
            known("a", "1.0.0", "1.1.0", "m1", "a"),
            known("a", "1.0.0", "1.2.0", "m1", "a-fork"),
        ],
    );
    known_updates.replace_for("a", "m2", vec![known("a", "1.0.0", "1.3.0", "m2", "a")]);
    known_updates.replace_for("b", "m1", vec![known("b", "2.0.0", "2.1.0", "m1", "b")]);

    // m1 now answers for `a` with one entry only: the fork is gone there.
    known_updates.replace_for("a", "m1", vec![known("a", "1.0.0", "1.1.0", "m1", "a")]);
    assert_eq!(
        ids(&known_updates.candidates()),
        [("a", "m1", "a"), ("a", "m2", "a"), ("b", "m1", "b")]
    );

    // m2 answers that `a` is up to date there.
    known_updates.replace_for("a", "m2", Vec::new());
    assert_eq!(
        ids(&known_updates.candidates()),
        [("a", "m1", "a"), ("b", "m1", "b")]
    );
}

fn enabled(ids: &[&str]) -> Vec<String> {
    ids.iter().map(|id| (*id).to_owned()).collect()
}

#[test]
fn the_view_selects_the_highest_version_and_on_a_tie_the_better_ranked_marketplace() {
    let candidates = vec![
        known("a", "1.0.0", "1.2.0", "m2", "a"),
        known("a", "1.0.0", "1.2.0", "m1", "a"),
        known("a", "1.0.0", "1.10.0", "m3", "a"),
        known("b", "1.0.0", "1.1.0", "m2", "b"),
        known("b", "1.0.0", "1.1.0", "m1", "b-other"),
    ];
    let installed = [extension("a", Some("1.0.0")), extension("b", Some("1.0.0"))];

    let shown = view(candidates, &installed, &enabled(&["m1", "m2", "m3"]));

    assert_eq!(shown.candidates.len(), 5);
    assert_eq!(
        ids(&shown.selected),
        [("a", "m3", "a"), ("b", "m1", "b-other")],
        "1.10.0 beats 1.2.0 as a version, not as text; on a tie rank 1 wins"
    );
}

#[test]
fn the_view_forgets_what_no_longer_applies_and_shows_the_installed_version_of_now() {
    let candidates = vec![
        known("gone", "1.0.0", "2.0.0", "m1", "gone"),
        known("off", "1.0.0", "2.0.0", "m9", "off"),
        known("done", "1.0.0", "1.1.0", "m1", "done"),
        known("later", "1.0.0", "1.5.0", "m1", "later"),
        known("unlive", "1.0.0", "1.5.0", "m1", "unlive"),
    ];
    let installed = [
        extension("off", Some("1.0.0")),
        extension("done", Some("1.1.0")),
        extension("later", Some("1.2.0")),
        extension("unlive", None),
    ];

    let shown = view(candidates, &installed, &enabled(&["m1"]));

    assert_eq!(ids(&shown.candidates), [("later", "m1", "later")]);
    assert_eq!(shown.candidates[0].installed, "1.2.0");
}

// ---- the check ----

fn installed_probe() -> Vec<InstalledExtension> {
    vec![extension("probe", Some("1.0.0"))]
}

fn source_of(market: &TestMarketplace, id: &str) -> Source {
    Source {
        id: id.to_owned(),
        name: format!("Market {id}"),
        base_url: market.base_url(),
        auth: None,
        unavailable: None,
    }
}

async fn serve_search(market: &TestMarketplace, name: &str, entries: Vec<serde_json::Value>) {
    Mock::given(method("GET"))
        .and(path("/extensions"))
        .and(query_param("search", name))
        .respond_with(ResponseTemplate::new(200).set_body_json(page(entries, 1, 1, 1)))
        .mount(&market.server)
        .await;
}

fn app() -> Version {
    Version::new(1, 0, 0)
}

#[tokio::test]
async fn an_installed_extension_gets_its_update_whatever_the_catalog_shows() {
    let market = TestMarketplace::start().await;
    serve_search(
        &market,
        "probe",
        vec![entry(
            "probe",
            5,
            vec![version("1.1.0", 1), version("1.0.0", 2)],
        )],
    )
    .await;

    let checked = check_updates(
        Http::default(),
        &[source_of(&market, "m1")],
        &installed_probe(),
        &app(),
    )
    .await;

    assert!(checked.source_errors.is_empty());
    assert_eq!(
        checked.answered,
        [(
            "probe".to_owned(),
            "m1".to_owned(),
            vec![known("probe", "1.0.0", "1.1.0", "m1", "probe")]
        )]
    );
}

#[tokio::test]
async fn a_name_that_only_partly_matches_is_ignored() {
    let market = TestMarketplace::start().await;
    serve_search(
        &market,
        "probe",
        vec![
            entry("probe-pro", 9, vec![version("9.0.0", 1)]),
            entry("probe", 5, vec![version("1.0.0", 2)]),
        ],
    )
    .await;

    let checked = check_updates(
        Http::default(),
        &[source_of(&market, "m1")],
        &installed_probe(),
        &app(),
    )
    .await;

    assert_eq!(
        checked.answered,
        [("probe".to_owned(), "m1".to_owned(), Vec::new())],
        "the other entry is no update of `probe`, and `probe` itself is up to date"
    );
}

#[tokio::test]
async fn a_failing_marketplace_is_one_error_line_and_answers_for_no_name() {
    let good = TestMarketplace::start().await;
    serve_search(
        &good,
        "a",
        vec![entry(
            "a",
            1,
            vec![version("1.1.0", 1), version("1.0.0", 2)],
        )],
    )
    .await;
    serve_search(&good, "b", vec![entry("b", 1, vec![version("1.0.0", 1)])]).await;
    let bad = TestMarketplace::start().await;
    bad.raw("/extensions", 500, "no").await;
    let installed = vec![extension("a", Some("1.0.0")), extension("b", Some("1.0.0"))];

    let checked = check_updates(
        Http::default(),
        &[source_of(&bad, "bad"), source_of(&good, "good")],
        &installed,
        &app(),
    )
    .await;

    assert_eq!(checked.source_errors.len(), 1, "one line, not one per name");
    assert_eq!(checked.source_errors[0].source_id, "bad");
    assert!(checked
        .answered
        .iter()
        .all(|(_, source, _)| source == "good"));
    assert_eq!(checked.answered.len(), 2);

    // Applied the way the command does: the failed marketplace's earlier candidate survives.
    let state = KnownUpdates::default();
    state.replace_for("a", "bad", vec![known("a", "1.0.0", "1.4.0", "bad", "a")]);
    for (name, source, updates) in &checked.answered {
        state.replace_for(name, source, updates.clone());
    }
    assert_eq!(
        ids(&state.candidates()),
        [("a", "bad", "a"), ("a", "good", "a")]
    );
}

#[tokio::test]
async fn a_source_that_cannot_be_asked_is_reported_without_a_request() {
    let market = TestMarketplace::start().await;
    let mut source = source_of(&market, "m1");
    source.unavailable = Some(SourceErrorKind::CredentialsMissing);

    let checked = check_updates(Http::default(), &[source], &installed_probe(), &app()).await;

    assert_eq!(checked.source_errors.len(), 1);
    assert_eq!(
        checked.source_errors[0].kind,
        SourceErrorKind::CredentialsMissing
    );
    assert!(market.requests().await.is_empty());
}

#[tokio::test]
async fn nothing_is_asked_without_an_installed_extension_or_a_marketplace() {
    let market = TestMarketplace::start().await;

    let none_installed = check_updates(
        Http::default(),
        &[source_of(&market, "m1")],
        &[extension("unlive", None)],
        &app(),
    )
    .await;
    let no_source = check_updates(Http::default(), &[], &installed_probe(), &app()).await;

    assert!(none_installed.answered.is_empty() && none_installed.source_errors.is_empty());
    assert!(no_source.answered.is_empty() && no_source.source_errors.is_empty());
    assert!(market.requests().await.is_empty());
}

#[tokio::test]
async fn at_most_four_searches_run_at_the_same_time() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/extensions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(page(Vec::new(), 1, 1, 0))
                .set_delay(Duration::from_millis(300)),
        )
        .mount(&server)
        .await;
    let source = Source {
        id: "m1".into(),
        name: "m1".into(),
        base_url: server.uri(),
        auth: None,
        unavailable: None,
    };
    let installed: Vec<_> = (0..8)
        .map(|n| extension(&format!("ext-{n}"), Some("1.0.0")))
        .collect();

    let started = Instant::now();
    let checked = check_updates(Http::default(), &[source], &installed, &app()).await;
    let took = started.elapsed();

    assert_eq!(checked.answered.len(), 8);
    assert!(
        took >= Duration::from_millis(580),
        "eight 300 ms searches, four at a time, take two rounds, took {took:?}"
    );
}

#[test]
fn the_result_lists_what_this_run_found() {
    let checked = Checked {
        answered: vec![
            (
                "a".into(),
                "m1".into(),
                vec![known("a", "1.0.0", "1.1.0", "m1", "a")],
            ),
            ("b".into(), "m1".into(), Vec::new()),
        ],
        source_errors: Vec::new(),
    };

    assert_eq!(
        checked.result().updates,
        [known("a", "1.0.0", "1.1.0", "m1", "a")]
    );
}

#[test]
fn nothing_in_this_module_fetches_by_itself() {
    // `KnownUpdates` has no way to reach a marketplace: it only stores what it is told.
    let known_updates = KnownUpdates::default();
    assert!(known_updates.candidates().is_empty());
}
