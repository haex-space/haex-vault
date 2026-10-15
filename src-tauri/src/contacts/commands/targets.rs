//! Commands of the window for sync targets (spec 051 FR-018b to FR-018d,
//! contracts/tauri-commands.md §Abgleichsziele). They run on every device: a contact gets a phone
//! as its target on the desktop too.

use serde::Deserialize;
use tauri::State;
use ts_rs::TS;

use super::service;
use crate::contacts::access::Caller;
use crate::contacts::phone::state::PhoneSummary;
use crate::contacts::phone::Account;
use crate::contacts::service::{OnPhone, Selection};
use crate::error::{HolziError, Result};
use crate::state::AppState;

#[derive(Debug, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../src/types/bindings/",
    optional_fields,
    rename = "ContactsTargetsSetArgs"
)]
#[serde(rename_all = "camelCase")]
pub struct TargetsSetArgs {
    /// The contacts; or `group_id` for every contact of a group.
    pub contact_ids: Option<Vec<String>>,
    pub group_id: Option<String>,
    pub device: String,
    /// Absent: the phone's target account.
    pub account: Option<Account>,
}

#[derive(Debug, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../src/types/bindings/",
    rename = "ContactsTargetsRemoveArgs"
)]
#[serde(rename_all = "camelCase")]
pub struct TargetsRemoveArgs {
    pub contact_ids: Vec<String>,
    pub device: String,
    pub on_phone: OnPhone,
}

/// The phones of the vault with the sync on.
#[tauri::command]
pub async fn contacts_phones(state: State<'_, AppState>) -> Result<Vec<PhoneSummary>> {
    service(&state)?.phones(&Caller::User).await
}

/// Makes a phone a target of contacts or of every contact of a group.
#[tauri::command]
pub async fn contacts_targets_set(state: State<'_, AppState>, args: TargetsSetArgs) -> Result<()> {
    let selection = match (args.contact_ids, args.group_id) {
        (Some(ids), None) => Selection::Contacts(ids),
        (None, Some(group)) => Selection::Group(group),
        _ => {
            return Err(HolziError::ContactsInvalid {
                reason: "name either contacts or a group".to_owned(),
            })
        }
    };
    service(&state)?
        .targets_set(&Caller::User, selection, args.device, args.account)
        .await
}

/// Removes a phone as a target of contacts; the phone deletes them or keeps them unsynced.
#[tauri::command]
pub async fn contacts_targets_remove(
    state: State<'_, AppState>,
    args: TargetsRemoveArgs,
) -> Result<()> {
    service(&state)?
        .targets_remove(&Caller::User, args.contact_ids, args.device, args.on_phone)
        .await
}
