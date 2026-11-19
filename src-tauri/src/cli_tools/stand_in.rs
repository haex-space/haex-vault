//! A stand-in catalog for the e2e suite (spec 094, T036): a catalog whose downloads come from a
//! server on this device, so a scenario installs a tool without the vendor's server and without
//! the real checksums. Only debug builds take it; a release build refuses the command, and the
//! catalog that ships is never replaced by anything a window sends in one.

use std::sync::RwLock;

use super::catalog::Catalog;
use crate::error::{HaexError, Result};

static STAND_IN: RwLock<Option<&'static Catalog>> = RwLock::new(None);

/// Puts `catalog_json` in place of the shipped catalog for this run. Its downloads may name
/// `http://127.0.0.1:<port>` or `http://localhost:<port>`. Debug builds only.
#[tauri::command]
pub fn e2e_stand_in_cli_tools(catalog_json: String) -> Result<()> {
    install(&catalog_json, cfg!(debug_assertions))
}

fn install(catalog_json: &str, allowed: bool) -> Result<()> {
    if !allowed {
        return Err(HaexError::InvalidInput {
            reason: "a stand-in catalog exists only in debug builds".to_string(),
        });
    }
    let catalog =
        Catalog::parse_loopback(catalog_json).map_err(|error| HaexError::InvalidInput {
            reason: error.to_string(),
        })?;
    // A scenario sets its stand-in once, so the few bytes a call leaks do not add up.
    *STAND_IN.write().unwrap_or_else(|e| e.into_inner()) = Some(Box::leak(Box::new(catalog)));
    Ok(())
}

/// Counts a run with the version of `vendor` that is in use as ended normally, as a request
/// that reached its regular end does (spec 094 FR-022). An e2e run has no model account to send
/// a request with, so this takes the place of one: it goes the way a run's end goes (the claim
/// on the version, the confirmation, the deletion of older versions, the event). Debug builds only.
#[tauri::command]
pub fn e2e_confirm_cli_tool(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::state::AppState>,
    vendor: String,
) -> Result<()> {
    if !cfg!(debug_assertions) {
        return Err(HaexError::InvalidInput {
            reason: "a confirmation by hand exists only in debug builds".to_string(),
        });
    }
    let tools = super::tools_of_open_vault(&app, &state)?;
    let runtime = tools
        .runtime(&vendor)
        .map_err(|error| HaexError::InvalidInput {
            reason: error.reason(),
        })?;
    runtime.confirm().map_err(|error| HaexError::InvalidInput {
        reason: format!("cli_tools.io_error: {error}"),
    })
}

/// The stand-in catalog; none outside an e2e run.
pub fn active() -> Option<&'static Catalog> {
    *STAND_IN.read().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
#[path = "stand_in_tests.rs"]
mod tests;
