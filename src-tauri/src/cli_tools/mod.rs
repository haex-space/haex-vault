//! CLI tools per vault (spec 094).
//!
//! The delegate backends (Claude Code, Codex; later the Abacus AI CLI) run a command-line tool.
//! holzi does not take that tool from the host: every vault has its own installation inside its
//! folder, `<vault>/cli-tools/<vendor>/<version>/`, and its own home folder, `<vault>/home/`,
//! which every delegate process gets as `HOME`. The user installs, updates and removes a tool in
//! Settings; holzi never does it silently, and only installs versions it has tested (the
//! [`catalog`], with a SHA-256 per download).
//!
//! This module owns the data and the logic: [`catalog`] (what can be installed), [`platform`]
//! (which artifact fits this device), [`installed`] (what a vault has, read from the file tree),
//! [`containment`] and [`runtime`] (how a delegate process is started inside the vault),
//! [`install`] and [`unpack`] (download, check, unpack and publish one version),
//! [`status`] and [`commands`] (what Settings shows and does), and
//! [`record`] (what happened, kept in the vault's folder). Paths come from
//! `instances::vault_paths::VaultPaths`, the one place that builds them.
//!
//! What stays outside: the delegate adapters (`adapters::cli_delegate`) only receive a
//! [`runtime::DelegateRuntime`]. Encryption of the vault's subfolders and an OS sandbox are separate
//! specs that plug into [`containment::Containment`] and the folder layout.

pub mod catalog;
pub mod commands;
pub mod containment;
pub mod install;
pub mod installed;
pub mod platform;
pub mod record;
pub mod runtime;
pub mod stand_in;
pub mod status;
pub mod unpack;
pub mod uses;

use std::sync::Arc;

use tauri::{AppHandle, Emitter, Runtime};

use crate::error::{HaexError, Result};
use crate::instances::paths::vault_paths;
use crate::instances::vault_paths::VaultPaths;
use crate::state::AppState;

/// The tools of the vault that is open now. Everything that starts or manages a tool takes the
/// vault from here, never from the caller: a window cannot name another vault's folder.
pub fn tools_of_open_vault<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
) -> Result<runtime::DelegateTools> {
    let name = state.active_name()?.ok_or(HaexError::NoActiveInstance)?;
    let handle = app.clone();
    // A confirmation changes what the row of the tool shows, in every window of the vault.
    Ok(
        runtime::DelegateTools::new(vault_paths(app)?, name).with_notifier(Arc::new(
            move |vendor: &str| {
                let _ = handle.emit(
                    commands::EVENT_CHANGED,
                    status::CliToolChanged {
                        vendor: vendor.to_string(),
                    },
                );
            },
        )),
    )
}

/// What `open_sweep` removed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SweepReport {
    /// Staging folders of install attempts that a kill or a crash interrupted.
    pub staging: usize,
    /// Run folders of delegate runs that did not end cleanly.
    pub runs: usize,
    /// Older versions of a tool whose deletion an earlier session could not finish (a run still
    /// used them, or the deletion failed); the newer version is confirmed.
    pub pruned: usize,
}

/// Clears what an earlier session left behind in the vault `name`: interrupted install attempts,
/// run folders and the older versions of a confirmed update that were not deleted yet. Called once where a vault opens (`instances::open` and `instances::import`),
/// before any delegate can start, so nothing live is removed. Failures are logged without a
/// path and leave the leftover for the next open.
pub fn open_sweep(paths: &VaultPaths, name: &str) -> SweepReport {
    let staging = installed::sweep_staging(&paths.folder(name), &paths.cli_tools(name));
    let runs = installed::sweep_runs(&paths.folder(name), &paths.run_root(name));
    // The staging sweep has restored any displaced version, so what is left is a settled state.
    let pruned = catalog::Catalog::active()
        .vendors
        .iter()
        .map(|vendor| {
            uses::prune(
                &paths.cli_tool_dir(name, &vendor.vendor),
                vendor,
                platform::PlatformKey::host(),
            )
        })
        .sum();
    SweepReport {
        staging,
        runs,
        pruned,
    }
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
