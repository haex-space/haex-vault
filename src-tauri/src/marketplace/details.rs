//! The categories and the details of an entry (spec 069, FR-005, research R8): categories are the
//! union over the asked sources, details come from the one source an entry belongs to.

use futures::future::join_all;
use semver::Version;

use super::catalog::{entry_from_wire, installed_state, valid_slug, InstalledExtension, Source};
use super::client::{Http, SourceFailure};
use super::model::{CatalogCategories, CatalogCategoryCount, CatalogDetail, SourceErrorKind};
use super::wire::{WireCategories, WireDetail};
use crate::extensions::registry::install::publisher_fingerprint;

/// The categories of all asked sources (`source_id` limits it to one): one per slug, counts
/// added up, the name of the first source that names it, per-source errors.
pub async fn fetch_categories(
    http: Http,
    sources: &[Source],
    source_id: Option<&str>,
) -> CatalogCategories {
    let asked: Vec<&Source> = sources
        .iter()
        .filter(|source| source_id.is_none_or(|id| id == source.id))
        .collect();
    let answers = join_all(asked.iter().map(|source| async move {
        if let Some(kind) = source.unavailable {
            return Err(SourceFailure { kind, detail: None });
        }
        http.get_json::<WireCategories>(&source.base_url, "/categories", &[], source.auth.as_ref())
            .await
    }))
    .await;

    let mut categories: Vec<CatalogCategoryCount> = Vec::new();
    let mut source_errors = Vec::new();
    for (source, answer) in asked.into_iter().zip(answers) {
        match answer {
            Ok(answer) => {
                for category in answer.categories {
                    match categories
                        .iter_mut()
                        .find(|known| known.slug == category.slug)
                    {
                        Some(known) => known.count = known.count.saturating_add(category.count),
                        None => categories.push(CatalogCategoryCount {
                            name: category.name,
                            slug: category.slug,
                            count: category.count,
                        }),
                    }
                }
            }
            Err(failure) => source_errors.push(source.error(failure)),
        }
    }
    CatalogCategories {
        categories,
        source_errors,
    }
}

/// The details of `slug` at `source`: the entry, the description as plain text, and the publisher
/// key as a short fingerprint, compared with the installed extension of the same name.
pub async fn fetch_details(
    http: Http,
    source: &Source,
    slug: &str,
    installed: &[InstalledExtension],
    app: &Version,
) -> Result<CatalogDetail, SourceFailure> {
    if !valid_slug(slug) {
        return Err(SourceFailure {
            kind: SourceErrorKind::BadResponse,
            detail: Some("invalid slug".to_owned()),
        });
    }
    if let Some(kind) = source.unavailable {
        return Err(SourceFailure { kind, detail: None });
    }
    let detail: WireDetail = http
        .get_json(
            &source.base_url,
            &format!("/extensions/{slug}"),
            &[],
            source.auth.as_ref(),
        )
        .await?;
    let key = detail
        .public_key
        .as_deref()
        .filter(|key| key.len() == 64 && key.bytes().all(|b| b.is_ascii_hexdigit()));
    let fingerprint = key.map(|key| publisher_fingerprint(&key.to_ascii_lowercase()));
    let matches_installed = fingerprint.as_ref().and_then(|fingerprint| {
        let name = &detail.item.name;
        installed
            .iter()
            .find(|extension| &extension.name == name && installed_state(name, installed).is_some())
            .map(|extension| &extension.publisher_fingerprint == fingerprint)
    });
    let description = detail.description;
    let author = detail.author;
    let entry = entry_from_wire(&source.catalog_source(), detail.item, installed, app).ok_or(
        SourceFailure {
            kind: SourceErrorKind::BadResponse,
            detail: Some("invalid entry".to_owned()),
        },
    )?;
    Ok(CatalogDetail {
        entry,
        description,
        author,
        publisher_fingerprint: fingerprint,
        publisher_matches_installed: matches_installed,
    })
}

#[cfg(test)]
#[path = "details_tests.rs"]
mod tests;
