//! The filter lists of the tracker blocking leave `agent-browser/filters/` for `filter-lists/`
//! (spec 084, stage 4a): the folder was shared with the profile of a vault named `filters`.
//!
//! Part of the move at start (`layout_move`), under the same device lock. Every file is one
//! rename and so restartable by itself. Only list files move (`layout_adopt::is_list_file`), what
//! else lies in the old folder is a profile and belongs to `layout_adopt`. A file whose target
//! exists is left, not overwritten.

use std::fs;
use std::path::PathBuf;

use super::layout_adopt::is_list_file;
use super::layout_move::{Budget, Stop};
use super::vault_paths::VaultPaths;

/// What a run moved (file names) and what it had to leave, with the reason.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Moved {
    pub done: Vec<String>,
    pub left: Vec<(String, String)>,
}

/// The list files still in the old folder.
fn legacy_files(paths: &VaultPaths) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(paths.legacy_filter_lists()) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .filter(|entry| {
            entry.path().is_file() && entry.file_name().to_str().is_some_and(is_list_file)
        })
        .map(|entry| entry.path())
        .collect();
    files.sort();
    files
}

/// Whether a list file is left at the old place.
pub(super) fn has_work(paths: &VaultPaths) -> bool {
    !legacy_files(paths).is_empty()
}

/// Moves the list files into `filter-lists/`; a failure of one file is `report.left`, only the
/// budget stops the run.
pub(super) fn move_lists(
    paths: &VaultPaths,
    budget: &mut Budget,
    report: &mut Moved,
) -> Result<(), Stop> {
    let files = legacy_files(paths);
    if files.is_empty() {
        return Ok(());
    }
    let target_dir = paths.filter_lists();
    fs::create_dir_all(&target_dir)?;
    for from in files {
        let Some(name) = from.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let to = target_dir.join(name);
        if to.exists() {
            report
                .left
                .push((name.to_string(), format!("{to:?} exists already")));
            continue;
        }
        budget.spend()?;
        match fs::rename(&from, &to) {
            Ok(()) => report.done.push(name.to_string()),
            Err(error) => report.left.push((name.to_string(), error.to_string())),
        }
    }
    // The old folder goes once nothing is in it; the profile of a vault `filters` keeps it.
    let _ = fs::remove_dir(paths.legacy_filter_lists());
    Ok(())
}
