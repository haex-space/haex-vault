//! Commands of the window for Visitenkarten and blocked vaults (spec 051 US4,
//! contracts/tauri-commands.md §Visitenkarten).

use serde::Deserialize;
use tauri::State;
use ts_rs::TS;

use super::service;
use crate::contacts::access::Caller;
use crate::contacts::card::own_card::OwnCard;
use crate::contacts::card::received::BlockedVault;
use crate::contacts::model::Contact;
use crate::contacts::service::{CardInto, CardPreview};
use crate::error::Result;
use crate::state::AppState;

#[derive(Debug, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../src/types/bindings/",
    optional_fields,
    rename = "ContactsOwnCardSetArgs"
)]
#[serde(rename_all = "camelCase")]
pub struct OwnCardSetArgs {
    pub own_contact: Option<String>,
    #[serde(default)]
    pub value_ids: Vec<String>,
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

/// The own Visitenkarte.
#[tauri::command]
pub async fn contacts_own_card(state: State<'_, AppState>) -> Result<OwnCard> {
    service(&state)?.own_card(&Caller::User).await
}

/// Chooses what the own Visitenkarte carries.
#[tauri::command]
pub async fn contacts_own_card_set(
    state: State<'_, AppState>,
    args: OwnCardSetArgs,
) -> Result<OwnCard> {
    service(&state)?
        .own_card_set(&Caller::User, args.own_contact, args.value_ids)
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
