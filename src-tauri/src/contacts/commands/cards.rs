//! Commands of the window for sharing a contact, reading a Visitenkarte and blocked vaults (spec 051
//! US4, spec 060 contracts/tauri-commands.md).

use serde::Deserialize;
use tauri::State;
use ts_rs::TS;

use super::service;
use crate::contacts::access::Caller;
use crate::contacts::card::received::BlockedVault;
use crate::contacts::model::Contact;
use crate::contacts::share::model::{
    ContactShareDecision, ContactShareOptions, ContactShareOutput, ContactSharePreview,
    ContactShareSelection, ContactShareTarget,
};
use crate::error::Result;
use crate::state::AppState;

#[derive(Debug, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../src/types/bindings/",
    rename = "ContactsShareOptionsArgs"
)]
#[serde(rename_all = "camelCase")]
pub struct ShareOptionsArgs {
    pub contact_id: String,
}

#[derive(Debug, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../src/types/bindings/",
    rename = "ContactsShareBuildArgs"
)]
#[serde(rename_all = "camelCase")]
pub struct ShareBuildArgs {
    pub contact_id: String,
    pub selection: ContactShareSelection,
}

#[derive(Debug, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../src/types/bindings/",
    optional_fields,
    rename = "ContactsSharePreviewArgs"
)]
pub struct SharePreviewArgs {
    pub text: String,
    pub target: Option<ContactShareTarget>,
}

#[derive(Debug, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../src/types/bindings/",
    optional_fields,
    rename = "ContactsShareAcceptArgs"
)]
#[serde(rename_all = "camelCase")]
pub struct ShareAcceptArgs {
    pub text: String,
    pub target: ContactShareTarget,
    #[serde(default)]
    pub decisions: Vec<ContactShareDecision>,
    pub preview_revision: Option<String>,
}

#[derive(Debug, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../src/types/bindings/",
    rename = "ContactsVaultArgs"
)]
#[serde(rename_all = "camelCase")]
pub struct VaultArgs {
    pub vault_pubkey: String,
}

/// What the share dialog shows for a contact.
#[tauri::command]
pub async fn contacts_share_options(
    state: State<'_, AppState>,
    args: ShareOptionsArgs,
) -> Result<ContactShareOptions> {
    service(&state)?
        .share_options(&Caller::User, args.contact_id)
        .await
}

/// The text of a share: link, clipboard text and QR code.
#[tauri::command]
pub async fn contacts_share_build(
    state: State<'_, AppState>,
    args: ShareBuildArgs,
) -> Result<ContactShareOutput> {
    service(&state)?
        .share_build(&Caller::User, args.contact_id, args.selection)
        .await
}

/// Checks a share (or an old Visitenkarte text) and shows what reading it in would do.
#[tauri::command]
pub async fn contacts_share_preview(
    state: State<'_, AppState>,
    args: SharePreviewArgs,
) -> Result<ContactSharePreview> {
    service(&state)?
        .share_preview(&Caller::User, args.text, args.target)
        .await
}

/// Reads a share in as a new contact or into an existing one, after the person's choices.
#[tauri::command]
pub async fn contacts_share_accept(
    state: State<'_, AppState>,
    args: ShareAcceptArgs,
) -> Result<Contact> {
    service(&state)?
        .share_accept(
            &Caller::User,
            args.text,
            args.target,
            args.decisions,
            args.preview_revision,
        )
        .await
}

/// The blocked vaults.
#[tauri::command]
pub async fn contacts_blocked(state: State<'_, AppState>) -> Result<Vec<BlockedVault>> {
    service(&state)?.blocked(&Caller::User).await
}

#[tauri::command]
pub async fn contacts_block(state: State<'_, AppState>, args: VaultArgs) -> Result<()> {
    service(&state)?
        .set_blocked(&Caller::User, args.vault_pubkey, true)
        .await
}

#[tauri::command]
pub async fn contacts_unblock(state: State<'_, AppState>, args: VaultArgs) -> Result<()> {
    service(&state)?
        .set_blocked(&Caller::User, args.vault_pubkey, false)
        .await
}
