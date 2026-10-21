//! Read commands of the window (spec 034, `contracts/tauri-commands.md` §Lesen). Each calls only
//! the service as `Caller::User`. Secrets leave only through `passwords_reveal`; copying and the
//! TOTP code never hand the secret to the webview.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};
use ts_rs::TS;
use zeroize::Zeroizing;

use super::service;
use crate::error::Result;
use crate::passwords::access::Caller;
use crate::passwords::clipboard::ClipboardPort;
use crate::passwords::model::{
    CopyField, ItemDetail, Overview, RevealedSecret, SecretField, TextField, TotpCode,
};
use crate::state::AppState;

#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct ItemIdArgs {
    pub item_id: String,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct RevealArgs {
    pub item_id: String,
    pub field: SecretField,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct CopyFieldArgs {
    pub item_id: String,
    pub field: CopyField,
}

/// What a copy did: after how many seconds the clipboard is cleared, `None` when that is off.
#[derive(Debug, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct CopyResult {
    pub clears_in_seconds: Option<u32>,
}

/// Every entry, folder and tag in one answer, for the store of the window.
#[tauri::command]
pub async fn passwords_load_overview(state: State<'_, AppState>) -> Result<Overview> {
    service(&state)?.load_overview(&Caller::User).await
}

/// One entry without secrets.
#[tauri::command]
pub async fn passwords_get_item(
    state: State<'_, AppState>,
    args: ItemIdArgs,
) -> Result<ItemDetail> {
    service(&state)?.get_item(&Caller::User, args.item_id).await
}

/// A secret, only on the user's explicit act.
#[tauri::command]
pub async fn passwords_reveal(
    state: State<'_, AppState>,
    args: RevealArgs,
) -> Result<RevealedSecret> {
    service(&state)?
        .reveal(&Caller::User, args.item_id, args.field)
        .await
}

/// The current TOTP code with the time left.
#[tauri::command]
pub async fn passwords_totp_code(state: State<'_, AppState>, args: ItemIdArgs) -> Result<TotpCode> {
    service(&state)?
        .totp_code(&Caller::User, &[], args.item_id)
        .await
}

#[derive(Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct CopyTextArgs {
    pub text: String,
    /// The entry the text belongs to, for its own placeholders; none for a new entry.
    #[ts(optional)]
    pub item_id: Option<String>,
    /// The field the text comes from: its placeholders are resolved before the copy (FR-045).
    /// Without it the text is copied as it is (a title, a date).
    #[ts(optional)]
    pub field: Option<TextField>,
}

/// Prints no text: it may be a password from the editor.
impl std::fmt::Debug for CopyTextArgs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CopyTextArgs")
            .field("text", &"<redacted>")
            .field("item_id", &self.item_id)
            .field("field", &self.field)
            .finish()
    }
}

/// Puts `text` on the clipboard and plans its clearing after the vault's delay.
pub(super) async fn copy_to_clipboard(
    app: AppHandle,
    state: &State<'_, AppState>,
    text: &str,
) -> Result<CopyResult> {
    let delay = service(state)?.clipboard_delay(&Caller::User).await?;
    let port: Arc<dyn ClipboardPort> = Arc::new(app);
    state.clipboard().copy(port, text, delay)?;
    Ok(CopyResult {
        clears_in_seconds: delay.map(|d| u32::try_from(d.as_secs()).unwrap_or(u32::MAX)),
    })
}

/// Copies a value to the clipboard in Rust and plans the clearing; the value is not returned.
#[tauri::command]
pub async fn passwords_copy_field(
    app: AppHandle,
    state: State<'_, AppState>,
    args: CopyFieldArgs,
) -> Result<CopyResult> {
    let copied = service(&state)?
        .copy_value(&Caller::User, args.item_id, args.field)
        .await?;
    copy_to_clipboard(app, &state, copied.as_str()).await
}

/// Copies a text the window already holds (a value in the editor, a plain field of a history
/// state), with the same clearing as every other copy of the password manager.
#[tauri::command]
pub async fn passwords_copy_text(
    app: AppHandle,
    state: State<'_, AppState>,
    args: CopyTextArgs,
) -> Result<CopyResult> {
    let text = Zeroizing::new(args.text);
    let text = match args.field {
        Some(field) => {
            service(&state)?
                .resolve_text(&Caller::User, args.item_id, field, text)
                .await?
        }
        None => text,
    };
    copy_to_clipboard(app, &state, &text).await
}
