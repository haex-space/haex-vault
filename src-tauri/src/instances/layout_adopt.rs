//! The folders of the agent browser profile and the chat files of a vault move into its folder
//! (spec 084, stage 3): `agent-browser/<name>/` becomes `vaults/<name>/agent-browser/` and
//! `chat-files/<name>/` becomes `vaults/<name>/chat-files/`.
//!
//! Part of the move at start (`layout_move`), after the databases, under the same device lock.
//! A folder is one rename, so it needs no staging and is restartable by itself: whatever is still
//! at the old place is adopted at the next start. A vault that another process holds open is not
//! touched (the same lock probe as for the database), a target that exists is not overwritten.
//!
//! One name is special: the tracker lists lay in `agent-browser/filters/`, so a vault named
//! `filters` shares that folder with them. Its profile is adopted entry by entry and the list
//! files stay for `layout_lists`.

use std::fs;
use std::path::{Path, PathBuf};

use super::layout_move::{Budget, Stop};
use super::paths::validate_instance_name;
use super::vault_lock::{probe, Probe};
use super::vault_paths::VaultPaths;

/// The vault whose profile folder is also the folder of the tracker lists.
const SHARED_WITH_LISTS: &str = "filters";

/// What a run adopted, as `<name>/<subfolder>`, and what it had to leave, with the reason.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Adopted {
    pub done: Vec<String>,
    pub left: Vec<(String, String)>,
}

/// A file of the tracker lists (`agent_browser::filter_lists`, `filter_settings`).
pub(super) fn is_list_file(name: &str) -> bool {
    name == "settings.json" || name.ends_with(".txt") || name.ends_with(".part")
}

/// One rename to do: `from` (a folder, or an entry of one) to `to`.
struct Step {
    from: PathBuf,
    to: PathBuf,
}

/// What is left to adopt of the folder `from` into `to`. The whole folder when `to` is missing;
/// for the profile of the vault `filters` its entries that are no list file.
fn steps(from: &Path, to: &Path, shared_with_lists: bool) -> Result<Vec<Step>, String> {
    if !from.is_dir() {
        return Ok(Vec::new());
    }
    if !shared_with_lists {
        if to.exists() {
            return Err(format!("{to:?} exists already"));
        }
        return Ok(vec![Step {
            from: from.to_path_buf(),
            to: to.to_path_buf(),
        }]);
    }
    let mut steps = Vec::new();
    for entry in fs::read_dir(from)
        .map_err(|error| error.to_string())?
        .flatten()
    {
        let file = entry.file_name();
        let Some(file) = file.to_str() else { continue };
        if is_list_file(file) {
            continue;
        }
        let target = to.join(file);
        if target.exists() {
            return Err(format!("{target:?} exists already"));
        }
        steps.push(Step {
            from: entry.path(),
            to: target,
        });
    }
    Ok(steps)
}

/// The two adoptions of the vault `name`: (subfolder, old place, new place, shares the folder with
/// the tracker lists).
fn candidates(paths: &VaultPaths, name: &str) -> [(&'static str, PathBuf, PathBuf, bool); 2] {
    [
        (
            "agent-browser",
            paths.legacy_agent_browser(name),
            paths.agent_browser(name),
            name == SHARED_WITH_LISTS,
        ),
        (
            "chat-files",
            paths.legacy_chat_files(name),
            paths.chat_files(name),
            false,
        ),
    ]
}

/// The names with a vault folder and something left at an old place. Cheap: two directory reads.
pub(super) fn names(paths: &VaultPaths) -> Vec<String> {
    let mut found = Vec::new();
    for root in [paths.agent_browser_root(), paths.legacy_chat_files_root()] {
        let Ok(entries) = fs::read_dir(root) else {
            continue;
        };
        for entry in entries.flatten() {
            let file = entry.file_name();
            let Some(name) = file.to_str() else { continue };
            if validate_instance_name(name).is_ok()
                && paths.folder(name).is_dir()
                && !found.iter().any(|known| known == name)
            {
                found.push(name.to_string());
            }
        }
    }
    found.sort();
    // A name whose only leftovers are errors would keep the work going; the caller asks for the
    // steps, so a name without any is no work.
    found.retain(|name| {
        candidates(paths, name)
            .iter()
            .any(|(_, from, to, shared)| steps(from, to, *shared).is_ok_and(|s| !s.is_empty()))
    });
    found
}

/// Adopts the folders of the vault `name` and records what happened in `report`. Only a stop of
/// the budget or a folder that cannot be made is an error; a rename that fails is `report.left`.
pub(super) fn adopt(
    paths: &VaultPaths,
    name: &str,
    budget: &mut Budget,
    report: &mut Adopted,
) -> Result<(), Stop> {
    let work = candidates(paths, name);
    // Held means another process has the vault open and uses its folders where they are.
    let _guard = match probe(&paths.database(name)) {
        Ok(Probe::Free(guard)) => guard,
        Ok(Probe::Held) => {
            report
                .left
                .push((name.to_string(), "open in another process".to_string()));
            return Ok(());
        }
        Err(error) => {
            report.left.push((name.to_string(), error));
            return Ok(());
        }
    };
    for (subfolder, from, to, shared) in work {
        let label = format!("{name}/{subfolder}");
        let steps = match steps(&from, &to, shared) {
            Ok(steps) => steps,
            Err(reason) => {
                report.left.push((label, reason));
                continue;
            }
        };
        if steps.is_empty() {
            continue;
        }
        if shared {
            fs::create_dir_all(&to)?;
        } else if let Some(parent) = to.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut failed = None;
        for step in steps {
            budget.spend()?;
            if let Err(error) = fs::rename(&step.from, &step.to) {
                failed = Some(error.to_string());
                break;
            }
        }
        match failed {
            None => {
                // The old folder goes once nothing is in it; the lists of `filters` keep it.
                let _ = fs::remove_dir(&from);
                report.done.push(label);
            }
            Some(reason) => report.left.push((label, reason)),
        }
    }
    Ok(())
}
