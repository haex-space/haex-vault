//! The move into vault folders (spec 084, research R4, R5; stage 3 adds the folders of the
//! profile and the chat files, `layout_adopt`): every vault of the old layout
//! (`instances/<name>.db` with its sidecar files) gets its folder `vaults/<name>/` with the
//! database called `vault.db`.
//!
//! Runs once per start in `setup`, before any command, so nothing lists or opens a vault while it
//! moves. Rules it keeps:
//!
//! - The files of one vault are never split across two visible places (SQLite needs `-wal` and
//!   `-shm` beside the database). They are renamed one by one into the hidden folder
//!   `vaults/.moving-<name>/`, and only when all are there that folder is renamed to
//!   `vaults/<name>/`. A crash at any point leaves a state the next run continues from.
//! - A vault another process holds open is left where it is (`vault_lock::probe`); the next start
//!   tries again.
//! - Nothing is overwritten and no vault data is deleted: a target that exists, a held vault or a
//!   rename that fails leaves the vault alone and is logged.
//! - Two processes starting at once take turns on the device lock `vaults.lock`.

use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

use super::layout_adopt::{self, Adopted};
use super::layout_lists::{self, Moved};
use super::paths::validate_instance_name;
use super::vault_lock::{probe, Probe};
use super::vault_paths::{VaultPaths, DATABASE_FILE, STAGING_PREFIX};

/// What a run did.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    /// Vaults that now lie in their folder.
    pub moved: Vec<String>,
    /// Vaults left where they were, with the reason. They are tried again at the next start.
    pub left: Vec<(String, String)>,
    /// The profile and chat files folders adopted into their vault folders (`<name>/<subfolder>`).
    pub adopted: Vec<String>,
    /// Folders left at their old place, with the reason; tried again at the next start.
    pub adoption_left: Vec<(String, String)>,
    /// The list files moved to `filter-lists/` (file names).
    pub lists: Vec<String>,
    /// List files left at their old place, with the reason; tried again at the next start.
    pub lists_left: Vec<(String, String)>,
    /// The run stopped on purpose after its step budget (tests only; see [`Budget`]).
    pub stopped: bool,
}

/// How many file operations a run may still do. Production runs are unlimited; a test sets a
/// number to stop a run in the middle, as a kill would.
pub struct Budget(Option<usize>);

impl Budget {
    pub fn unlimited() -> Self {
        Self(None)
    }

    pub fn steps(count: usize) -> Self {
        Self(Some(count))
    }

    pub(super) fn spend(&mut self) -> Result<(), Stop> {
        match &mut self.0 {
            Some(0) => Err(Stop::Killed),
            Some(left) => {
                *left -= 1;
                Ok(())
            }
            None => Ok(()),
        }
    }
}

pub(super) enum Stop {
    /// This vault stays; the reason is logged.
    Left(String),
    /// The budget ran out.
    Killed,
}

impl From<io::Error> for Stop {
    fn from(error: io::Error) -> Self {
        Stop::Left(error.to_string())
    }
}

/// Moves every vault of the old layout. Returns once everything that can move has moved.
pub fn run(paths: &VaultPaths) -> io::Result<Outcome> {
    run_with(paths, &mut Budget::unlimited())
}

/// [`run`] with a step budget.
pub fn run_with(paths: &VaultPaths, budget: &mut Budget) -> io::Result<Outcome> {
    let mut outcome = Outcome::default();
    if !has_work(paths) {
        return Ok(outcome);
    }
    // Held until the function returns; a second process waits here and then finds nothing to do.
    let _device = lock_device(&paths.device_lock())?;
    fs::create_dir_all(paths.vaults_dir())?;

    for name in names(paths) {
        match move_vault(paths, &name, budget) {
            Ok(true) => outcome.moved.push(name),
            Ok(false) => {}
            Err(Stop::Left(reason)) => outcome.left.push((name, reason)),
            Err(Stop::Killed) => {
                outcome.stopped = true;
                return Ok(outcome);
            }
        }
    }
    // The old folder goes once nothing is in it; a leftover keeps it.
    let _ = fs::remove_dir(paths.legacy_dir());

    // The folders of the profile and the chat files follow their database (stage 3).
    let mut adopted = Adopted::default();
    for name in layout_adopt::names(paths) {
        match layout_adopt::adopt(paths, &name, budget, &mut adopted) {
            Ok(()) => {}
            Err(Stop::Left(reason)) => adopted.left.push((name, reason)),
            Err(Stop::Killed) => {
                outcome.stopped = true;
                break;
            }
        }
    }
    outcome.adopted = adopted.done;
    outcome.adoption_left = adopted.left;
    if outcome.stopped {
        return Ok(outcome);
    }

    // The tracker lists leave the folder they shared with a profile (stage 4a).
    let mut lists = Moved::default();
    match layout_lists::move_lists(paths, budget, &mut lists) {
        Ok(()) => {}
        Err(Stop::Left(reason)) => lists.left.push(("filters".to_string(), reason)),
        Err(Stop::Killed) => outcome.stopped = true,
    }
    outcome.lists = lists.done;
    outcome.lists_left = lists.left;
    Ok(outcome)
}

/// The tracing entry for `setup`: runs the move and logs what happened.
pub fn run_at_start(paths: &VaultPaths) {
    match run(paths) {
        Ok(outcome) => {
            for name in &outcome.moved {
                log::info!("vault folders: moved `{name}` into its folder");
            }
            for (name, reason) in &outcome.left {
                log::warn!("vault folders: `{name}` stays where it is for now: {reason}");
            }
            for label in &outcome.adopted {
                log::info!("vault folders: moved `{label}` into the vault folder");
            }
            for (label, reason) in &outcome.adoption_left {
                log::warn!("vault folders: `{label}` stays where it is for now: {reason}");
            }
            for file in &outcome.lists {
                log::info!("vault folders: moved the filter list file `{file}` to filter-lists");
            }
            for (file, reason) in &outcome.lists_left {
                log::warn!("vault folders: filter list file `{file}` stays for now: {reason}");
            }
        }
        Err(error) => log::warn!("vault folders: the move could not start: {error}"),
    }
}

fn has_work(paths: &VaultPaths) -> bool {
    paths.legacy_dir().exists()
        || !staged_names(paths).is_empty()
        || !layout_adopt::names(paths).is_empty()
        || layout_lists::has_work(paths)
}

fn lock_device(path: &Path) -> io::Result<File> {
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(path)?;
    file.lock()?;
    Ok(file)
}

/// Every name with something to move: databases and markers of the old layout, and half moved
/// vaults in staging.
fn names(paths: &VaultPaths) -> BTreeSet<String> {
    let mut found = staged_names(paths);
    if let Ok(entries) = fs::read_dir(paths.legacy_dir()) {
        for entry in entries.flatten() {
            let file = entry.file_name();
            let Some(file) = file.to_str() else { continue };
            let stem = file
                .strip_suffix(".db")
                .or_else(|| file.strip_suffix(".db.pending"));
            if let Some(name) = stem.filter(|name| validate_instance_name(name).is_ok()) {
                found.insert(name.to_string());
            }
        }
    }
    found
}

fn staged_names(paths: &VaultPaths) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    if let Ok(entries) = fs::read_dir(paths.vaults_dir()) {
        for entry in entries.flatten() {
            let file = entry.file_name();
            let name = file
                .to_str()
                .and_then(|file| file.strip_prefix(STAGING_PREFIX));
            if let Some(name) = name.filter(|name| validate_instance_name(name).is_ok()) {
                found.insert(name.to_string());
            }
        }
    }
    found
}

/// The files of the old layout that belong to `name`, with the name they get in the folder. The
/// lock file is not among them: it carries no data and is not moved (see [`remove_lock`]).
fn legacy_files(paths: &VaultPaths, name: &str) -> Vec<(PathBuf, String)> {
    let prefix = format!("{name}.db");
    let mut files = Vec::new();
    if let Ok(entries) = fs::read_dir(paths.legacy_dir()) {
        for entry in entries.flatten() {
            let file = entry.file_name();
            let Some(file) = file.to_str() else { continue };
            let Some(rest) = file.strip_prefix(&prefix) else {
                continue;
            };
            if !(rest.is_empty() || rest.starts_with('.') || rest.starts_with('-')) {
                continue;
            }
            if rest != ".lock" {
                files.push((entry.path(), format!("{DATABASE_FILE}{rest}")));
            }
        }
    }
    // The marker first (an interrupted creation stays recognisable), the database last.
    files.sort_by_key(|(_, target)| match target.as_str() {
        "vault.db.pending" => 0,
        DATABASE_FILE => 2,
        _ => 1,
    });
    files
}

/// Moves one vault. `Ok(true)` when it was moved now, `Ok(false)` when there was nothing to do.
fn move_vault(paths: &VaultPaths, name: &str, budget: &mut Budget) -> Result<bool, Stop> {
    let folder = paths.folder(name);
    let staging = paths.staging(name);
    let files = legacy_files(paths, name);

    if folder.exists() {
        if files.is_empty() {
            // Done by an earlier run; only the lock file is left.
            remove_lock(paths, name);
            let _ = fs::remove_dir(&staging);
            return Ok(false);
        }
        return Err(Stop::Left(format!(
            "{folder:?} exists already and files of the old layout are left"
        )));
    }

    // The same lock `Database::open` takes, on the old place: held means another process has the
    // vault open. Kept until the completed folder is published, so a process waiting on the old
    // lock cannot create a new database at the legacy path while the move is finishing.
    let guard = match probe(&paths.legacy_database(name)) {
        Ok(Probe::Free(guard)) => guard,
        Ok(Probe::Held) => return Err(Stop::Left("open in another process".to_string())),
        Err(error) => return Err(Stop::Left(error)),
    };

    fs::create_dir_all(&staging)?;
    for (source, target) in files {
        let target = staging.join(target);
        if target.exists() {
            return Err(Stop::Left(format!("{target:?} exists already")));
        }
        budget.spend()?;
        fs::rename(source, target)?;
    }
    budget.spend()?;
    fs::rename(&staging, &folder)?;
    drop(guard);
    remove_lock(paths, name);
    Ok(true)
}

/// The lock file of the old place: an advisory lock's anchor with no content. The vault takes a
/// new one in its folder at the next open.
fn remove_lock(paths: &VaultPaths, name: &str) {
    let mut lock = paths.legacy_database(name).into_os_string();
    lock.push(".lock");
    let lock = PathBuf::from(lock);
    match fs::remove_file(&lock) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => log::warn!("vault folders: could not remove {lock:?}: {error}"),
    }
}

#[cfg(test)]
#[path = "layout_move_tests.rs"]
mod tests;
