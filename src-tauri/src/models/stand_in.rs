//! Stand-in models for the e2e suite (spec 043 T079): catalog entries beside the built-in ones and
//! a local server in place of HuggingFace, so a scenario sees what a download asks for without
//! loading a real model. Only debug builds take them; a release build refuses the command.

use std::sync::RwLock;

use crate::catalog::CatalogEntry;
use crate::error::{HaexError, Result};

struct StandIn {
    base_url: String,
    entries: &'static [CatalogEntry],
}

static STAND_IN: RwLock<Option<StandIn>> = RwLock::new(None);

/// Puts `entries` beside the catalog and sends HuggingFace requests to `base_url`, a server on this
/// device (`http://127.0.0.1:<port>`). Debug builds only.
#[tauri::command]
pub fn e2e_stand_in_models(base_url: String, entries: Vec<CatalogEntry>) -> Result<()> {
    if !cfg!(debug_assertions) {
        return Err(HaexError::InvalidInput {
            reason: "stand-in models exist only in debug builds".to_string(),
        });
    }
    let url = url::Url::parse(&base_url).map_err(|e| HaexError::InvalidInput {
        reason: format!("invalid stand-in address: {e}"),
    })?;
    if !matches!(url.host_str(), Some("127.0.0.1" | "localhost")) {
        return Err(HaexError::InvalidInput {
            reason: "a stand-in server runs on this device".to_string(),
        });
    }
    // The catalog hands out `&'static` entries; a scenario sets its stand-ins once, so the few
    // bytes a call leaks do not add up.
    let entries: &'static [CatalogEntry] = Box::leak(entries.into_boxed_slice());
    *STAND_IN.write().unwrap_or_else(|e| e.into_inner()) = Some(StandIn { base_url, entries });
    Ok(())
}

/// The stand-in catalog entries; none outside an e2e run.
pub fn entries() -> &'static [CatalogEntry] {
    STAND_IN
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .map_or(&[], |stand_in| stand_in.entries)
}

/// Where HuggingFace requests go: the stand-in server in an e2e run, HuggingFace otherwise.
pub fn hf_base_url() -> String {
    STAND_IN
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .map_or_else(
            || super::huggingface::DEFAULT_BASE_URL.to_string(),
            |stand_in| stand_in.base_url.clone(),
        )
}
