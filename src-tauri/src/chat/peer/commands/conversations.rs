//! The conversation list and its entries (spec 072 US1, FR-001, FR-002, FR-004, FR-005, FR-008).

use serde::{Deserialize, Serialize};
use tauri::State;
use uuid::Uuid;

use crate::chat::peer::store::{self, Conversation, ListFilter};
use crate::contacts::card::received;
use crate::contacts::ids::is_self;
use crate::error::{HolziError, Result};
use crate::state::AppState;
use crate::state_utils::active_database;
use crate::storage::query::Query;
use crate::sync::keys::{self, hex};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenDirectArgs {
    pub contact_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenedConversation {
    pub thread_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MuteArgs {
    pub thread_id: String,
    pub muted: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadArgs {
    pub thread_id: String,
}

/// Every conversation of the vault, all kinds in one list (FR-001, FR-002).
#[tauri::command]
pub async fn chat_conversations_list(
    state: State<'_, AppState>,
    args: Option<ListFilter>,
) -> Result<Vec<Conversation>> {
    let db = active_database(&state)?;
    let filter = args.unwrap_or_default();
    db.read(move |r| {
        let own = own_vault(r)?;
        Ok(store::list(r, &own, filter)?)
    })
    .await
}

/// Opens the direct conversation with a contact, creating it the first time (FR-005). Refused for
/// the own contact and for contacts without a confirmed card of another vault (FR-004).
#[tauri::command]
pub async fn chat_conversation_open_direct(
    state: State<'_, AppState>,
    args: OpenDirectArgs,
) -> Result<OpenedConversation> {
    if is_self(&args.contact_id) {
        return Err(no_card());
    }
    let db = active_database(&state)?;
    let now = now_ms();
    let id = db
        .write(move |tx| {
            let own = own_vault(tx)?;
            let card = received::stored_of_contact(tx, &args.contact_id)?.ok_or_else(no_card)?;
            let peer = hex(&card.card.vault);
            if peer == own {
                return Err(no_card().into());
            }
            Ok(store::open_direct(tx, &own, &peer, now)?)
        })
        .await?;
    Ok(OpenedConversation {
        thread_id: id.to_string(),
    })
}

/// Opens the user's notes ("Ich"), creating them the first time (FR-002b).
#[tauri::command]
pub async fn chat_conversation_open_note(state: State<'_, AppState>) -> Result<OpenedConversation> {
    let db = active_database(&state)?;
    let now = now_ms();
    let id = db
        .write(move |tx| {
            let own = own_vault(tx)?;
            Ok(store::open_note(tx, &own, now)?)
        })
        .await?;
    Ok(OpenedConversation {
        thread_id: id.to_string(),
    })
}

/// Mutes or unmutes a conversation of any kind (FR-008).
#[tauri::command]
pub async fn chat_conversation_mute(state: State<'_, AppState>, args: MuteArgs) -> Result<()> {
    let id = thread_id(&args.thread_id)?;
    let db = active_database(&state)?;
    let found = db
        .write(move |tx| Ok(store::set_muted(tx, id, args.muted)?))
        .await?;
    found.then_some(()).ok_or_else(|| not_found(id))
}

/// Deletes a conversation of any kind on every own device (FR-008).
#[tauri::command]
pub async fn chat_conversation_delete(state: State<'_, AppState>, args: ThreadArgs) -> Result<()> {
    let id = thread_id(&args.thread_id)?;
    let db = active_database(&state)?;
    let found = db.write(move |tx| Ok(store::delete(tx, id)?)).await?;
    found.then_some(()).ok_or_else(|| not_found(id))
}

fn own_vault(q: &mut impl Query) -> Result<String> {
    keys::vault_pubkey(q)?
        .map(|vault| hex(&vault))
        .ok_or_else(|| HolziError::NotFound {
            name: "vault identity".into(),
        })
}

fn thread_id(raw: &str) -> Result<Uuid> {
    Uuid::parse_str(raw).map_err(|_| HolziError::InvalidInput {
        reason: "threadId must be a valid UUID".into(),
    })
}

fn no_card() -> HolziError {
    HolziError::InvalidInput {
        reason: "the contact has no confirmed Visitenkarte of another vault".into(),
    }
}

fn not_found(id: Uuid) -> HolziError {
    HolziError::NotFound {
        name: id.to_string(),
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
