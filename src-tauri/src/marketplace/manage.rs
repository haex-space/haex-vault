//! Managing the marketplaces of a vault (spec 069 US4, FR-020 to FR-024): the commands behind
//! Settings → Erweiterungen → Marketplaces and the empty state of the catalog. Every row is equal
//! — editing, switching off, reordering and deleting work the same for the preset — and nothing
//! here creates a row by itself: only a saved form and `marketplace_add_preset` do.

use tauri::State;

use super::catalog::Source;
use super::client::Http;
use super::commands::{list_views, refused, source_of};
use super::credentials;
use super::model::{
    MarketplaceAuthType, MarketplaceInput, MarketplaceTestResult, MarketplaceView, SourceErrorKind,
};
use super::store::{self, normalize_base_url, MarketplaceRow};
use super::wire::{WireHealth, WireListResponse};
use crate::error::Result;
use crate::passwords::clock::unix_millis;
use crate::passwords::commands::service;
use crate::passwords::service::PasswordsService;
use crate::remote_storage::model::CredentialsCandidate;
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

/// The marketplace of a form as a source, before it is saved. A login typed in the form is used
/// as it is (nothing is created for a test); a chosen entry is read from the password manager.
async fn source_of_input(
    passwords: &PasswordsService,
    db: &VaultDb,
    input: &MarketplaceInput,
) -> Result<Source> {
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
    let Some(typed) = input
        .new_credentials
        .as_ref()
        .filter(|_| input.auth_type.needs_credentials())
    else {
        return source_of(passwords, db, &row).await;
    };
    let auth = credentials::header_of_typed(typed, input.auth_type);
    let unavailable = if !store::login_transport_ok(&row.base_url) {
        Some(SourceErrorKind::InsecureCredentials)
    } else if auth.is_none() {
        Some(SourceErrorKind::CredentialsMissing)
    } else {
        None
    };
    Ok(Source {
        id: row.id,
        name: row.name,
        base_url: row.base_url,
        auth: auth.filter(|_| unavailable.is_none()),
        unavailable,
    })
}

/// [`test_marketplace`] with the values of the form.
pub async fn test_input(
    db: &VaultDb,
    passwords: &PasswordsService,
    input: &MarketplaceInput,
) -> Result<MarketplaceTestResult> {
    let source = source_of_input(passwords, db, input).await?;
    Ok(test_marketplace(Http::default(), &source).await)
}

/// The view of the row `id` after a change.
async fn view_of(db: &VaultDb, id: &str) -> Result<MarketplaceView> {
    list_views(db)
        .await?
        .into_iter()
        .find(|view| view.id == id)
        .ok_or_else(|| refused(super::kinds::NOT_FOUND))
}

/// Deletes the entry a marketplace pointed at when no other marketplace uses it and the form
/// created it; an entry of the user stays (FR-029). A failure is logged: the row is already saved.
async fn release(passwords: &PasswordsService, db: &VaultDb, item_id: Option<String>) {
    let Some(item_id) = item_id else { return };
    let used = {
        let id = item_id.clone();
        db.read(move |q| store::credentials_in_use(q, &id).map_err(Into::into))
            .await
    };
    match used {
        Ok(false) => {
            if let Err(error) = credentials::delete(passwords, &item_id).await {
                log::warn!("marketplace: an unused login entry was not deleted: {error}");
            }
        }
        Ok(true) => {}
        Err(error) => log::warn!("marketplace: could not check the login entry: {error}"),
    }
}

/// Creates a marketplace (no `id`) or changes one (FR-021, FR-024): the address is normalized
/// and checked, a login needs `https` and an entry of the password manager (FR-027). A login
/// typed in the form becomes a new entry first; the entry a changed row no longer uses goes with
/// it when the form created it.
pub async fn save(
    db: &VaultDb,
    passwords: &PasswordsService,
    mut input: MarketplaceInput,
    at: i64,
) -> Result<MarketplaceView> {
    let typed = input
        .new_credentials
        .take()
        .filter(|_| input.auth_type.needs_credentials());
    let checked = store::check_with(&input, typed.is_some())?;
    let previous = match input.id.clone() {
        Some(id) => db.read(move |q| Ok(store::get(q, &id)?)).await?,
        None => None,
    }
    .and_then(|row| row.credentials_item_id);
    let created = match &typed {
        Some(typed) => Some(
            credentials::create(
                passwords,
                &checked.name,
                &checked.base_url,
                checked.auth_type,
                typed,
            )
            .await?,
        ),
        None => None,
    };
    if created.is_some() {
        input.credentials_item_id = created.clone();
    }
    let stored = input.clone();
    let id = match db
        .write(move |tx| store::save(tx, &stored, at).map_err(Into::into))
        .await
    {
        Ok(id) => id,
        Err(error) => {
            if let Some(item_id) = created {
                let _ = credentials::delete(passwords, &item_id).await;
            }
            return Err(error);
        }
    };
    if created.is_none() {
        if let Some(item_id) = &checked.credentials_item_id {
            // The tag is a convenience: a failure is logged and the saved row stands.
            if let Err(error) = credentials::tag_selected(passwords, item_id).await {
                log::warn!("marketplace: the chosen login entry was not tagged: {error}");
            }
        }
    }
    if previous != created.clone().or(checked.credentials_item_id) {
        release(passwords, db, previous).await;
    }
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
/// extensions stay; the login entry goes with it when the form created it and no other row uses
/// it.
pub async fn delete(db: &VaultDb, passwords: &PasswordsService, id: String) -> Result<()> {
    let row_id = id.clone();
    let item_id = db
        .read(move |q| Ok(store::get(q, &row_id)?))
        .await?
        .and_then(|row| row.credentials_item_id);
    db.write(move |tx| store::delete(tx, &id).map_err(Into::into))
        .await?;
    release(passwords, db, item_id).await;
    Ok(())
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
    save(&active_database(&state)?, &service(&state)?, input, now()).await
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
    delete(&active_database(&state)?, &service(&state)?, id).await
}

/// The empty state of the catalog calls this.
#[tauri::command]
pub async fn marketplace_add_preset(state: State<'_, AppState>) -> Result<MarketplaceView> {
    add_preset(&active_database(&state)?, now()).await
}

/// Tries a marketplace with the values of the form: `GET /health` and the first page (FR-021).
/// A login travels only over `https`. The form may still save what failed.
#[tauri::command]
pub async fn marketplace_test(
    state: State<'_, AppState>,
    input: MarketplaceInput,
) -> Result<MarketplaceTestResult> {
    test_input(&active_database(&state)?, &service(&state)?, &input).await
}

/// The entries of the password manager that fit `auth_type`, to choose as the login of a
/// marketplace (research R11); titles, user names and addresses only.
#[tauri::command]
pub async fn marketplace_credentials_candidates(
    state: State<'_, AppState>,
    auth_type: MarketplaceAuthType,
) -> Result<Vec<CredentialsCandidate>> {
    credentials::candidates(&active_database(&state)?, auth_type).await
}

#[cfg(test)]
#[path = "manage_tests.rs"]
mod tests;
