//! A stand-in for the Abacus sign-in service for the e2e suite (spec 096, T024): a scenario points
//! the connect flow at a server on this device, so it signs in without the vendor and without an
//! account. Only debug builds take it, only an address on this device is accepted, and the address
//! that ships is never replaced by anything a window sends in one (the pattern of
//! `cli_tools::stand_in`).

use std::sync::RwLock;

use crate::error::{HaexError, Result};

static STAND_IN: RwLock<Option<String>> = RwLock::new(None);

/// Points the sign-in at `base_url` (`http://127.0.0.1:<port>` or `http://localhost:<port>`) for this
/// run. Debug builds only.
#[tauri::command]
pub fn e2e_stand_in_abacus_signin(base_url: String) -> Result<()> {
    install(&base_url, cfg!(debug_assertions))
}

fn install(base_url: &str, allowed: bool) -> Result<()> {
    if !allowed {
        return Err(HaexError::InvalidInput {
            reason: "a stand-in sign-in service exists only in debug builds".to_string(),
        });
    }
    let url = url::Url::parse(base_url).map_err(|_| HaexError::InvalidInput {
        reason: "the stand-in address is not a URL".to_string(),
    })?;
    let on_this_device = url.scheme() == "http"
        && matches!(url.host_str(), Some("127.0.0.1" | "localhost"))
        && url.port().is_some();
    if !on_this_device {
        return Err(HaexError::InvalidInput {
            reason: "the stand-in address must be http on this device with a port".to_string(),
        });
    }
    *STAND_IN.write().unwrap_or_else(|e| e.into_inner()) =
        Some(base_url.trim_end_matches('/').to_string());
    Ok(())
}

/// The stand-in address; none outside an e2e run.
pub fn active() -> Option<String> {
    STAND_IN.read().unwrap_or_else(|e| e.into_inner()).clone()
}

#[cfg(test)]
#[path = "abacus_stand_in_tests.rs"]
mod tests;
