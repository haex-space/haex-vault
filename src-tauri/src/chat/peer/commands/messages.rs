//! Messages of contact conversations (spec 072 US2, contracts/ipc.md): send, send again, list and
//! mark read. Sending queues; the delivery service of the open vault publishes.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};
use uuid::Uuid;

use crate::chat::peer::delivery::now_ms;
use crate::chat::peer::messages::{self, Page, PeerMessage, PAGE};
use crate::chat::peer::session::{PeerRegistry, Session};
use crate::chat::peer::{outbox, send};
use crate::error::{HolziError, Result};
use crate::state::AppState;
use crate::state_utils::active_database;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendArgs {
    pub thread_id: String,
    pub text: String,
    /// A UUID the window makes once per message; a retry with the same key sends nothing twice.
    pub idempotency_key: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SentMessage {
    pub message_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageArgs {
    pub message_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadArgs {
    pub thread_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListArgs {
    pub thread_id: String,
    /// Only messages with an earlier `sortAt` (the next older page).
    pub before: Option<i64>,
    pub limit: Option<u32>,
}

fn session(app: &AppHandle) -> Result<Arc<Session>> {
    app.try_state::<Arc<PeerRegistry>>()
        .ok_or_else(|| HolziError::NotFound {
            name: "chat session".into(),
        })?
        .require()
}

fn uuid(raw: &str, name: &str) -> Result<Uuid> {
    Uuid::parse_str(raw).map_err(|_| HolziError::InvalidInput {
        reason: format!("{name} must be a valid UUID"),
    })
}

/// Sends a text to a direct conversation (FR-010, FR-012).
#[tauri::command]
pub async fn chat_peer_send(
    app: AppHandle,
    state: State<'_, AppState>,
    args: SendArgs,
) -> Result<SentMessage> {
    let thread_id = uuid(&args.thread_id, "threadId")?;
    let event_id = uuid(&args.idempotency_key, "idempotencyKey")?
        .simple()
        .to_string();
    let session = session(&app)?;
    let db = active_database(&state)?;
    let me = session.me.clone();
    let now = now_ms();
    let message_id = db
        .write(move |tx| {
            Ok(send::message(
                tx, &me, thread_id, &event_id, &args.text, now,
            )?)
        })
        .await?;
    session.kick.notify_one();
    Ok(SentMessage {
        message_id: message_id.to_string(),
    })
}

/// "Erneut senden" for a failed message (US2 scenario 4).
#[tauri::command]
pub async fn chat_peer_resend(
    app: AppHandle,
    state: State<'_, AppState>,
    args: MessageArgs,
) -> Result<()> {
    let message_id = uuid(&args.message_id, "messageId")?.to_string();
    let session = session(&app)?;
    let db = active_database(&state)?;
    let now = now_ms();
    let found = db
        .write(move |tx| Ok(outbox::resend(tx, &message_id, now)?))
        .await?;
    if !found {
        return Err(HolziError::InvalidInput {
            reason: "chat.error.resendElsewhere".into(),
        });
    }
    session.kick.notify_one();
    Ok(())
}

/// The messages of a contact conversation, oldest first.
#[tauri::command]
pub async fn chat_peer_messages_list(
    app: AppHandle,
    state: State<'_, AppState>,
    args: ListArgs,
) -> Result<Vec<PeerMessage>> {
    let thread_id = uuid(&args.thread_id, "threadId")?;
    let page = Page {
        before: args.before,
        limit: args.limit.unwrap_or(PAGE).clamp(1, 1_000),
    };
    let own = session(&app)?.me.vault_hex.clone();
    let db = active_database(&state)?;
    db.read(move |r| Ok(messages::list(r, &own, thread_id, page)?))
        .await
}

/// Marks a conversation read up to its newest message (FR-022) and sends the read receipt
/// (FR-024).
#[tauri::command]
pub async fn chat_conversation_read(
    app: AppHandle,
    state: State<'_, AppState>,
    args: ThreadArgs,
) -> Result<()> {
    let thread_id = uuid(&args.thread_id, "threadId")?;
    let session = session(&app)?;
    let db = active_database(&state)?;
    let me = session.me.clone();
    let now = now_ms();
    let queued = db
        .write(move |tx| {
            let upto = messages::newest(tx, thread_id)?;
            Ok(send::read_up_to(tx, &me, thread_id, upto, now)?)
        })
        .await?;
    if queued {
        session.kick.notify_one();
    }
    Ok(())
}
