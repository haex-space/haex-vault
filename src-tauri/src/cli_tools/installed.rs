//! What is installed in a vault, read from the file tree (spec 094, `data-model.md`).
//!
//! `<vault>/cli-tools/<vendor>/<version>/` is complete when it holds a readable `install.json`
//! written for this vendor and this version and the program the marker names, with exactly the
//! size the marker recorded. The marker is written last into a staging folder that is then
//! renamed, so a folder without one is never mistaken for a usable version. Nothing here
//! touches the network, and no hash is read: the hash is checked once, at install (spec FR-006).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use semver::Version;
use serde::{Deserialize, Serialize};

use super::catalog::VendorDefinition;
use super::platform::PlatformKey;

/// The file that marks a version folder complete.
pub const MARKER_FILE: &str = "install.json";
/// Created in a version folder when a run with that version ended normally (spec FR-022).
pub const CONFIRMED_FILE: &str = ".confirmed";
/// A folder `.<version>.<uuid>.staging` is an install attempt in progress or interrupted.
pub const STAGING_SUFFIX: &str = ".staging";
/// A displaced installation kept until its replacement has been validated.
pub const BACKUP_SUFFIX: &str = ".backup";

const SCHEMA: u32 = 1;

/// `install.json`: what was installed, for which platform, and how big the program is.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstallMarker {
    pub schema: u32,
    pub vendor: String,
    pub version: String,
    /// The platform key the program was installed for.
    pub platform: String,
    /// The SHA-256 of the download.
    pub sha256: String,
    /// The size of the download in bytes.
    pub size: u64,
    /// The size of the program file in bytes, compared at each start.
    pub executable_size: u64,
    /// Every regular package file, with a normalized vault-relative path and its installed size.
    pub files: Vec<InstalledFile>,
    /// Unix seconds.
    pub installed_at: u64,
}

/// A regular package file whose presence and size are checked before a tool starts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstalledFile {
    pub path: String,
    pub size: u64,
}

impl InstallMarker {
    /// A marker for a version just unpacked. `installed_at` is the current time.
    pub fn new(
        vendor: &str,
        version: &str,
        platform: PlatformKey,
        sha256: &str,
        size: u64,
        executable_size: u64,
        files: Vec<InstalledFile>,
    ) -> Self {
        let installed_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_secs());
        Self {
            schema: SCHEMA,
            vendor: vendor.to_string(),
            version: version.to_string(),
            platform: platform.as_str().to_string(),
            sha256: sha256.to_string(),
            size,
            executable_size,
            files,
            installed_at,
        }
    }

    /// Writes the marker into `dir` (the last step before a staging folder is published).
    pub fn write(&self, dir: &Path) -> io::Result<()> {
        let json = serde_json::to_vec_pretty(self).map_err(io::Error::other)?;
        fs::write(dir.join(MARKER_FILE), json)
    }

    /// Reads the marker of a version folder; `None` when it is missing or not readable.
    pub fn read(dir: &Path) -> Option<Self> {
        let bytes = fs::read(dir.join(MARKER_FILE)).ok()?;
        let marker: Self = serde_json::from_slice(&bytes).ok()?;
        (marker.schema == SCHEMA).then_some(marker)
    }
}

/// A complete installation that fits this device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installation {
    pub vendor: String,
    pub version: String,
    /// The version folder.
    pub dir: PathBuf,
    /// The absolute path of the program.
    pub executable: PathBuf,
    /// Whether a run with this version has ended normally (spec FR-022).
    pub confirmed: bool,
    pub marker: InstallMarker,
}

/// Why a folder is not a usable installation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Misfit {
    /// Installed for another operating system or architecture.
    Platform,
    /// A file is missing, the marker is unreadable or does not match, or the program's size
    /// differs from the recorded one.
    Incomplete,
}

/// A version folder as found, newest first in [`scan`]'s result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Found {
    Usable(Box<Installation>),
    Unusable { version: String, why: Misfit },
}

impl Found {
    pub fn version(&self) -> &str {
        match self {
            Found::Usable(installation) => &installation.version,
            Found::Unusable { version, .. } => version,
        }
    }
}

/// What a vault has of one vendor's tool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    /// The highest complete version that fits this device.
    Usable(Box<Installation>),
    /// A folder exists but none is usable; reinstalling is the way out.
    NeedsReinstall(Misfit),
    NotInstalled,
}

/// Every version folder of `vendor_dir`, newest first. Folders that are not named like a version
/// (the staging folders, the lock file, anything stray) are not installations and are skipped.
pub fn scan(vendor_dir: &Path, vendor: &VendorDefinition, host: Option<PlatformKey>) -> Vec<Found> {
    let Ok(entries) = fs::read_dir(vendor_dir) else {
        return Vec::new();
    };
    let mut found: Vec<(Version, Found)> = entries
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let version = Version::parse(&name).ok()?;
            Some((version, inspect(&entry.path(), &name, vendor, host)))
        })
        .collect();
    found.sort_by(|a, b| b.0.cmp(&a.0));
    found.into_iter().map(|(_, entry)| entry).collect()
}

fn inspect(dir: &Path, name: &str, vendor: &VendorDefinition, host: Option<PlatformKey>) -> Found {
    let unusable = |why| Found::Unusable {
        version: name.to_string(),
        why,
    };
    let Some(marker) = InstallMarker::read(dir) else {
        return unusable(Misfit::Incomplete);
    };
    if marker.vendor != vendor.vendor || marker.version != name {
        return unusable(Misfit::Incomplete);
    }
    let Some(installed_for) = PlatformKey::parse(&marker.platform) else {
        return unusable(Misfit::Incomplete);
    };
    // A tool installed for the host's own key, or for the key the host falls back to (a Linux
    // x86-64 host without AVX2 runs what was installed for `linux-x64`), fits this device.
    if !host.is_some_and(|key| key.candidates().contains(&installed_for)) {
        return unusable(Misfit::Platform);
    }
    let Some(relative) = vendor.executable_for(installed_for) else {
        return unusable(Misfit::Incomplete);
    };
    let executable = dir.join(relative);
    if !marker
        .files
        .iter()
        .any(|file| file.path == relative.replace('\\', "/") && file.size == marker.executable_size)
        || !marker_files_are_complete(dir, &marker)
    {
        return unusable(Misfit::Incomplete);
    }
    Found::Usable(Box::new(Installation {
        vendor: vendor.vendor.clone(),
        version: name.to_string(),
        confirmed: dir.join(CONFIRMED_FILE).is_file(),
        dir: dir.to_path_buf(),
        executable,
        marker,
    }))
}

fn marker_files_are_complete(dir: &Path, marker: &InstallMarker) -> bool {
    if marker.files.is_empty() {
        return false;
    }
    let mut seen = std::collections::HashSet::with_capacity(marker.files.len());
    marker.files.iter().all(|file| {
        if !is_normalized_package_path(&file.path) || !seen.insert(&file.path) {
            return false;
        }
        let path = dir.join(&file.path);
        fs::symlink_metadata(path)
            .is_ok_and(|metadata| metadata.file_type().is_file() && metadata.len() == file.size)
    })
}

fn is_normalized_package_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.contains(':')
        && path
            .split('/')
            .all(|component| !component.is_empty() && component != "." && component != "..")
}

/// What a vault has of the vendor's tool: the highest usable version, or the reason there is
/// none.
pub fn resolve(
    vendor_dir: &Path,
    vendor: &VendorDefinition,
    host: Option<PlatformKey>,
) -> Resolution {
    resolve_found(&scan(vendor_dir, vendor, host))
}

/// [`resolve`] for a folder that was scanned already.
pub fn resolve_found(found: &[Found]) -> Resolution {
    let mut first_misfit = None;
    for entry in found.iter().cloned() {
        match entry {
            Found::Usable(installation) => return Resolution::Usable(installation),
            Found::Unusable { why, .. } => {
                first_misfit.get_or_insert(why);
            }
        }
    }
    first_misfit.map_or(Resolution::NotInstalled, Resolution::NeedsReinstall)
}

/// Marks `version` as proven: a run with it ended normally (spec FR-022). Returns whether the
/// mark was new. The marker is an empty file in the version folder, so a vault that is copied
/// keeps it with the version.
pub fn confirm(vendor_dir: &Path, version: &str) -> io::Result<bool> {
    let marker = vendor_dir.join(version).join(CONFIRMED_FILE);
    if marker.is_file() {
        return Ok(false);
    }
    fs::write(marker, b"")?;
    Ok(true)
}

/// Deletes every version folder below the newest usable one, once that one is confirmed, except
/// those `in_use` says a run still uses (they stay until that run ends: the next call, from the
/// end of the run or the next vault open, removes them). Returns how many it removed.
///
/// Older folders that are not usable go as well: after the new version is proven nothing of the
/// old ones is worth keeping. A newest version that is not confirmed removes nothing, because the
/// older one is its way back.
pub fn prune_older(
    vendor_dir: &Path,
    vendor: &VendorDefinition,
    host: Option<PlatformKey>,
    in_use: &dyn Fn(&str) -> bool,
) -> usize {
    let found = scan(vendor_dir, vendor, host);
    let Some((newest, rest)) = split_newest_usable(&found) else {
        return 0;
    };
    if !newest.confirmed {
        return 0;
    }
    rest.iter()
        .filter(|entry| !in_use(entry.version()))
        .filter(|entry| remove_entry(&vendor_dir.join(entry.version())))
        .count()
}

/// The newest usable installation and the entries below it; entries above it (a newer folder that
/// is not usable) are neither.
fn split_newest_usable(found: &[Found]) -> Option<(&Installation, &[Found])> {
    let position = found
        .iter()
        .position(|entry| matches!(entry, Found::Usable(_)))?;
    match &found[position] {
        Found::Usable(installation) => Some((installation, &found[position + 1..])),
        Found::Unusable { .. } => None,
    }
}

/// Why a revert could not be done.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevertError {
    /// The newest version is confirmed, or there is no older complete version to go back to.
    NothingToRevert,
    Io,
}

/// Goes back to the older version while the newest is unconfirmed: deletes the newest folder and
/// returns the version that is in use afterwards. The caller has made sure no run uses the tool.
pub fn revert(
    vendor_dir: &Path,
    vendor: &VendorDefinition,
    host: Option<PlatformKey>,
) -> Result<String, RevertError> {
    let found = scan(vendor_dir, vendor, host);
    let (newest, rest) = split_newest_usable(&found).ok_or(RevertError::NothingToRevert)?;
    let previous = rest
        .iter()
        .find_map(|entry| match entry {
            Found::Usable(installation) => Some(installation.version.clone()),
            Found::Unusable { .. } => None,
        })
        .ok_or(RevertError::NothingToRevert)?;
    if newest.confirmed {
        return Err(RevertError::NothingToRevert);
    }
    fs::remove_dir_all(&newest.dir).map_err(|_| RevertError::Io)?;
    Ok(previous)
}

/// The older complete version a revert would go back to, while the newest is unconfirmed.
pub fn previous_version(found: &[Found]) -> Option<String> {
    let (newest, rest) = split_newest_usable(found)?;
    if newest.confirmed {
        return None;
    }
    rest.iter().find_map(|entry| match entry {
        Found::Usable(installation) => Some(installation.version.clone()),
        Found::Unusable { .. } => None,
    })
}

/// Whether a folder of a version older than the newest usable one is still on disk, which is a
/// fallback waiting for its last run to end (or a deletion that failed).
pub fn older_remains(found: &[Found]) -> bool {
    split_newest_usable(found).is_some_and(|(_, rest)| !rest.is_empty())
}

/// Removes the staging folders of every vendor below `cli_tools_dir` (`<vault>/cli-tools/`):
/// attempts that a kill or a crash interrupted. Returns how many it removed. A failure leaves
/// the folder for the next vault open; the log line carries no path (spec FR-019).
pub fn sweep_staging(vault_dir: &Path, cli_tools_dir: &Path) -> usize {
    if !sweep_path_is_safe(vault_dir, cli_tools_dir) {
        log::warn!("cli tools: staging cleanup skipped because its root is unsafe");
        return 0;
    }
    let Ok(vendors) = fs::read_dir(cli_tools_dir) else {
        return 0;
    };
    let mut removed = 0;
    for vendor in vendors.flatten() {
        if !vendor.file_type().is_ok_and(|kind| kind.is_dir()) {
            continue;
        }
        let Ok(entries) = fs::read_dir(vendor.path()) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with('.') && name.ends_with(STAGING_SUFFIX) {
                removed += usize::from(remove_entry(&entry.path()));
            } else if name.starts_with('.') && name.ends_with(BACKUP_SUFFIX) {
                let Some((version, _)) = name
                    .strip_prefix('.')
                    .and_then(|name| name.strip_suffix(BACKUP_SUFFIX))
                    .and_then(|name| name.rsplit_once('.'))
                else {
                    continue;
                };
                let target = vendor.path().join(version);
                if target.exists() {
                    removed += usize::from(remove_entry(&entry.path()));
                } else if fs::rename(entry.path(), target).is_ok() {
                    removed += 1;
                }
            }
        }
    }
    removed
}

/// Removes everything below `run_root` (`<vault>/home/run/`): run folders of runs that did not
/// end cleanly. The folder itself stays. Returns how many entries it removed.
pub fn sweep_runs(vault_dir: &Path, run_root: &Path) -> usize {
    if !sweep_path_is_safe(vault_dir, run_root) {
        log::warn!("cli tools: run cleanup skipped because its root is unsafe");
        return 0;
    }
    let Ok(entries) = fs::read_dir(run_root) else {
        return 0;
    };
    entries
        .flatten()
        .map(|entry| usize::from(remove_entry(&entry.path())))
        .sum()
}

/// Refuses to traverse a cleanup root when the vault or any path component below it is a
/// symlink. The sweeps are best-effort: a copied or synced vault may contain links that point at
/// another vault or arbitrary host files, and cleanup must never follow them.
fn sweep_path_is_safe(vault_dir: &Path, target: &Path) -> bool {
    let Ok(relative) = target.strip_prefix(vault_dir) else {
        return false;
    };
    let mut current = vault_dir.to_path_buf();
    for component in std::iter::once(None).chain(relative.components().map(Some)) {
        if let Some(component) = component {
            current.push(component);
        }
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => return false,
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => return true,
            Err(_) => return false,
        }
    }
    true
}

fn remove_entry(path: &Path) -> bool {
    let is_dir = fs::symlink_metadata(path).is_ok_and(|meta| meta.is_dir());
    let result = if is_dir {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    };
    if result.is_err() {
        log::warn!("cli tools: a leftover could not be removed, it stays for the next open");
    }
    result.is_ok()
}

#[cfg(test)]
#[path = "installed_tests.rs"]
mod tests;
