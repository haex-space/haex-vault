//! Searching by name from a folder down (spec 044 FR-027 to FR-030, research R6). A blocking walk
//! with `walkdir`: it never follows links (no loops), stays on the file system it starts on (so a
//! search from `/` skips `/proc`, `/sys`, `/dev`), and skips folders the system denies without a
//! word. Names match typo-tolerant with `frizbee`. Hits go out in batches while the walk runs; the
//! window sorts them by score. Agents search with tighter limits and never see holzi's own places.

use std::path::Path;
use std::time::{Duration, Instant};

use frizbee::{Config, Matcher, Pattern, PatternConfig};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use ts_rs::TS;
use walkdir::WalkDir;

use crate::files::encrypted::format::is_folder_prefix;
use crate::files::kind::{category, FileCategory};
use crate::files::local::ops::{is_hidden, stat};
use crate::files::local::OwnPlaces;
use crate::files::{Entry, EntryKind};

/// What a search keeps to (FR-029). Size and type only let files through.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct SearchFilters {
    #[ts(optional)]
    pub types: Option<Vec<FileCategory>>,
    #[ts(optional, type = "number")]
    pub size_min: Option<u64>,
    #[ts(optional, type = "number")]
    pub size_max: Option<u64>,
    /// Milliseconds since 1970.
    #[ts(optional, type = "number")]
    pub modified_from: Option<i64>,
    #[ts(optional, type = "number")]
    pub modified_to: Option<i64>,
}

impl SearchFilters {
    fn files_only(&self) -> bool {
        self.types.is_some() || self.size_min.is_some() || self.size_max.is_some()
    }

    pub(crate) fn admits(&self, entry: &Entry) -> bool {
        if entry.kind == EntryKind::Dir && self.files_only() {
            return false;
        }
        if let Some(types) = &self.types {
            if !category(&entry.name).is_some_and(|found| types.contains(&found)) {
                return false;
            }
        }
        let size = entry.size.unwrap_or(0);
        if self.size_min.is_some_and(|min| size < min)
            || self.size_max.is_some_and(|max| size > max)
        {
            return false;
        }
        let modified = entry.modified_ms;
        if let Some(from) = self.modified_from {
            if modified.is_none_or(|at| at < from) {
                return false;
            }
        }
        if let Some(to) = self.modified_to {
            if modified.is_none_or(|at| at > to) {
                return false;
            }
        }
        true
    }
}

/// When a search stops early.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchLimits {
    pub max_hits: usize,
    pub max_time: Option<Duration>,
}

impl SearchLimits {
    /// The window: enough to scroll, few enough to stay quick; a sharper query finds the rest.
    pub const USER: Self = Self {
        max_hits: 1_000,
        max_time: None,
    };
    /// Agents (contracts/agent-actions.md `files.search`): 500 hits or 30 seconds.
    pub const AGENT: Self = Self {
        max_hits: 500,
        max_time: Some(Duration::from_secs(30)),
    };
}

/// How a search runs.
#[derive(Debug, Clone)]
pub struct SearchOptions {
    pub filters: SearchFilters,
    pub show_hidden: bool,
    /// holzi's own places: hits in them are marked read-only.
    pub own: OwnPlaces,
    /// Leaves the own places out with all below (agents, FR-033).
    pub hide_own: bool,
    pub limits: SearchLimits,
}

/// One hit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct SearchHit {
    pub entry: Entry,
    /// Higher is better.
    pub score: u16,
}

/// How a search ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchEnd {
    /// `truncated`: a limit stopped it before the walk was through.
    Done {
        truncated: bool,
    },
    Cancelled,
}

/// The typos a needle may hold: one up to five characters, else two (research R6).
pub fn max_typos(needle: &str) -> u16 {
    if needle.chars().count() <= 5 {
        1
    } else {
        2
    }
}

/// The typo-tolerant, case-blind matcher of a query (research R6).
pub(crate) fn matcher_for(query: &str) -> Matcher {
    let pattern = Pattern::new(
        query,
        PatternConfig::default().max_typos(Some(max_typos(query))),
    );
    Matcher::new(
        pattern,
        &Config {
            casing: frizbee::CaseMatching::Ignore,
            ..Config::default()
        },
    )
}

/// How often hits go out at most.
const BATCH_EVERY: Duration = Duration::from_millis(100);

/// Searches `root` and below for names like `query`; hands hits to `emit` in batches. The raw
/// directories of encrypted folders are neither entered nor hits (spec 048 FR-049).
pub fn search(
    root: &Path,
    query: &str,
    options: &SearchOptions,
    cancel: &CancellationToken,
    emit: &mut dyn FnMut(Vec<SearchHit>),
) -> SearchEnd {
    search_passing(root, query, options, cancel, emit, &mut Vec::new())
}

/// [`search`], noting in `passed` the directories of encrypted folders it went past, so they can
/// be searched by their decrypted names ([`super::search_device`]).
pub fn search_passing(
    root: &Path,
    query: &str,
    options: &SearchOptions,
    cancel: &CancellationToken,
    emit: &mut dyn FnMut(Vec<SearchHit>),
    passed: &mut Vec<std::path::PathBuf>,
) -> SearchEnd {
    let query = query.trim();
    if query.is_empty() {
        return SearchEnd::Done { truncated: false };
    }
    let mut matcher = matcher_for(query);
    let started = Instant::now();
    // `None` until the first hit went out: that one goes at once (SC-004).
    let mut last: Option<Instant> = None;
    let mut batch = Vec::new();
    let mut found = 0;
    let walk = WalkDir::new(root)
        .follow_links(false)
        .same_file_system(true)
        .min_depth(1)
        // A folder's files before its sub folders: hits close to the start come first (SC-004).
        .sort_by(|a, b| {
            let dir = |item: &walkdir::DirEntry| item.file_type().is_dir();
            dir(a)
                .cmp(&dir(b))
                .then_with(|| a.file_name().cmp(b.file_name()))
        })
        .into_iter()
        .filter_entry(|item| {
            let encrypted = item.file_type().is_dir()
                && item.file_name().to_str().is_some_and(is_folder_prefix);
            if encrypted {
                passed.push(item.path().to_path_buf());
                return false;
            }
            let hidden = !options.show_hidden
                && item
                    .metadata()
                    .is_ok_and(|meta| is_hidden(&item.file_name().to_string_lossy(), &meta));
            let off_limits = options.hide_own && options.own.contains_resolved(item.path());
            !hidden && !off_limits
        });
    let mut end = SearchEnd::Done { truncated: false };
    // Folders the system denies come as errors; they are skipped (FR-030).
    for item in walk.flatten() {
        if cancel.is_cancelled() {
            return SearchEnd::Cancelled;
        }
        if options
            .limits
            .max_time
            .is_some_and(|limit| started.elapsed() >= limit)
        {
            end = SearchEnd::Done { truncated: true };
            break;
        }
        if !batch.is_empty() && last.is_none_or(|at| at.elapsed() >= BATCH_EVERY) {
            last = Some(Instant::now());
            emit(std::mem::take(&mut batch));
        }
        let name = item.file_name().to_string_lossy();
        let Some(matched) = matcher.match_one(name.as_ref(), 0) else {
            continue;
        };
        let Ok(entry) = stat(item.path(), &options.own) else {
            continue;
        };
        if !options.filters.admits(&entry) {
            continue;
        }
        batch.push(SearchHit {
            entry,
            score: matched.score,
        });
        found += 1;
        if last.is_none() {
            last = Some(Instant::now());
            emit(std::mem::take(&mut batch));
        }
        if found >= options.limits.max_hits {
            end = SearchEnd::Done { truncated: true };
            break;
        }
    }
    if cancel.is_cancelled() {
        return SearchEnd::Cancelled;
    }
    if !batch.is_empty() {
        emit(batch);
    }
    end
}

/// Searches a storage from the folder `root` down (spec 044 US5, FR-030): folder by folder through
/// its listings, with the same matching, filters and limits as on the device. `progress` hears how
/// many folders are searched.
pub async fn search_storage(
    files: &crate::files::storage_source::StorageFiles,
    root: &str,
    query: &str,
    options: &SearchOptions,
    cancel: &CancellationToken,
    emit: &mut (dyn FnMut(Vec<SearchHit>) + Send),
    progress: &mut (dyn FnMut(u64) + Send),
) -> SearchEnd {
    let query = query.trim();
    if query.is_empty() {
        return SearchEnd::Done { truncated: false };
    }
    let started = Instant::now();
    let mut folders = std::collections::VecDeque::from([root.to_owned()]);
    let mut searched = 0;
    let mut found = 0;
    while let Some(folder) = folders.pop_front() {
        if cancel.is_cancelled() {
            return SearchEnd::Cancelled;
        }
        if options
            .limits
            .max_time
            .is_some_and(|limit| started.elapsed() >= limit)
        {
            return SearchEnd::Done { truncated: true };
        }
        // A folder that cannot be listed is skipped, as one without access on the device.
        let Ok(entries) = files.list(&folder).await else {
            continue;
        };
        if cancel.is_cancelled() {
            return SearchEnd::Cancelled;
        }
        searched += 1;
        progress(searched);
        let mut batch = Vec::new();
        let mut matcher = matcher_for(query);
        for entry in entries {
            if !options.show_hidden && entry.hidden {
                continue;
            }
            if entry.kind == EntryKind::Dir {
                // Agents have no encrypted-folder scope. Do not search or expose the raw HXEF
                // prefix: fuzzy matching can otherwise mistake its random name for a query.
                if files.scope().is_none() && is_folder_prefix(&entry.name) {
                    continue;
                }
                folders.push_back(entry.path.clone());
            }
            let Some(matched) = matcher.match_one(entry.name.as_str(), 0) else {
                continue;
            };
            if !options.filters.admits(&entry) {
                continue;
            }
            batch.push(SearchHit {
                entry,
                score: matched.score,
            });
            found += 1;
            if found >= options.limits.max_hits {
                emit(batch);
                return SearchEnd::Done { truncated: true };
            }
        }
        drop(matcher);
        if !batch.is_empty() {
            emit(batch);
        }
    }
    SearchEnd::Done { truncated: false }
}

/// What the window hears of a search.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum FilesSearchEvent {
    Hits {
        hits: Vec<SearchHit>,
    },
    /// How many folders of a storage are searched so far (FR-030: a storage shows its progress).
    Progress {
        #[ts(type = "number")]
        dirs: u64,
    },
    /// The walk is through, or a limit stopped it (`truncated`).
    Done {
        truncated: bool,
    },
}

pub use super::search_manager::SearchManager;

#[cfg(test)]
#[path = "search_tests.rs"]
mod tests;
