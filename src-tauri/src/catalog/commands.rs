//! Catalog Tauri commands beyond the plain `list_catalog` listing.
//!
//! `catalog_recommend_tiers` powers the onboarding wizard's three
//! model-suggestion chips (Easy / Sweet / Max). See spec 002
//! `contracts/tauri-commands.md`.

use crate::error::{HaexError, Result};
use crate::hardware;

use super::{recommend_tiers, TierRecommendation};

/// Returns exactly three tier-labelled recommendations against the
/// current hardware, from the presets of this platform (spec 043 FR-027). Errors as `CatalogEntryNotFound` when the built-in
/// catalog is empty; in production this only happens if the compiled
/// JSON blob is malformed.
#[tauri::command]
pub async fn catalog_recommend_tiers() -> Result<[TierRecommendation; 3]> {
    let hw = hardware::probe_async().await;
    recommend_tiers(&hw, crate::platform::capabilities().model_presets).ok_or_else(|| {
        HaexError::CatalogEntryNotFound {
            id: "<empty catalog>".to_string(),
        }
    })
}
