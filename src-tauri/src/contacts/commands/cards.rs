//! Commands of the window for sharing a contact, reading a Visitenkarte and blocked vaults (spec 051
//! US4, spec 060 contracts/tauri-commands.md).

use serde::Deserialize;
use tauri::State;
use ts_rs::TS;

use super::service;
use crate::contacts::access::Caller;
use crate::contacts::card::received::BlockedVault;
use crate::contacts::model::Contact;
use crate::contacts::service::{CardInto, CardPreview};
use crate::contacts::share::model::{
    ContactShareOptions, ContactShareOutput, ContactShareSelection,
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
    rename = "ContactsCardTextArgs"
)]
pub struct CardTextArgs {
    pub text: String,
}

#[derive(Debug, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../src/types/bindings/",
    rename = "ContactsCardAcceptArgs"
)]
pub struct CardAcceptArgs {
    pub text: String,
    pub into: CardInto,
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

/// Checks a Visitenkarte and shows what it carries.
#[tauri::command]
pub async fn contacts_card_preview(
    state: State<'_, AppState>,
    args: CardTextArgs,
) -> Result<CardPreview> {
    service(&state)?
        .card_preview(&Caller::User, args.text)
        .await
}

/// Stores a Visitenkarte as a new contact or on an existing one.
#[tauri::command]
pub async fn contacts_card_accept(
    state: State<'_, AppState>,
    args: CardAcceptArgs,
) -> Result<Contact> {
    service(&state)?
        .card_accept(&Caller::User, args.text, args.into)
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
