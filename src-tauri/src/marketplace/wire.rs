//! The marketplace protocol as the host reads it (contracts/marketplace-api.md, FR-035). Read
//! tolerantly: unknown fields are ignored, absent or `null` optional fields are `None`, and an
//! entry or a version that does not parse is left out instead of failing the whole answer. What a
//! marketplace must deliver (`extensionId`, `name`, `slug`, `shortDescription` per entry; the size
//! and checksum of a version to install it) is checked where it is used, not by `serde`.

use serde::de::{DeserializeOwned, Deserializer};
use serde::Deserialize;

/// Reads a list and drops the elements that do not parse (logged by count, never by content).
fn lenient<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    let values = Vec::<serde_json::Value>::deserialize(deserializer)?;
    let total = values.len();
    let kept: Vec<T> = values
        .into_iter()
        .filter_map(|value| serde_json::from_value(value).ok())
        .collect();
    if kept.len() < total {
        log::warn!(
            "a marketplace sent {} entries that do not fit the protocol; they are left out",
            total - kept.len()
        );
    }
    Ok(kept)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WirePublisher {
    pub display_name: String,
    pub slug: String,
    #[serde(default)]
    pub verified: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WireCategory {
    pub name: String,
    pub slug: String,
}

/// One version of an extension.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireVersion {
    /// Semver; one that is not stays listed but is not installable (checked by the catalog).
    pub version: String,
    #[serde(default)]
    pub changelog: Option<String>,
    #[serde(default)]
    pub bundle_size: Option<u64>,
    /// SHA-256 as 64 hex characters.
    #[serde(default)]
    pub bundle_hash: Option<String>,
    #[serde(default)]
    pub min_app_version: Option<String>,
    #[serde(default)]
    pub max_app_version: Option<String>,
}

/// An entry of the catalog page (`ListItem`).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireListItem {
    pub extension_id: String,
    pub name: String,
    pub slug: String,
    pub short_description: String,
    #[serde(default)]
    pub icon_url: Option<String>,
    #[serde(default)]
    pub verified: bool,
    #[serde(default)]
    pub total_downloads: u64,
    #[serde(default)]
    pub average_rating: Option<f64>,
    #[serde(default)]
    pub review_count: u32,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    #[serde(default)]
    pub publisher: Option<WirePublisher>,
    #[serde(default)]
    pub category: Option<WireCategory>,
    #[serde(default, deserialize_with = "lenient")]
    pub versions: Vec<WireVersion>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WirePagination {
    pub page: u32,
    pub limit: u32,
    pub total: u64,
    pub total_pages: u32,
}

/// `GET /extensions`.
#[derive(Debug, Clone, Deserialize)]
pub struct WireListResponse {
    #[serde(deserialize_with = "lenient")]
    pub extensions: Vec<WireListItem>,
    pub pagination: WirePagination,
}

/// `GET /extensions/{slug}`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireDetail {
    #[serde(flatten)]
    pub item: WireListItem,
    /// The publisher key (hex).
    #[serde(default)]
    pub public_key: Option<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: Option<String>,
}

/// One entry of `GET /categories`; the public marketplace sends the count as `extensionCount`.
#[derive(Debug, Clone, Deserialize)]
pub struct WireCategoryCount {
    pub name: String,
    pub slug: String,
    #[serde(alias = "extensionCount")]
    pub count: u32,
}

/// `GET /categories`.
#[derive(Debug, Clone, Deserialize)]
pub struct WireCategories {
    #[serde(deserialize_with = "lenient")]
    pub categories: Vec<WireCategoryCount>,
}

/// `GET /extensions/{slug}/download`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireDownload {
    pub download_url: String,
    pub version: String,
    #[serde(default)]
    pub bundle_size: Option<u64>,
    #[serde(default)]
    pub bundle_hash: Option<String>,
}

/// `GET /health`.
#[derive(Debug, Clone, Deserialize)]
pub struct WireHealth {
    pub status: String,
}

#[cfg(test)]
#[path = "wire_tests.rs"]
mod tests;
