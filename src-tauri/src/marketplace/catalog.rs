//! The common catalog (spec 069, FR-001 to FR-004, FR-012, FR-018, research R7, R8, R9): every
//! enabled marketplace is asked at the same time, each page is the requested page of every source
//! in the order of the list, and entries of different sources are never merged: a source must not
//! be able to hide the entry of another. What is installed and what could be updated is worked out
//! here, once, from the versions of the answer and the installed extensions.

use futures::future::join_all;
use semver::Version;

use super::client::{AuthHeader, Http, SourceFailure};
use super::model::{
    CatalogCategory, CatalogEntry, CatalogInstalled, CatalogPage, CatalogPublisher, CatalogQuery,
    CatalogSource, CatalogVersion, SourceError, SourceErrorKind,
};
use super::wire::{WireListItem, WireListResponse, WireVersion};
use super::PAGE_LIMIT;

/// One marketplace as the catalog asks it: its row and the login, if it needs one.
#[derive(Debug, Clone)]
pub struct Source {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub auth: Option<AuthHeader>,
    /// Set when the source cannot be asked at all (its credentials entry is missing or has not
    /// arrived): it is reported as an error line and no request goes out (FR-028).
    pub unavailable: Option<SourceErrorKind>,
}

impl Source {
    pub fn catalog_source(&self) -> CatalogSource {
        CatalogSource {
            id: self.id.clone(),
            name: self.name.clone(),
        }
    }

    /// The error line of this source for `failure`.
    pub fn error(&self, failure: SourceFailure) -> SourceError {
        SourceError {
            source_id: self.id.clone(),
            name: self.name.clone(),
            kind: failure.kind,
            detail: failure.detail,
        }
    }
}

/// An installed extension, as far as the catalog needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledExtension {
    pub name: String,
    /// `None` while no bundle of it is live (then it counts as not installed).
    pub version: Option<String>,
    pub enabled: bool,
    pub publisher_fingerprint: String,
}

/// Whether a version of an entry can be installed by this HaexVault.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Installability {
    Installable,
    /// `minAppVersion` is newer than this HaexVault.
    NeedsNewer(String),
    /// `maxAppVersion` is older than this HaexVault.
    NeedsOlder(String),
    /// The marketplace gave no usable size and checksum (FR-013).
    NoChecksum,
}

/// A slug the host puts into a path: letters, digits, `-`, `_` and `.`, at most 100 characters.
pub fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug != "."
        && slug != ".."
        && slug.len() <= 100
        && slug
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

fn parse(text: &str) -> Option<Version> {
    Version::parse(text.trim()).ok()
}

fn has_checksum(version: &WireVersion) -> bool {
    version.bundle_size.is_some()
        && version
            .bundle_hash
            .as_deref()
            .is_some_and(|hash| hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// Judges one version against `app` (FR-018, FR-013). A bound that is not a version is ignored, a
/// missing one does not apply.
pub fn installability(version: &WireVersion, app: &Version) -> Installability {
    if let Some(min) = version.min_app_version.as_deref() {
        if parse(min).is_some_and(|min_version| *app < min_version) {
            return Installability::NeedsNewer(min.trim().to_owned());
        }
    }
    if let Some(max) = version.max_app_version.as_deref() {
        if parse(max).is_some_and(|max_version| *app > max_version) {
            return Installability::NeedsOlder(max.trim().to_owned());
        }
    }
    if !has_checksum(version) {
        return Installability::NoChecksum;
    }
    Installability::Installable
}

fn catalog_version(version: &WireVersion, app: &Version) -> CatalogVersion {
    let (compatible, reason, bound) = match installability(version, app) {
        Installability::Installable => (true, None, None),
        Installability::NeedsNewer(bound) => (false, Some("needs_newer"), Some(bound)),
        Installability::NeedsOlder(bound) => (false, Some("needs_older"), Some(bound)),
        Installability::NoChecksum => (false, Some("no_checksum"), None),
    };
    CatalogVersion {
        version: version.version.clone(),
        bundle_size: version.bundle_size,
        changelog: version.changelog.clone(),
        compatible,
        incompatible_reason: reason.map(str::to_owned),
        bound,
    }
}

/// Newest first by semver (the order of the answer is not guaranteed, research R1); versions that
/// are not semver follow in the order they came.
fn newest_first(mut versions: Vec<CatalogVersion>) -> Vec<CatalogVersion> {
    versions.sort_by(|a, b| match (parse(&a.version), parse(&b.version)) {
        (Some(x), Some(y)) => y.cmp(&x),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    });
    versions
}

/// The newest version this HaexVault can install.
pub fn latest_compatible(versions: &[CatalogVersion]) -> Option<String> {
    versions
        .iter()
        .filter(|version| version.compatible)
        .filter_map(|version| parse(&version.version).map(|parsed| (parsed, &version.version)))
        .max_by(|a, b| a.0.cmp(&b.0))
        .map(|(_, text)| text.clone())
}

/// What is installed under `name` (FR-004: by name; the preview decides by publisher key and
/// name in the end, research R8).
pub fn installed_state(name: &str, installed: &[InstalledExtension]) -> Option<CatalogInstalled> {
    installed
        .iter()
        .find(|extension| extension.name == name)
        .and_then(|extension| {
            extension.version.as_ref().map(|version| CatalogInstalled {
                version: version.clone(),
                enabled: extension.enabled,
            })
        })
}

/// `latest` when it is newer than what is installed.
pub fn update_to(installed: Option<&CatalogInstalled>, latest: Option<&str>) -> Option<String> {
    let installed = parse(&installed?.version)?;
    let latest_text = latest?;
    (parse(latest_text)? > installed).then(|| latest_text.to_owned())
}

/// An entry of the answer as the catalog shows it; `None` for one whose slug cannot be used.
pub fn entry_from_wire(
    source: &CatalogSource,
    item: WireListItem,
    installed: &[InstalledExtension],
    app: &Version,
) -> Option<CatalogEntry> {
    if !valid_slug(&item.slug) {
        log::warn!("a marketplace sent an entry with a slug that cannot be used; it is left out");
        return None;
    }
    let versions = newest_first(
        item.versions
            .iter()
            .map(|version| catalog_version(version, app))
            .collect(),
    );
    let latest = latest_compatible(&versions);
    let installed_here = installed_state(&item.name, installed);
    let update = update_to(installed_here.as_ref(), latest.as_deref());
    Some(CatalogEntry {
        source: source.clone(),
        extension_id: item.extension_id,
        name: item.name,
        slug: item.slug,
        short_description: item.short_description,
        icon_url: item.icon_url,
        verified: item.verified,
        total_downloads: item.total_downloads,
        average_rating: item.average_rating,
        review_count: item.review_count,
        tags: item.tags.unwrap_or_default(),
        publisher: item.publisher.map(|publisher| CatalogPublisher {
            display_name: publisher.display_name,
            slug: publisher.slug,
            verified: publisher.verified,
        }),
        category: item.category.map(|category| CatalogCategory {
            name: category.name,
            slug: category.slug,
        }),
        versions,
        installed: installed_here,
        latest_compatible: latest,
        update_to: update,
    })
}

fn page_query(query: &CatalogQuery) -> Vec<(&'static str, String)> {
    let mut pairs = vec![
        ("page", query.page.max(1).to_string()),
        ("limit", PAGE_LIMIT.to_string()),
        ("sort", "downloads".to_owned()),
    ];
    if let Some(category) = query
        .category
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty())
    {
        pairs.push(("category", category.to_owned()));
    }
    if let Some(search) = query
        .search
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        pairs.push(("search", search.to_owned()));
    }
    pairs
}

async fn ask(
    http: Http,
    source: &Source,
    query: &[(&str, String)],
) -> Result<WireListResponse, SourceFailure> {
    if let Some(kind) = source.unavailable {
        return Err(SourceFailure { kind, detail: None });
    }
    http.get_json(&source.base_url, "/extensions", query, source.auth.as_ref())
        .await
}

/// The entries of a search by name in one source: the update check (research R8) asks for every
/// installed name, the answers are matched exactly by the caller.
pub async fn search(
    http: Http,
    source: &Source,
    name: &str,
    installed: &[InstalledExtension],
    app: &Version,
) -> Result<Vec<CatalogEntry>, SourceFailure> {
    let query = [
        ("page", "1".to_owned()),
        ("limit", PAGE_LIMIT.to_string()),
        ("sort", "downloads".to_owned()),
        ("search", name.to_owned()),
    ];
    let answer = ask(http, source, &query).await?;
    let from = source.catalog_source();
    Ok(answer
        .extensions
        .into_iter()
        .filter_map(|item| entry_from_wire(&from, item, installed, app))
        .collect())
}

/// One page of the common catalog (FR-001, FR-012). Page N is page N of every asked source, one
/// after the other in the order of `sources`; `total_pages` is the largest of the sources and
/// `total` the sum, so no entry disappears while paging. A source that fails adds an error line
/// and nothing else (FR-031).
pub async fn fetch_catalog(
    http: Http,
    sources: &[Source],
    query: &CatalogQuery,
    installed: &[InstalledExtension],
    app: &Version,
) -> CatalogPage {
    let asked: Vec<&Source> = sources
        .iter()
        .filter(|source| query.source_id.as_deref().is_none_or(|id| id == source.id))
        .collect();
    let pairs = page_query(query);
    let answers = join_all(asked.iter().map(|source| ask(http, source, &pairs))).await;

    let mut entries = Vec::new();
    let mut source_errors = Vec::new();
    let mut total_pages = 0;
    let mut total: u64 = 0;
    for (source, answer) in asked.into_iter().zip(answers) {
        match answer {
            Ok(page) => {
                total_pages = total_pages.max(page.pagination.total_pages);
                total = total.saturating_add(page.pagination.total);
                let from = source.catalog_source();
                entries.extend(
                    page.extensions
                        .into_iter()
                        .filter_map(|item| entry_from_wire(&from, item, installed, app)),
                );
            }
            Err(failure) => source_errors.push(source.error(failure)),
        }
    }
    CatalogPage {
        entries,
        page: query.page.max(1),
        total_pages,
        total,
        source_errors,
    }
}

#[cfg(test)]
#[path = "catalog_tests.rs"]
mod tests;
