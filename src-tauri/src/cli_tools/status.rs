//! What the Settings page shows of one tool, derived from the catalog and the vault's file tree
//! (spec 094, `data-model.md` "Tool status"). It reads no network and no file outside the vault
//! folder, and a damaged installation is a state, never an error.

use std::path::Path;

use semver::Version;
use serde::Serialize;
use ts_rs::TS;

use super::catalog::{ArtifactKind, VendorDefinition};
use super::containment::IsolationLevel;
use super::installed::{self, Misfit, Resolution};
use super::platform::PlatformKey;

/// Where a tool stands in a vault.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum CliToolState {
    /// Nothing installed; the install is offered.
    NotInstalled,
    /// A complete installation that fits this device, and no newer version is listed.
    Installed,
    /// A complete installation that fits this device, and a newer version is listed.
    UpdateAvailable,
    /// An install of this vendor is running in this vault.
    Installing,
    /// An installation exists but is for another device or damaged; installing again is the way out.
    NeedsReinstall,
    /// No program of this vendor is listed for this device; nothing is offered.
    Unsupported,
    /// The vendor is in the catalog but has no tested version yet; nothing is offered (spec 096).
    NoTestedVersion,
}

/// One vendor's tool in the open vault.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct CliToolStatus {
    pub vendor: String,
    pub display_name: String,
    pub state: CliToolState,
    /// The version in use, when there is one.
    pub installed_version: Option<String>,
    /// The newest listed version that has a program for this device.
    pub latest_listed_version: Option<String>,
    /// Whether a run with the version in use has ended normally (spec FR-022).
    pub confirmed: bool,
    /// While the version in use is unconfirmed and an older complete one is still on disk: that
    /// older version, which "go back" returns to.
    pub previous_version: Option<String>,
    /// The version in use is not on the list and newer than it: a vault from a newer haex-vault. It
    /// is still started.
    pub untested: bool,
    /// The version in use is older than the oldest listed one; the update is offered.
    pub outdated: bool,
    /// The size of the download an install would fetch.
    #[ts(type = "number | null")]
    pub download_bytes: Option<u64>,
    /// The space an install takes in the vault.
    #[ts(type = "number | null")]
    pub unpacked_bytes: Option<u64>,
    pub isolation: IsolationLevel,
    /// For `needs_reinstall`: `platform` or `incomplete`.
    pub reason: Option<String>,
}

/// Which phase an install is in, as the progress event reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum CliToolPhase {
    Downloading,
    Verifying,
    Unpacking,
    Publishing,
}

/// The payload of the `cli-tool-progress` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct CliToolProgress {
    pub vendor: String,
    pub phase: CliToolPhase,
    #[ts(type = "number")]
    pub bytes_done: u64,
    /// 0 outside the download phase.
    #[ts(type = "number")]
    pub bytes_total: u64,
}

/// The payload of the `cli-tool-changed` event: the vendor whose status is no longer what a
/// window last read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct CliToolChanged {
    pub vendor: String,
}

/// The status of `vendor` whose installations lie in `vendor_dir`. `installing` is whether an
/// install of this vendor runs in this vault right now (held in memory only); `avx2` is whether
/// this device's CPU has AVX2, which decides whether an artifact that needs it is offered.
pub fn derive(
    vendor: &VendorDefinition,
    vendor_dir: &Path,
    host: Option<PlatformKey>,
    avx2: bool,
    installing: bool,
    isolation: IsolationLevel,
) -> CliToolStatus {
    let newest = host.and_then(|key| vendor.newest_for(key, avx2));
    let artifact = newest.map(|found| found.artifact);
    let mut status = CliToolStatus {
        vendor: vendor.vendor.clone(),
        display_name: vendor.display_name.clone(),
        state: CliToolState::NotInstalled,
        installed_version: None,
        latest_listed_version: newest.map(|found| found.version.version.clone()),
        confirmed: false,
        previous_version: None,
        untested: false,
        outdated: false,
        download_bytes: artifact.map(|a| a.size),
        unpacked_bytes: artifact.map(|a| match a.kind {
            ArtifactKind::File => a.size,
            ArtifactKind::TarGz | ArtifactKind::Zip => a.unpacked_size.unwrap_or(a.size),
        }),
        isolation,
        reason: None,
    };

    let found = installed::scan(vendor_dir, vendor, host);
    match installed::resolve_found(&found) {
        Resolution::Usable(installation) => {
            status.state = CliToolState::Installed;
            status.confirmed = installation.confirmed;
            status.previous_version = installed::previous_version(&found);
            let current = Version::parse(&installation.version).ok();
            let newest_catalog_version = vendor
                .versions
                .iter()
                .filter_map(|entry| Version::parse(&entry.version).ok())
                .max();
            let oldest = vendor
                .versions
                .iter()
                .filter_map(|entry| Version::parse(&entry.version).ok())
                .min();
            status.outdated =
                matches!((&current, &oldest), (Some(have), Some(oldest)) if have < oldest);
            status.untested = matches!(
                (&current, &newest_catalog_version),
                (Some(have), Some(newest)) if have > newest
            );
            // The update offered is the newest one that has a program for this device.
            let offered = newest.and_then(|found| Version::parse(&found.version.version).ok());
            if matches!((&current, &offered), (Some(have), Some(offered)) if have < offered) {
                status.state = CliToolState::UpdateAvailable;
            }
            status.installed_version = Some(installation.version);
        }
        Resolution::NeedsReinstall(why) => {
            status.state = CliToolState::NeedsReinstall;
            status.reason = Some(
                match why {
                    Misfit::Platform => "platform",
                    Misfit::Incomplete => "incomplete",
                }
                .to_string(),
            );
        }
        Resolution::NotInstalled => {}
    }
    let in_use = matches!(
        status.state,
        CliToolState::Installed | CliToolState::UpdateAvailable
    );
    if !in_use && newest.is_none() {
        status.state = if vendor.versions.is_empty() {
            CliToolState::NoTestedVersion
        } else {
            CliToolState::Unsupported
        };
        status.reason = None;
    }
    if installing {
        status.state = CliToolState::Installing;
    }
    status
}

#[cfg(test)]
#[path = "status_tests.rs"]
mod tests;
