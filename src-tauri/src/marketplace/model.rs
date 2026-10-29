//! The types the marketplace commands send and receive (contracts/commands.md, data-model.md).
//! The protocol itself is read through the tolerant structs of [`super::wire`]; these are what the
//! frontend sees, and none of them holds a secret.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// How a marketplace asks for a login (FR-026).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum MarketplaceAuthType {
    None,
    Bearer,
    Basic,
}

impl MarketplaceAuthType {
    /// The value stored in `haex_marketplaces.auth_type`.
    pub fn as_str(self) -> &'static str {
        match self {
            MarketplaceAuthType::None => "none",
            MarketplaceAuthType::Bearer => "bearer",
            MarketplaceAuthType::Basic => "basic",
        }
    }

    /// The type of a stored value; an unknown one reads as `None` so a row of a newer version
    /// never makes the list fail.
    pub fn from_stored(value: &str) -> Self {
        match value {
            "bearer" => MarketplaceAuthType::Bearer,
            "basic" => MarketplaceAuthType::Basic,
            _ => MarketplaceAuthType::None,
        }
    }

    /// Whether this type needs an entry of the password manager.
    pub fn needs_credentials(self) -> bool {
        self != MarketplaceAuthType::None
    }
}

/// Whether the credentials entry of a marketplace is on this device (research R11).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum MarketplaceCredentialsState {
    /// The marketplace needs no login.
    None,
    Present,
    /// The entry was deleted or lacks the values of the auth type.
    Missing,
    /// The entry has not arrived through the sync yet.
    Syncing,
}

/// What the form of a marketplace sends (`marketplace_save`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct MarketplaceInput {
    /// Absent for a new marketplace.
    #[ts(optional)]
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    pub base_url: String,
    pub auth_type: MarketplaceAuthType,
    #[ts(optional)]
    #[serde(default)]
    pub credentials_item_id: Option<String>,
}

/// One marketplace of the list; all of them are equal (FR-023).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct MarketplaceView {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub enabled: bool,
    /// 1-based position in the list.
    pub rank: u32,
    pub auth_type: MarketplaceAuthType,
    pub credentials: MarketplaceCredentialsState,
}

/// Why one source gave no answer (FR-031). Never carries text of the marketplace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum SourceErrorKind {
    Unreachable,
    Timeout,
    AuthRejected,
    CredentialsMissing,
    CredentialsSyncing,
    BadResponse,
    InsecureCredentials,
}

/// The error line of one source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct SourceError {
    pub source_id: String,
    pub name: String,
    pub kind: SourceErrorKind,
    /// Locally generated and short (`HTTP 500`, the kind of a connection error), never text of
    /// the marketplace, an address or a header (contract "Errors").
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// What `marketplace_catalog` asks for.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct CatalogQuery {
    /// 1-based.
    pub page: u32,
    #[ts(optional)]
    #[serde(default)]
    pub search: Option<String>,
    #[ts(optional)]
    #[serde(default)]
    pub category: Option<String>,
    /// Only this marketplace; absent: all enabled ones.
    #[ts(optional)]
    #[serde(default)]
    pub source_id: Option<String>,
}

/// The marketplace an entry comes from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct CatalogSource {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct CatalogPublisher {
    pub display_name: String,
    pub slug: String,
    /// The marketplace's own statement; only shown, never a reason to trust (FR-005).
    pub verified: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct CatalogCategory {
    pub name: String,
    pub slug: String,
}

/// One version of an entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct CatalogVersion {
    pub version: String,
    #[ts(type = "number | null")]
    pub bundle_size: Option<u64>,
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changelog: Option<String>,
    /// HaexVault of this version is within `minAppVersion` and `maxAppVersion` (FR-018).
    pub compatible: bool,
    /// `needs_newer` or `needs_older` when not compatible, or `no_checksum` when the marketplace
    /// gave no size or checksum (nothing is installed then, FR-013).
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub incompatible_reason: Option<String>,
    /// The bound the reason names (`minAppVersion` or `maxAppVersion`).
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bound: Option<String>,
}

/// What is installed of an entry (FR-004).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct CatalogInstalled {
    pub version: String,
    pub enabled: bool,
}

/// One extension of the catalog, identified by `(source.id, slug)` and never merged with an entry
/// of another source (FR-012).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct CatalogEntry {
    pub source: CatalogSource,
    pub extension_id: String,
    pub name: String,
    pub slug: String,
    pub short_description: String,
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
    pub verified: bool,
    #[ts(type = "number")]
    pub total_downloads: u64,
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub average_rating: Option<f64>,
    pub review_count: u32,
    pub tags: Vec<String>,
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publisher: Option<CatalogPublisher>,
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<CatalogCategory>,
    pub versions: Vec<CatalogVersion>,
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub installed: Option<CatalogInstalled>,
    /// The newest version this HaexVault can install and that has a checksum.
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_compatible: Option<String>,
    /// `latest_compatible` when it is newer than the installed version.
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_to: Option<String>,
}

/// One page of the common catalog (FR-001, FR-012).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct CatalogPage {
    pub entries: Vec<CatalogEntry>,
    pub page: u32,
    pub total_pages: u32,
    #[ts(type = "number")]
    pub total: u64,
    pub source_errors: Vec<SourceError>,
}

/// A category of the catalog with the number of its entries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct CatalogCategoryCount {
    pub name: String,
    pub slug: String,
    pub count: u32,
}

/// The categories of all asked sources (`marketplace_categories`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct CatalogCategories {
    pub categories: Vec<CatalogCategoryCount>,
    pub source_errors: Vec<SourceError>,
}

/// The details of one entry (`marketplace_details`).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct CatalogDetail {
    pub entry: CatalogEntry,
    /// Plain text; never rendered as markup (FR-006).
    pub description: String,
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// Short form of the publisher key the details name (the first and last four groups).
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publisher_fingerprint: Option<String>,
    /// An installed extension of this name has the same publisher key; absent when none is
    /// installed or the marketplace names no key.
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publisher_matches_installed: Option<bool>,
}

/// The result of the form's test (FR-021).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct MarketplaceTestResult {
    pub ok: bool,
    /// Entries the first page reported as a total.
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(type = "number | undefined")]
    pub count: Option<u64>,
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<SourceErrorKind>,
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// A newer version known for an installed extension (FR-017).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct KnownUpdate {
    /// The installed extension's name.
    pub installed_name: String,
    pub installed: String,
    pub latest_compatible: String,
    pub source_id: String,
    pub slug: String,
}

/// The progress of a download (FR-015).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct DownloadProgress {
    #[ts(type = "number")]
    pub loaded: u64,
    #[ts(type = "number")]
    pub total: u64,
}

/// A downloaded bundle waiting in the host for its install preview.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct PendingBundleRef {
    pub id: String,
}
