//! Managing the marketplaces of a vault (spec 069 US4, FR-020 to FR-024): the commands behind
//! Settings → Erweiterungen → Marketplaces and the empty state of the catalog. Every row is equal
//! — editing, switching off, reordering and deleting work the same for the preset — and nothing
//! here creates a row by itself: only a saved form and `marketplace_add_preset` do.

use tauri::State;

use super::catalog::Source;
use super::client::Http;
use super::commands::{rows, source_of, views};
use super::model::{MarketplaceInput, MarketplaceTestResult, MarketplaceView, SourceErrorKind};
use super::store::{self, normalize_base_url, MarketplaceRow};
use super::wire::{WireHealth, WireListResponse};
use crate::error::Result;
use crate::passwords::clock::unix_millis;
use crate::state::AppState;
use crate::state_utils::active_database;
use crate::vault_gate::VaultDb;

fn now() -> i64 {
    unix_millis(std::time::SystemTime::now())
}

/// What the test of a marketplace found: reachable with the number of entries of its first page,
/// or why not. The answer of the marketplace itself never reaches the result (it may echo a
/// login).
pub async fn test_marketplace(http: Http, source: &Source) -> MarketplaceTestResult {
    let failed = |kind: SourceErrorKind, detail: Option<String>| MarketplaceTestResult {
        ok: false,
        count: None,
        error: Some(kind),
        detail,
    };
    if let Some(kind) = source.unavailable {
        return failed(kind, None);
    }
    let auth = source.auth.as_ref();
    if let Err(failure) = http
        .get_json::<WireHealth>(&source.base_url, "/health", &[], auth)
        .await
    {
        return failed(failure.kind, failure.detail);
    }
    let query = [("page", "1".to_owned()), ("limit", "20".to_owned())];
    match http
        .get_json::<WireListResponse>(&source.base_url, "/extensions", &query, auth)
        .await
    {
        Ok(answer) => MarketplaceTestResult {
            ok: true,
            count: Some(answer.pagination.total),
            error: None,
            detail: None,
        },
        Err(failure) => failed(failure.kind, failure.detail),
    }
}

/// The marketplace of a form as a source, before it is saved. One that needs a login cannot be
/// asked yet (the credentials come with spec 069 US5).
fn source_of_input(input: &MarketplaceInput) -> Result<Source> {
    let base_url = normalize_base_url(&input.base_url)?;
    let row = MarketplaceRow {
        id: input.id.clone().unwrap_or_else(|| "new".to_owned()),
        name: input.name.trim().to_owned(),
        base_url,
        enabled: true,
        sort_order: 0,
        auth_type: input.auth_type,
        credentials_item_id: input.credentials_item_id.clone(),
    };
    Ok(source_of(&row))
}

/// The view of the row `id` after a change.
async fn view_of(db: &VaultDb, id: &str) -> Result<MarketplaceView> {
    let all = rows(db).await?;
    views(&all)
        .into_iter()
        .find(|view| view.id == id)
        .ok_or_else(|| super::commands::refused(super::kinds::NOT_FOUND))
}

/// Creates a marketplace (no `id`) or changes one (FR-021, FR-024): the address is normalized
/// and checked, a login needs `https` and an entry of the password manager (FR-027).
pub async fn save(db: &VaultDb, input: MarketplaceInput, at: i64) -> Result<MarketplaceView> {
    let id = db
        .write(move |tx| store::save(tx, &input, at).map_err(Into::into))
        .await?;
    view_of(db, &id).await
}

/// Switches a marketplace on or off; takes effect at once (FR-020).
pub async fn set_enabled(db: &VaultDb, id: String, enabled: bool, at: i64) -> Result<()> {
    db.write(move |tx| store::set_enabled(tx, &id, enabled, at).map_err(Into::into))
        .await
}

/// Writes the order the user chose (`ids` is the complete list).
pub async fn reorder(db: &VaultDb, ids: Vec<String>, at: i64) -> Result<()> {
    db.write(move |tx| store::reorder(tx, &ids, at).map_err(Into::into))
        .await
}

/// Deletes a marketplace, whichever it is; nothing creates it again (FR-023). Installed
/// extensions stay.
pub async fn delete(db: &VaultDb, id: String) -> Result<()> {
    db.write(move |tx| store::delete(tx, &id).map_err(Into::into))
        .await
}

/// Adds the public marketplace if its row is missing; one that exists is left as it is, even
/// switched off (FR-022).
pub async fn add_preset(db: &VaultDb, at: i64) -> Result<MarketplaceView> {
    let row = db
        .write(move |tx| store::add_preset(tx, at).map_err(Into::into))
        .await?;
    view_of(db, &row.id).await
}

#[tauri::command]
pub async fn marketplace_save(
    state: State<'_, AppState>,
    input: MarketplaceInput,
) -> Result<MarketplaceView> {
    save(&active_database(&state)?, input, now()).await
}

#[tauri::command]
pub async fn marketplace_set_enabled(
    state: State<'_, AppState>,
    id: String,
    enabled: bool,
) -> Result<()> {
    set_enabled(&active_database(&state)?, id, enabled, now()).await
}

#[tauri::command]
pub async fn marketplace_reorder(state: State<'_, AppState>, ids: Vec<String>) -> Result<()> {
    reorder(&active_database(&state)?, ids, now()).await
}

#[tauri::command]
pub async fn marketplace_delete(state: State<'_, AppState>, id: String) -> Result<()> {
    delete(&active_database(&state)?, id).await
}

/// The empty state of the catalog calls this.
#[tauri::command]
pub async fn marketplace_add_preset(state: State<'_, AppState>) -> Result<MarketplaceView> {
    add_preset(&active_database(&state)?, now()).await
}

/// Tries a marketplace with the values of the form: `GET /health` and the first page (FR-021).
/// A login travels only over `https`. The form may still save what failed.
#[tauri::command]
pub async fn marketplace_test(input: MarketplaceInput) -> Result<MarketplaceTestResult> {
    let source = source_of_input(&input)?;
    Ok(test_marketplace(Http::default(), &source).await)
}

#[cfg(test)]
#[path = "manage_tests.rs"]
mod tests;
