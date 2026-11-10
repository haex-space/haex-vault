//! Requests of vaults that are no contact (spec 072 US5, contracts/ipc.md): the carried card for
//! the preview of spec 060, and declining or blocking. None of them is open to agents (FR-072).

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::chat::peer::requests::{self, Answer};
use crate::error::{HolziError, Result};
use crate::state::AppState;
use crate::state_utils::active_database;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestArgs {
    pub thread_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnswerArgs {
    pub thread_id: String,
    pub answer: Answer,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestCard {
    /// `holzi:card:…`, read in by spec 060's preview on "Annehmen".
    pub card_text: String,
}

/// The card the sender of a request carried (FR-041).
#[tauri::command]
pub async fn chat_request_card(
    state: State<'_, AppState>,
    args: RequestArgs,
) -> Result<RequestCard> {
    let id = thread_id(&args.thread_id)?;
    let db = active_database(&state)?;
    let card_text = db.read(move |r| Ok(requests::card_text(r, id)?)).await?;
    Ok(RequestCard { card_text })
}

/// Declines or blocks a request (FR-041, FR-042).
#[tauri::command]
pub async fn chat_request_answer(state: State<'_, AppState>, args: AnswerArgs) -> Result<()> {
    let id = thread_id(&args.thread_id)?;
    let db = active_database(&state)?;
    db.write(move |tx| Ok(requests::answer(tx, id, args.answer)?))
        .await
}

fn thread_id(raw: &str) -> Result<uuid::Uuid> {
    uuid::Uuid::parse_str(raw).map_err(|_| HolziError::InvalidInput {
        reason: "threadId must be a valid UUID".into(),
    })
}
