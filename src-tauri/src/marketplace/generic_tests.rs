//! The marketplace is generic (spec 069 SC-009, FR-034): any number of marketplaces answer the
//! same protocol, whatever their address and path prefix, and the address of the project's own one
//! stands in exactly one place of the code.

use std::fs;
use std::path::{Path, PathBuf};

use semver::Version;

use crate::marketplace::catalog::{fetch_catalog, Source};
use crate::marketplace::client::Http;
use crate::marketplace::model::{CatalogQuery, SourceErrorKind};
use crate::marketplace::test_support::{entry, page, version, TestMarketplace};

fn source(id: &str, market: &TestMarketplace) -> Source {
    Source {
        id: id.to_owned(),
        name: format!("Market {id}"),
        base_url: market.base_url(),
        auth: None,
        unavailable: None,
    }
}

#[tokio::test]
async fn marketplaces_at_any_address_serve_one_catalog_together() {
    // One under a path prefix, one at the root of another port, one that wants a login: nothing
    // below knows any of their addresses, only the rows of the list do.
    let team = TestMarketplace::with_prefix("/team/market").await;
    team.json(
        "/extensions",
        200,
        page(
            vec![entry("from-team", 5, vec![version("1.0.0", 1)])],
            1,
            1,
            1,
        ),
    )
    .await;
    let plain = TestMarketplace::start().await;
    plain
        .json(
            "/extensions",
            200,
            page(
                vec![entry("from-plain", 9, vec![version("2.0.0", 2)])],
                1,
                1,
                1,
            ),
        )
        .await;
    let closed = TestMarketplace::start().await;
    closed.raw("/extensions", 401, "login required").await;

    let answer = fetch_catalog(
        Http::default(),
        &[
            source("team", &team),
            source("closed", &closed),
            source("plain", &plain),
        ],
        &CatalogQuery {
            page: 1,
            search: None,
            category: None,
            source_id: None,
        },
        &[],
        &Version::new(1, 0, 0),
    )
    .await;

    let entries: Vec<(&str, &str)> = answer
        .entries
        .iter()
        .map(|e| (e.source.id.as_str(), e.slug.as_str()))
        .collect();
    assert_eq!(entries, [("team", "from-team"), ("plain", "from-plain")]);
    assert_eq!(answer.source_errors.len(), 1);
    assert_eq!(answer.source_errors[0].source_id, "closed");
    assert_eq!(answer.source_errors[0].kind, SourceErrorKind::AuthRejected);
    assert_eq!(team.hits("/extensions").await, 1);
}

fn files_under(root: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if path.is_dir() {
            if !matches!(
                name.as_str(),
                "node_modules" | "target" | ".nuxt" | ".output"
            ) {
                files_under(&path, found);
            }
        } else if matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("rs" | "ts" | "vue" | "json")
        ) {
            found.push(path);
        }
    }
}

#[test]
fn the_address_of_the_public_marketplace_stands_in_one_place_of_the_code() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let needle = ["marketplace", "haex", "space"].join(".");
    let mut files = Vec::new();
    files_under(&manifest.join("src"), &mut files);
    files_under(&manifest.join("../src"), &mut files);

    let allowed = |path: &Path| {
        let text = path.to_string_lossy().replace('\\', "/");
        text.ends_with("marketplace/store.rs")
            || text.ends_with("_tests.rs")
            || text.ends_with("test_support.rs")
            || text.contains("/src/i18n/locales/")
    };
    let hits: Vec<String> = files
        .iter()
        .filter(|path| !allowed(path))
        .filter(|path| fs::read_to_string(path).is_ok_and(|text| text.contains(needle.as_str())))
        .map(|path| path.display().to_string())
        .collect();

    assert!(
        hits.is_empty(),
        "FR-034: the public marketplace is only the preset row of marketplace/store.rs, found in {hits:?}"
    );
}
