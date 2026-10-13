//! The encrypted folders a search of the device passed (spec 048 US7, FR-049): the walk never enters
//! their raw directories; each is searched afterwards by its decrypted names through the device's
//! storage, the folder's own name included. Hits carry device paths.

use std::path::PathBuf;

use tokio_util::sync::CancellationToken;

use super::encrypted::device::DeviceRoot;
use super::encrypted::route::{located_at, located_entry};
use super::search::{
    matcher_for, search_storage, SearchEnd, SearchHit, SearchLimits, SearchOptions,
};
use super::storage_source::{folder_prefix, StorageFiles};

/// Searches the encrypted folders `passed` (their directories on the device) for `query`, after a
/// walk that found `found` hits; `emit` gets hits with device paths.
#[allow(clippy::too_many_arguments)]
pub async fn search_passed(
    files: &StorageFiles,
    root: &DeviceRoot,
    passed: Vec<PathBuf>,
    query: &str,
    options: &SearchOptions,
    mut found: usize,
    cancel: &CancellationToken,
    emit: &mut (dyn FnMut(Vec<SearchHit>) + Send),
) -> SearchEnd {
    let query = query.trim();
    for dir in passed {
        if cancel.is_cancelled() {
            return SearchEnd::Cancelled;
        }
        if found >= options.limits.max_hits {
            return SearchEnd::Done { truncated: true };
        }
        let Ok(prefix) = folder_prefix(&root.view_of(&dir)) else {
            continue;
        };
        // A folder whose head cannot be read is skipped, as a folder without access.
        let Ok(at) = located_at(files, &prefix).await else {
            continue;
        };
        let folder = located_entry(&at);
        if let Some(matched) = matcher_for(query).match_one(folder.name.as_str(), 0) {
            if options.filters.admits(&folder) && (options.show_hidden || !folder.hidden) {
                found += 1;
                emit(vec![SearchHit {
                    entry: root.entry(folder),
                    score: matched.score,
                }]);
            }
        }
        let inside = SearchOptions {
            limits: SearchLimits {
                max_hits: options.limits.max_hits.saturating_sub(found),
                ..options.limits
            },
            ..options.clone()
        };
        let mut counted = 0;
        let end = search_storage(
            files,
            &at.root,
            query,
            &inside,
            cancel,
            &mut |hits: Vec<SearchHit>| {
                counted += hits.len();
                emit(
                    hits.into_iter()
                        .map(|hit| SearchHit {
                            entry: root.entry(hit.entry),
                            score: hit.score,
                        })
                        .collect(),
                );
            },
            &mut |_| {},
        )
        .await;
        found += counted;
        match end {
            SearchEnd::Done { truncated: false } => {}
            other => return other,
        }
    }
    SearchEnd::Done { truncated: false }
}

#[cfg(test)]
#[path = "search_device_tests.rs"]
mod tests;
