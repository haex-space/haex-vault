//! Commands of the sync with the phone's address book (spec 051 US2, contracts/tauri-commands.md).
//! Only on a device with a phone address book (`PlatformCapabilities.phone_contacts`); elsewhere
//! each answers `PhoneContactsUnavailable`. A command that talks to the phone runs on a tracked
//! blocking thread and waits for a run of the sync that is under way.

use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};
use ts_rs::TS;

use crate::contacts::phone::android::{ensure_permission, AndroidPhoneBook};
use crate::contacts::phone::state::{self, PhoneStatus, VAULTS_FILE};
use crate::contacts::phone::trigger::{announce, PhoneRuns};
use crate::contacts::phone::{
    review, run, Account, MassDecision, Mode, PairDecision, PhoneBook, ReviewPlan, RunSummary,
};
use crate::error::{HolziError, Result};
use crate::passwords::clock;
use crate::state::AppState;
use crate::state_utils::active_database;
use crate::vault_gate::VaultDb;

fn available() -> Result<()> {
    if crate::platform::capabilities().phone_contacts {
        Ok(())
    } else {
        Err(HolziError::PhoneContactsUnavailable)
    }
}

fn vaults_file(app: &AppHandle) -> Result<PathBuf> {
    let dir = app
        .path()
        .app_local_data_dir()
        .map_err(|error| HolziError::PathResolution {
            reason: format!("app_local_data_dir: {error}"),
        })?;
    Ok(dir.join(VAULTS_FILE))
}

/// The vault's public key as hex, which names it in the list of syncing vaults (FR-027).
async fn vault_name(db: &VaultDb) -> Result<String> {
    let key = db.read(|q| crate::sync::keys::vault_pubkey(q)).await?;
    Ok(key.map_or_else(
        || db.device_id().to_string(),
        |key| data_encoding::HEXLOWER.encode(&key),
    ))
}

/// Runs `work` with the phone's address book on a tracked blocking thread, one run at a time.
async fn with_phone<T: Send + 'static>(
    app: &AppHandle,
    state: &State<'_, AppState>,
    work: impl FnOnce(&VaultDb, &dyn PhoneBook, &str) -> Result<T> + Send + 'static,
) -> Result<T> {
    available()?;
    let db = active_database(state)?;
    let runs = Arc::clone(&app.state::<Arc<PhoneRuns>>());
    let book = AndroidPhoneBook::new(app.clone());
    state
        .gate()
        .spawn_blocking(move || {
            let _held = runs.hold();
            let device = db.device_id().to_string();
            work(&db, &book, &device)
        })?
        .await
        .map_err(|error| HolziError::PhoneContactsFailed {
            reason: error.to_string(),
        })?
}

/// What switching the sync on led to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(
    export,
    export_to = "../../src/types/bindings/",
    rename = "ContactsPhoneEnabled"
)]
#[serde(rename_all = "camelCase")]
pub struct PhoneEnableResult {
    /// The overview of the first run waits.
    pub review: bool,
    /// Another vault already syncs with this phone's address book (FR-027).
    pub other_vault: bool,
}

/// Switches the sync on after asking for the permission (FR-017); a refusal leaves it off and
/// answers `ContactsDenied`.
#[tauri::command]
pub async fn contacts_phone_enable(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<PhoneEnableResult> {
    available()?;
    ensure_permission(&app).await?;
    let db = active_database(&state)?;
    let device = db.device_id().to_string();
    let vault = vault_name(&db).await?;
    let file = vaults_file(&app)?;
    let other_vault = state::other_vault(&file, &vault);
    let status = db
        .write(move |tx| {
            state::enable(tx, &device)?;
            state::status(tx, &device).map_err(Into::into)
        })
        .await?;
    state::mark_vault(&file, &vault, true);
    let _ = tauri::Emitter::emit(&app, crate::contacts::phone::trigger::STATUS_EVENT, &status);
    Ok(PhoneEnableResult {
        review: status.state == state::PhoneState::Review,
        other_vault,
    })
}

/// Switches the sync off; nothing is deleted on either side (FR-026).
#[tauri::command]
pub async fn contacts_phone_disable(app: AppHandle, state: State<'_, AppState>) -> Result<()> {
    available()?;
    let db = active_database(&state)?;
    let device = db.device_id().to_string();
    let vault = vault_name(&db).await?;
    let status = db
        .write(move |tx| {
            state::disable(tx, &device)?;
            state::status(tx, &device).map_err(Into::into)
        })
        .await?;
    state::mark_vault(&vaults_file(&app)?, &vault, false);
    let _ = tauri::Emitter::emit(&app, crate::contacts::phone::trigger::STATUS_EVENT, &status);
    Ok(())
}

/// The accounts of the phone's address book.
#[tauri::command]
pub async fn contacts_phone_accounts(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<Account>> {
    with_phone(&app, &state, |_, book, _| {
        Ok(book.accounts()?.into_iter().map(Account::from).collect())
    })
    .await
}

#[derive(Debug, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../src/types/bindings/",
    rename = "ContactsPhoneTargetArgs"
)]
pub struct PhoneTargetArgs {
    pub account: Account,
}

/// Sets the account new contacts go to on this phone (FR-018); contacts already written stay.
#[tauri::command]
pub async fn contacts_phone_set_target(
    state: State<'_, AppState>,
    args: PhoneTargetArgs,
) -> Result<()> {
    available()?;
    let db = active_database(&state)?;
    let device = db.device_id().to_string();
    db.write(move |tx| state::set_target(tx, &device, &args.account).map_err(Into::into))
        .await
}

/// The overview before the first run (FR-019).
#[tauri::command]
pub async fn contacts_phone_review(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<ReviewPlan> {
    with_phone(&app, &state, review).await
}

#[derive(Debug, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../src/types/bindings/",
    rename = "ContactsPhoneConfirmArgs"
)]
#[serde(rename_all = "camelCase")]
pub struct PhoneConfirmArgs {
    pub decisions: Vec<PairDecision>,
}

/// Runs the first run with the person's decisions on the uncertain pairs.
#[tauri::command]
pub async fn contacts_phone_confirm(
    app: AppHandle,
    state: State<'_, AppState>,
    args: PhoneConfirmArgs,
) -> Result<RunSummary> {
    let handle = app.clone();
    with_phone(&app, &state, move |db, book, device| {
        let summary = run(
            db,
            book,
            device,
            Mode::First(&args.decisions),
            &clock::now(),
        )?;
        announce(&handle, db, &summary);
        Ok(summary)
    })
    .await
}

/// The state of the sync on this phone.
#[tauri::command]
pub async fn contacts_phone_status(state: State<'_, AppState>) -> Result<PhoneStatus> {
    available()?;
    let db = active_database(&state)?;
    let device = db.device_id().to_string();
    db.read(move |q| state::status(q, &device).map_err(Into::into))
        .await
}

#[derive(Debug, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../src/types/bindings/",
    rename = "ContactsPhoneMassDeleteArgs"
)]
pub struct PhoneMassDeleteArgs {
    pub decision: MassDecision,
}

/// The person's decision on held-back deletions (FR-016); the run goes on with it.
#[tauri::command]
pub async fn contacts_phone_mass_delete(
    app: AppHandle,
    state: State<'_, AppState>,
    args: PhoneMassDeleteArgs,
) -> Result<RunSummary> {
    let handle = app.clone();
    with_phone(&app, &state, move |db, book, device| {
        let summary = run(
            db,
            book,
            device,
            Mode::Decided(args.decision),
            &clock::now(),
        )?;
        announce(&handle, db, &summary);
        Ok(summary)
    })
    .await
}

/// Runs the sync now.
#[tauri::command]
pub async fn contacts_phone_run(app: AppHandle, state: State<'_, AppState>) -> Result<RunSummary> {
    let handle = app.clone();
    with_phone(&app, &state, move |db, book, device| {
        let summary = run(db, book, device, Mode::Normal, &clock::now())?;
        announce(&handle, db, &summary);
        Ok(summary)
    })
    .await
}
