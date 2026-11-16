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
    /// Unix seconds.
    pub installed_at: u64,
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
    if host != Some(installed_for) {
        return unusable(Misfit::Platform);
    }
    let Some(relative) = vendor.executable_for(installed_for) else {
        return unusable(Misfit::Incomplete);
    };
    let executable = dir.join(relative);
    let sized = fs::metadata(&executable)
        .is_ok_and(|meta| meta.is_file() && meta.len() == marker.executable_size);
    if !sized {
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

/// What a vault has of the vendor's tool: the highest usable version, or the reason there is
/// none.
pub fn resolve(
    vendor_dir: &Path,
    vendor: &VendorDefinition,
    host: Option<PlatformKey>,
) -> Resolution {
    let found = scan(vendor_dir, vendor, host);
    let mut first_misfit = None;
    for entry in found {
        match entry {
            Found::Usable(installation) => return Resolution::Usable(installation),
            Found::Unusable { why, .. } => {
                first_misfit.get_or_insert(why);
            }
        }
    }
    first_misfit.map_or(Resolution::NotInstalled, Resolution::NeedsReinstall)
}

/// Removes the staging folders of every vendor below `cli_tools_dir` (`<vault>/cli-tools/`):
/// attempts that a kill or a crash interrupted. Returns how many it removed. A failure leaves
/// the folder for the next vault open; the log line carries no path (spec FR-019).
pub fn sweep_staging(cli_tools_dir: &Path) -> usize {
    let Ok(vendors) = fs::read_dir(cli_tools_dir) else {
        return 0;
    };
    let mut removed = 0;
    for vendor in vendors.flatten() {
        let Ok(entries) = fs::read_dir(vendor.path()) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with('.') && name.ends_with(STAGING_SUFFIX) {
                removed += usize::from(remove_entry(&entry.path()));
            }
        }
    }
    removed
}

/// Removes everything below `run_root` (`<vault>/home/run/`): run folders of runs that did not
/// end cleanly. The folder itself stays. Returns how many entries it removed.
pub fn sweep_runs(run_root: &Path) -> usize {
    let Ok(entries) = fs::read_dir(run_root) else {
        return 0;
    };
    entries
        .flatten()
        .map(|entry| usize::from(remove_entry(&entry.path())))
        .sum()
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
