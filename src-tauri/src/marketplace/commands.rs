//! The marketplace commands of the catalog (spec 069, contracts/commands.md): the list of the
//! marketplaces, the common catalog, the categories, the details of an entry and the icons. They
//! are registered in the handler of the main window only, so no agent action, no MCP tool and no
//! extension reaches them (FR-030, checked by `commands_tests.rs`).

use futures::future::Abortable;
use tauri::{AppHandle, State};

use super::catalog::{fetch_catalog, InstalledExtension, Source};
use super::client::{Http, SourceFailure};
use super::details::{fetch_categories, fetch_details};
use super::icons::Network;
use super::model::{
    CatalogCategories, CatalogDetail, CatalogPage, CatalogQuery, MarketplaceAuthType,
    MarketplaceCredentialsState, MarketplaceView, SourceErrorKind,
};
use super::store::{self, MarketplaceRow};
use super::{app_version, kinds, MarketplaceState};
use crate::error::{HolziError, Result};
use crate::extensions::registry::list::list as list_extensions;
use crate::state::AppState;
use crate::state_utils::active_database;
use crate::storage::wm_session_commands::current_device_uuid;
use crate::vault_gate::VaultDb;

fn refused(kind: &str) -> HolziError {
    HolziError::Marketplace {
        reason: kind.to_owned(),
    }
}

/// How a failed call to a source reads as an error of a command that asked only one.
pub fn failure_error(failure: &SourceFailure) -> HolziError {
    refused(match failure.kind {
        SourceErrorKind::Unreachable => kinds::SOURCE_UNREACHABLE,
        SourceErrorKind::Timeout => kinds::SOURCE_TIMEOUT,
        SourceErrorKind::AuthRejected => kinds::SOURCE_AUTH_REJECTED,
        SourceErrorKind::BadResponse => kinds::SOURCE_BAD_RESPONSE,
        SourceErrorKind::CredentialsMissing => kinds::CREDENTIALS_REQUIRED,
        SourceErrorKind::CredentialsSyncing => kinds::CREDENTIALS_SYNCING,
        SourceErrorKind::InsecureCredentials => kinds::INSECURE_CREDENTIALS,
    })
}

/// The list of the marketplaces in the order of the user.
pub fn views(rows: &[MarketplaceRow]) -> Vec<MarketplaceView> {
    rows.iter()
        .zip(1u32..)
        .map(|(row, rank)| MarketplaceView {
            id: row.id.clone(),
            name: row.name.clone(),
            base_url: row.base_url.clone(),
            enabled: row.enabled,
            rank,
            auth_type: row.auth_type,
            credentials: credentials_state(row),
        })
        .collect()
}

/// Until the credentials entries are wired in (spec 069 US5) a marketplace that asks for a login
/// reads as missing its credentials.
fn credentials_state(row: &MarketplaceRow) -> MarketplaceCredentialsState {
    match row.auth_type {
        MarketplaceAuthType::None => MarketplaceCredentialsState::None,
        _ => MarketplaceCredentialsState::Missing,
    }
}

/// The sources to ask: the enabled marketplaces in the order of the list. A disabled one is not
/// asked at all (US4-3).
pub fn sources_of(rows: &[MarketplaceRow]) -> Vec<Source> {
    rows.iter()
        .filter(|row| row.enabled)
        .map(source_of)
        .collect()
}

/// One marketplace as a source; one with a login cannot be asked yet.
pub fn source_of(row: &MarketplaceRow) -> Source {
    Source {
        id: row.id.clone(),
        name: row.name.clone(),
        base_url: row.base_url.clone(),
        auth: None,
        unavailable: match credentials_state(row) {
            MarketplaceCredentialsState::None | MarketplaceCredentialsState::Present => None,
            MarketplaceCredentialsState::Syncing => Some(SourceErrorKind::CredentialsSyncing),
            MarketplaceCredentialsState::Missing => Some(SourceErrorKind::CredentialsMissing),
        },
    }
}

async fn rows(db: &VaultDb) -> Result<Vec<MarketplaceRow>> {
    db.read(|q| Ok(store::list(q)?)).await
}

async fn installed(app: &AppHandle, db: &VaultDb) -> Result<Vec<InstalledExtension>> {
    let device = current_device_uuid(app, db)?;
    let summaries = db.read(move |q| Ok(list_extensions(q, device)?)).await?;
    Ok(summaries
        .into_iter()
        .map(|summary| InstalledExtension {
            name: summary.name,
            version: summary.version,
            enabled: summary.enabled,
            publisher_fingerprint: summary.publisher_fingerprint,
        })
        .collect())
}

/// Every marketplace in the order of the user (FR-020).
#[tauri::command]
pub async fn marketplace_list(state: State<'_, AppState>) -> Result<Vec<MarketplaceView>> {
    let db = active_database(&state)?;
    Ok(views(&rows(&db).await?))
}

/// One page of the common catalog (FR-001, FR-012). A newer call aborts a running one.
#[tauri::command]
pub async fn marketplace_catalog(
    app: AppHandle,
    state: State<'_, AppState>,
    market: State<'_, MarketplaceState>,
    query: CatalogQuery,
) -> Result<CatalogPage> {
    let db = active_database(&state)?;
    let registration = market.begin_catalog();
    let work = async {
        let sources = sources_of(&rows(&db).await?);
        let installed = installed(&app, &db).await?;
        Ok(fetch_catalog(
            Http::default(),
            &sources,
            &query,
            &installed,
            &app_version(),
        )
        .await)
    };
    match Abortable::new(work, registration).await {
        Ok(result) => result,
        Err(_) => Err(refused(kinds::SUPERSEDED)),
    }
}

/// The categories of the enabled marketplaces, or of one.
#[tauri::command]
pub async fn marketplace_categories(
    state: State<'_, AppState>,
    source_id: Option<String>,
) -> Result<CatalogCategories> {
    let db = active_database(&state)?;
    let sources = sources_of(&rows(&db).await?);
    Ok(fetch_categories(Http::default(), &sources, source_id.as_deref()).await)
}

/// The details of an entry at its marketplace.
#[tauri::command]
pub async fn marketplace_details(
    app: AppHandle,
    state: State<'_, AppState>,
    source_id: String,
    slug: String,
) -> Result<CatalogDetail> {
    let db = active_database(&state)?;
    let rows = rows(&db).await?;
    let row = rows
        .iter()
        .find(|row| row.id == source_id)
        .ok_or_else(|| refused(kinds::NOT_FOUND))?;
    let installed = installed(&app, &db).await?;
    fetch_details(
        Http::default(),
        &source_of(row),
        &slug,
        &installed,
        &app_version(),
    )
    .await
    .map_err(|failure| failure_error(&failure))
}

/// An icon as a `data:` URL, loaded by the host (FR-006, research R12).
#[tauri::command]
pub async fn marketplace_icon(
    state: State<'_, AppState>,
    market: State<'_, MarketplaceState>,
    source_id: String,
    url: String,
) -> Result<String> {
    let db = active_database(&state)?;
    let rows = rows(&db).await?;
    let row = rows
        .iter()
        .find(|row| row.id == source_id)
        .ok_or_else(|| refused(kinds::NOT_FOUND))?;
    market
        .icons
        .load(&Network::system(), &row.base_url, &url)
        .await
}

#[cfg(test)]
#[path = "commands_tests.rs"]
mod tests;
