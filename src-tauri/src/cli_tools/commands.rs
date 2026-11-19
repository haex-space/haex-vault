//! The Tauri commands of the tool manager (spec 094, `contracts/tauri-commands.md`).
//!
//! All of them work on the vault that is open: the vault gate supplies it, and no command takes a
//! path, a URL or a checksum from the frontend (they come from the catalog). They are unavailable
//! where the device has no CLI delegates (Android, iOS) with the same reason the delegates give.

use std::sync::Arc;

use semver::Version;
use tauri::{AppHandle, Emitter, State};

use super::catalog::{Catalog, VendorDefinition};
use super::install::{self, InstallContext, InstallError, Phase, Progress};
use super::installed::{self, Resolution};
use super::platform::PlatformKey;
use super::runtime::DelegateTools;
use super::status::{self, CliToolChanged, CliToolPhase, CliToolProgress, CliToolStatus};
use super::uses;
use crate::error::{HaexError, Result};
use crate::models::cancel::DownloadRegistry;
use crate::state::AppState;

pub const EVENT_PROGRESS: &str = "cli-tool-progress";
pub const EVENT_CHANGED: &str = "cli-tool-changed";

/// The key an install of `vendor` registers under in the registry of running transfers.
fn registry_key(vendor: &str) -> String {
    format!("cli-tool:{vendor}")
}

fn ensure_available() -> Result<()> {
    if crate::platform::capabilities().cli_delegates {
        Ok(())
    } else {
        Err(HaexError::InvalidInput {
            reason: "backend unavailable: platform: CLI delegates are not available on this device"
                .into(),
        })
    }
}

fn failed(error: InstallError) -> HaexError {
    HaexError::InvalidInput {
        reason: error.to_string(),
    }
}

/// The status of every vendor of the catalog, in catalog order.
pub(crate) fn status_of_all(
    tools: &DelegateTools,
    running: &DownloadRegistry,
) -> Vec<CliToolStatus> {
    Catalog::active()
        .vendors
        .iter()
        .map(|vendor| status_of(tools, running, &vendor.vendor, vendor))
        .collect()
}

fn status_of(
    tools: &DelegateTools,
    running: &DownloadRegistry,
    vendor_id: &str,
    vendor: &super::catalog::VendorDefinition,
) -> CliToolStatus {
    status::derive(
        vendor,
        &tools.paths().cli_tool_dir(tools.vault(), vendor_id),
        PlatformKey::host(),
        running.is_running(&registry_key(vendor_id)),
        tools.containment().level(),
    )
}

/// What an install request comes to once it is checked against the catalog.
pub(crate) enum Plan {
    /// The requested version is the one in use; nothing to do.
    AlreadyInstalled,
    Install(Box<InstallContext>),
}

/// Checks `vendor` and `version` against the catalog and this device and builds the context of the
/// install. The newest listed version that has a program for this device is the default; a
/// version that is given must be on the list.
pub(crate) fn plan_install(
    tools: &DelegateTools,
    vendor_id: &str,
    version: Option<&str>,
) -> std::result::Result<Plan, InstallError> {
    let vendor = Catalog::active()
        .vendor(vendor_id)
        .ok_or(InstallError::UnknownVendor)?;
    plan_for(tools, vendor, version)
}

/// [`plan_install`] for a vendor definition at hand (a test lists several versions).
pub(crate) fn plan_for(
    tools: &DelegateTools,
    vendor: &VendorDefinition,
    version: Option<&str>,
) -> std::result::Result<Plan, InstallError> {
    let vendor_id = vendor.vendor.as_str();
    let platform = PlatformKey::host().ok_or(InstallError::UnsupportedPlatform)?;
    let entry = match version {
        Some(wanted) => vendor
            .versions
            .iter()
            .find(|entry| entry.version == wanted)
            .ok_or(InstallError::UnknownVersion)?,
        None => vendor
            .newest_for(platform)
            .ok_or(InstallError::UnsupportedPlatform)?,
    };
    let artifact = entry
        .platforms
        .get(platform.as_str())
        .ok_or(InstallError::UnsupportedPlatform)?
        .clone();

    let vendor_dir = tools.paths().cli_tool_dir(tools.vault(), vendor_id);
    let mut found = installed::scan(&vendor_dir, vendor, Some(platform));
    if let Resolution::Usable(current) = installed::resolve_found(&found) {
        // An install never switches to an older version: that is the revert's job (FR-022).
        let wanted = Version::parse(&entry.version).ok();
        let have = Version::parse(&current.version).ok();
        if wanted.is_some() && have.is_some() && wanted <= have {
            return Ok(Plan::AlreadyInstalled);
        }
        // An update keeps the version in use as the way back until the new one is confirmed, so
        // a second one must wait: the way back would be gone, or unclear (FR-022).
        if !current.confirmed && installed::previous_version(&found).is_some() {
            return Err(InstallError::UnconfirmedUpdate);
        }
        if current.confirmed && installed::older_remains(&found) {
            // A deletion that an earlier run's end or a failure left pending may be doable now.
            uses::prune(&vendor_dir, vendor, Some(platform));
            found = installed::scan(&vendor_dir, vendor, Some(platform));
            if installed::older_remains(&found) {
                return Err(InstallError::VersionInUse);
            }
        }
    }
    Ok(Plan::Install(Box::new(InstallContext {
        vendor: Arc::new(vendor.clone()),
        version: entry.version.clone(),
        artifact,
        platform,
        vendor_dir,
        home: tools.paths().home(tools.vault()),
        run_root: tools.paths().run_root(tools.vault()),
        containment: tools.containment().clone(),
        record: tools.record(),
    })))
}

fn phase_of(phase: Phase) -> CliToolPhase {
    match phase {
        Phase::Downloading => CliToolPhase::Downloading,
        Phase::Verifying => CliToolPhase::Verifying,
        Phase::Unpacking => CliToolPhase::Unpacking,
        Phase::Publishing => CliToolPhase::Publishing,
    }
}

fn emit_changed(app: &AppHandle, vendor: &str) {
    let _ = app.emit(
        EVENT_CHANGED,
        CliToolChanged {
            vendor: vendor.to_string(),
        },
    );
}

/// The tools of the open vault, one entry per vendor of the catalog. Reads the file tree only.
#[tauri::command]
pub async fn cli_tools_status(
    app: AppHandle,
    state: State<'_, AppState>,
    running: State<'_, DownloadRegistry>,
) -> Result<Vec<CliToolStatus>> {
    ensure_available()?;
    let tools = super::tools_of_open_vault(&app, &state)?;
    Ok(status_of_all(&tools, &running))
}

/// Installs the newest listed version of `vendor` for this device into the open vault, or the
/// listed `version` given (tests and the quickstart; the UI never sends one). Used for the first
/// install and, with US3, for an update.
#[tauri::command]
pub async fn cli_tools_install(
    app: AppHandle,
    state: State<'_, AppState>,
    running: State<'_, DownloadRegistry>,
    vendor: String,
    version: Option<String>,
) -> Result<CliToolStatus> {
    ensure_available()?;
    let tools = super::tools_of_open_vault(&app, &state)?;
    let definition = Catalog::active()
        .vendor(&vendor)
        .ok_or_else(|| failed(InstallError::UnknownVendor))?;

    let context = match plan_install(&tools, &vendor, version.as_deref()).map_err(failed)? {
        Plan::AlreadyInstalled => return Ok(status_of(&tools, &running, &vendor, definition)),
        Plan::Install(context) => context,
    };

    let slot = running
        .try_begin(&registry_key(&vendor))
        .ok_or_else(|| failed(InstallError::Locked))?;
    emit_changed(&app, &vendor);
    let progress_app = app.clone();
    let progress_vendor = vendor.clone();
    let outcome = state
        .gate()
        .run(install::install(
            &context,
            slot.token(),
            move |p: Progress| {
                let _ = progress_app.emit(
                    EVENT_PROGRESS,
                    CliToolProgress {
                        vendor: progress_vendor.clone(),
                        phase: phase_of(p.phase),
                        bytes_done: p.bytes_done,
                        bytes_total: p.bytes_total,
                    },
                );
            },
        ))
        .await;
    drop(slot);
    emit_changed(&app, &vendor);

    outcome?.map_err(failed)?;
    Ok(status_of(&tools, &running, &vendor, definition))
}

/// Goes back to the older version while the newest is unconfirmed (spec FR-022): deletes the
/// newest and leaves the older one in use. Refused while a run of the tool is in progress, and
/// while an install of it runs. holzi never calls this by itself.
pub(crate) fn revert_tool(
    tools: &DelegateTools,
    vendor: &VendorDefinition,
) -> std::result::Result<(), InstallError> {
    let vendor_dir = tools.paths().cli_tool_dir(tools.vault(), &vendor.vendor);
    if uses::vendor_in_use(&vendor_dir) {
        return Err(InstallError::InUse);
    }
    installed::revert(&vendor_dir, vendor, PlatformKey::host())
        .map(|_| ())
        .map_err(|error| match error {
            installed::RevertError::NothingToRevert => InstallError::NothingToRevert,
            installed::RevertError::Io => InstallError::Io {
                reason: "remove the new version".into(),
            },
        })
}

/// Goes back to the version before an update that has not been confirmed.
#[tauri::command]
pub async fn cli_tools_revert(
    app: AppHandle,
    state: State<'_, AppState>,
    running: State<'_, DownloadRegistry>,
    vendor: String,
) -> Result<CliToolStatus> {
    ensure_available()?;
    let tools = super::tools_of_open_vault(&app, &state)?;
    let definition = Catalog::active()
        .vendor(&vendor)
        .ok_or_else(|| failed(InstallError::UnknownVendor))?;
    let slot = running
        .try_begin(&registry_key(&vendor))
        .ok_or_else(|| failed(InstallError::Locked))?;
    let reverted = {
        let tools = tools.clone();
        // Deleting a version touches many files.
        tokio::task::spawn_blocking(move || revert_tool(&tools, definition))
            .await
            .map_err(|error| HaexError::InvalidInput {
                reason: format!("cli_tools.io_error: {error}"),
            })?
    };
    drop(slot);
    reverted.map_err(failed)?;
    emit_changed(&app, &vendor);
    Ok(status_of(&tools, &running, &vendor, definition))
}

/// Removes every installed version of `vendor` in the vault (spec 094 US4, FR-014): the vendor's
/// folder goes, the vault's home and the other vendors stay. Refused while a run of the tool is in
/// progress. A vendor that has nothing installed is already removed.
pub(crate) fn remove_tool(
    tools: &DelegateTools,
    vendor: &VendorDefinition,
) -> std::result::Result<(), InstallError> {
    let vendor_dir = tools.paths().cli_tool_dir(tools.vault(), &vendor.vendor);
    if uses::vendor_in_use(&vendor_dir) {
        return Err(InstallError::InUse);
    }
    match std::fs::remove_dir_all(&vendor_dir) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(InstallError::Io {
            reason: "remove the tool".into(),
        }),
    }
}

/// Removes the tool of `vendor` from the open vault. The stored connection is the caller's to
/// remove (the settings page does it with the tool).
#[tauri::command]
pub async fn cli_tools_remove(
    app: AppHandle,
    state: State<'_, AppState>,
    running: State<'_, DownloadRegistry>,
    vendor: String,
) -> Result<CliToolStatus> {
    ensure_available()?;
    let tools = super::tools_of_open_vault(&app, &state)?;
    let definition = Catalog::active()
        .vendor(&vendor)
        .ok_or_else(|| failed(InstallError::UnknownVendor))?;
    // An install and a removal of one vendor never overlap.
    let slot = running
        .try_begin(&registry_key(&vendor))
        .ok_or_else(|| failed(InstallError::Locked))?;
    let removed = {
        let tools = tools.clone();
        // Deleting a tool touches many files.
        tokio::task::spawn_blocking(move || remove_tool(&tools, definition))
            .await
            .map_err(|error| HaexError::InvalidInput {
                reason: format!("cli_tools.io_error: {error}"),
            })?
    };
    drop(slot);
    emit_changed(&app, &vendor);
    removed.map_err(failed)?;
    Ok(status_of(&tools, &running, &vendor, definition))
}

/// Cancels a running install of `vendor` in the open vault. Idempotent.
#[tauri::command]
pub fn cli_tools_cancel(running: State<'_, DownloadRegistry>, vendor: String) {
    running.cancel(&registry_key(&vendor));
}

#[cfg(test)]
#[path = "commands_tests.rs"]
mod tests;
