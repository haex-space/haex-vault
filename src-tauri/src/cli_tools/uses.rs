//! Which installed versions a run uses right now (spec 094 FR-010, FR-022).
//!
//! A run holds a [`Lease`] on the version it selected at its start, for as long as the process
//! can run. The lease keeps that version's folder from being deleted when a newer version is
//! confirmed, and it is what a revert or a removal checks before it touches the tool. The
//! registry lives in memory and is shared by everything in the app process: the tools of a vault
//! are rebuilt for each command, so the count cannot live in them.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use super::catalog::VendorDefinition;
use super::installed;
use super::platform::PlatformKey;

/// Told which vendor's installed state changed, so open windows refresh its row.
pub type ChangeNotifier = Arc<dyn Fn(&str) + Send + Sync>;

/// vendor folder -> version -> number of leases.
type Counts = HashMap<PathBuf, HashMap<String, usize>>;

fn registry() -> &'static Mutex<Counts> {
    static REGISTRY: OnceLock<Mutex<Counts>> = OnceLock::new();
    REGISTRY.get_or_init(Mutex::default)
}

fn counts() -> std::sync::MutexGuard<'static, Counts> {
    // The state is a map of counters, so a poisoned lock holds nothing half-written.
    registry().lock().unwrap_or_else(|e| e.into_inner())
}

/// Whether a run uses `version` of the tool in `vendor_dir`.
pub fn version_in_use(vendor_dir: &Path, version: &str) -> bool {
    counts()
        .get(vendor_dir)
        .is_some_and(|versions| versions.get(version).is_some_and(|count| *count > 0))
}

/// Whether a run uses any version of the tool in `vendor_dir`.
pub fn vendor_in_use(vendor_dir: &Path) -> bool {
    counts()
        .get(vendor_dir)
        .is_some_and(|versions| versions.values().any(|count| *count > 0))
}

/// One run's claim on one installed version. Dropping the last claim on a version that a newer,
/// confirmed one has replaced removes that version's folder.
pub struct Lease {
    vendor_dir: PathBuf,
    version: String,
    vendor: Arc<VendorDefinition>,
    host: Option<PlatformKey>,
    changed: Option<ChangeNotifier>,
}

impl Lease {
    /// Takes a claim on `version` of the tool in `vendor_dir`.
    pub fn take(
        vendor_dir: PathBuf,
        version: String,
        vendor: Arc<VendorDefinition>,
        host: Option<PlatformKey>,
        changed: Option<ChangeNotifier>,
    ) -> Self {
        *counts()
            .entry(vendor_dir.clone())
            .or_default()
            .entry(version.clone())
            .or_insert(0) += 1;
        Self {
            vendor_dir,
            version,
            vendor,
            host,
            changed,
        }
    }

    /// The version this run selected.
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Records that a run with this version ended normally and, now that it is proven, removes
    /// the older versions no run uses. A version that was confirmed before costs one `stat`.
    pub fn confirm(&self) -> io::Result<()> {
        if installed::confirm(&self.vendor_dir, &self.version)? {
            prune(&self.vendor_dir, &self.vendor, self.host);
            self.notify();
        }
        Ok(())
    }

    fn notify(&self) {
        if let Some(changed) = &self.changed {
            changed(&self.vendor.vendor);
        }
    }

    /// Whether a folder of a newer version lies next to this one: this lease may be the last
    /// claim on a fallback that is waiting for deletion. A directory listing, no more.
    fn newer_version_exists(&self) -> bool {
        let Ok(mine) = semver::Version::parse(&self.version) else {
            return false;
        };
        std::fs::read_dir(&self.vendor_dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| entry.file_name().into_string().ok())
            .filter_map(|name| semver::Version::parse(&name).ok())
            .any(|version| version > mine)
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        let last = {
            let mut counts = counts();
            let Some(versions) = counts.get_mut(&self.vendor_dir) else {
                return;
            };
            let count = versions.entry(self.version.clone()).or_insert(1);
            *count = count.saturating_sub(1);
            let last = *count == 0;
            if last {
                versions.remove(&self.version);
            }
            if versions.is_empty() {
                counts.remove(&self.vendor_dir);
            }
            last
        };
        if last
            && self.newer_version_exists()
            && prune(&self.vendor_dir, &self.vendor, self.host) > 0
        {
            self.notify();
        }
    }
}

/// Removes what [`installed::prune_older`] allows, counting the leases of the registry.
pub fn prune(vendor_dir: &Path, vendor: &VendorDefinition, host: Option<PlatformKey>) -> usize {
    installed::prune_older(vendor_dir, vendor, host, &|version| {
        version_in_use(vendor_dir, version)
    })
}

#[cfg(test)]
#[path = "uses_tests.rs"]
mod tests;
