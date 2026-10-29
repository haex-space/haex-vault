use serde_json::json;

use super::*;

fn item(slug: &str) -> serde_json::Value {
    json!({
        "id": "b43eb35c-0000-4000-8000-000000000001",
        "extensionId": format!("haex/{slug}"),
        "name": slug,
        "slug": slug,
        "shortDescription": "A game",
        "iconUrl": "https://img.example/icon.png",
        "verified": false,
        "totalDownloads": 7,
        "averageRating": null,
        "reviewCount": 0,
        "tags": [],
        "publishedAt": null,
        "publisher": { "displayName": "haex", "slug": "haex", "verified": true, "email": null },
        "category": null,
        "versions": [
            { "id": "v1", "version": "0.1.1", "changelog": null, "bundleSize": 1234,
              "bundleHash": "ab".repeat(32), "permissions": null,
              "minAppVersion": null, "maxAppVersion": null, "downloads": 0,
              "publishedAt": "2026-10-27T09:19:04.962Z" }
        ],
        "somethingNew": { "a": 1 }
    })
}

fn page(extensions: Vec<serde_json::Value>) -> serde_json::Value {
    json!({
        "extensions": extensions,
        "pagination": { "page": 1, "limit": 20, "total": 2, "totalPages": 1 }
    })
}

#[test]
fn a_page_in_the_shape_of_the_live_service_parses_and_unknown_fields_are_ignored() {
    let parsed: WireListResponse =
        serde_json::from_value(page(vec![item("haex-tetris"), item("haex-doom")])).expect("parse");
    assert_eq!(parsed.extensions.len(), 2);
    let first = &parsed.extensions[0];
    assert_eq!(first.slug, "haex-tetris");
    assert_eq!(first.average_rating, None, "null is none");
    assert_eq!(first.versions[0].bundle_size, Some(1234));
    assert_eq!(
        first.versions[0].bundle_hash.as_deref(),
        Some("ab".repeat(32).as_str())
    );
    assert_eq!(first.publisher.as_ref().map(|p| p.verified), Some(true));
    assert_eq!(parsed.pagination.total_pages, 1);
}

#[test]
fn an_entry_without_a_required_field_is_left_out_and_the_rest_stays() {
    let mut broken = item("no-slug");
    broken.as_object_mut().expect("object").remove("slug");
    let parsed: WireListResponse =
        serde_json::from_value(page(vec![item("haex-tetris"), broken, item("haex-doom")]))
            .expect("parse");
    let slugs: Vec<_> = parsed.extensions.iter().map(|e| e.slug.as_str()).collect();
    assert_eq!(slugs, ["haex-tetris", "haex-doom"]);
}

#[test]
fn a_version_without_checksum_or_size_stays_listed() {
    let mut entry = item("haex-tetris");
    entry["versions"] = json!([{ "version": "0.1.1" }, { "version": "0.1.0", "bundleSize": 5 }]);
    let parsed: WireListResponse = serde_json::from_value(page(vec![entry])).expect("parse");
    let versions = &parsed.extensions[0].versions;
    assert_eq!(versions.len(), 2);
    assert!(versions[0].bundle_hash.is_none() && versions[0].bundle_size.is_none());
    assert_eq!(versions[1].bundle_size, Some(5));
}

#[test]
fn a_version_without_a_version_string_is_left_out() {
    let mut entry = item("haex-tetris");
    entry["versions"] = json!([{ "bundleSize": 5 }, { "version": "0.1.0" }]);
    let parsed: WireListResponse = serde_json::from_value(page(vec![entry])).expect("parse");
    assert_eq!(parsed.extensions[0].versions.len(), 1);
}

#[test]
fn a_page_without_pagination_is_not_a_page() {
    let body = json!({ "extensions": [item("a")] });
    assert!(serde_json::from_value::<WireListResponse>(body).is_err());
}

#[test]
fn the_category_count_comes_as_extension_count_or_count() {
    let parsed: WireCategories = serde_json::from_value(json!({
        "categories": [
            { "id": "1", "name": "Notes", "slug": "notes", "extensionCount": 3, "sortOrder": 5 },
            { "name": "Media", "slug": "media", "count": 2 },
            { "name": "Broken" }
        ]
    }))
    .expect("parse");
    let counts: Vec<_> = parsed
        .categories
        .iter()
        .map(|c| (c.slug.as_str(), c.count))
        .collect();
    assert_eq!(counts, [("notes", 3), ("media", 2)]);
}

#[test]
fn details_carry_the_key_and_the_description_next_to_the_list_fields() {
    let mut entry = item("haex-tetris");
    entry["publicKey"] = json!("b4401f13".repeat(8));
    entry["description"] = json!("Classic Tetris");
    entry["latestVersion"] = json!({ "version": "0.1.1" });
    let detail: WireDetail = serde_json::from_value(entry).expect("parse");
    assert_eq!(detail.item.slug, "haex-tetris");
    assert_eq!(
        detail.public_key.as_deref(),
        Some("b4401f13".repeat(8).as_str())
    );
    assert_eq!(detail.description, "Classic Tetris");
}

#[test]
fn a_download_answer_without_checksum_parses_and_is_refused_later() {
    let parsed: WireDownload = serde_json::from_value(json!({
        "downloadUrl": "https://files.example/x.xt",
        "version": "0.1.1"
    }))
    .expect("parse");
    assert!(parsed.bundle_hash.is_none() && parsed.bundle_size.is_none());
}
