//! Requests of strangers (spec 099, contracts/ipc.md): the carried card for the preview of spec 060,
//! and declining. None of them is open to agents (spec 072 FR-072).

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

use crate::chat::peer::delivery::now_ms;
use crate::chat::peer::requests::{self, Answer};
use crate::error::{HaexError, Result};
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
    /// `haex:card:…`, read in by spec 060's preview on "Annehmen".
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

/// Declines a request (spec 099 FR-009): blocks its vault and sends it the answer "blocked".
#[tauri::command]
pub async fn chat_request_answer(
    app: AppHandle,
    state: State<'_, AppState>,
    args: AnswerArgs,
) -> Result<()> {
    let id = thread_id(&args.thread_id)?;
    let db = active_database(&state)?;
    let now = now_ms();
    db.write(move |tx| Ok(requests::answer(tx, id, args.answer, now)?))
        .await?;
    // The answer goes out at the session's next pump; wake it now.
    if let Ok(session) = super::files::session(&app) {
        session.kick.notify_one();
    }
    Ok(())
}

fn thread_id(raw: &str) -> Result<uuid::Uuid> {
    uuid::Uuid::parse_str(raw).map_err(|_| HaexError::InvalidInput {
        reason: "threadId must be a valid UUID".into(),
    })
}
