//! Thread + message CRUD, distinct from the streaming `send_message`
//! path in `commands.rs`.

use serde::{Deserialize, Serialize};
use tauri::State;
use unicode_segmentation::UnicodeSegmentation;
use uuid::Uuid;

use crate::error::{HolziError, Result};
use crate::state::AppState;
use crate::state_utils::active_database;
use crate::storage::chat_messages::{self as msg_store, ChatMessage};
use crate::storage::chat_threads::{self as thread_store, ChatThread};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateThreadArgs {
    pub title: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameThreadArgs {
    pub thread_id: String,
    pub title: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteThreadArgs {
    pub thread_id: String,
}

/// Trims and validates the user-visible thread title.
pub fn validate_thread_title(title: &str) -> Result<String> {
    let trimmed = title.trim();
    let length = trimmed.graphemes(true).count();
    if !(1..=120).contains(&length) {
        return Err(HolziError::InvalidInput {
            reason: "thread title must contain between 1 and 120 characters".into(),
        });
    }
    Ok(trimmed.to_string())
}

/// Creates an empty thread. Callers typically let `send_message`
/// create the thread implicitly; this command exists for a "New chat"
/// button that wants a stable id before the first message.
#[tauri::command]
pub async fn create_thread(
    state: State<'_, AppState>,
    args: CreateThreadArgs,
) -> Result<ThreadPayload> {
    let db = active_database(&state)?;
    let now = now_ms();
    let thread = ChatThread {
        id: Uuid::new_v4(),
        title: args.title.unwrap_or_else(|| "New chat".to_string()),
        last_provider_id: None,
        last_model_id: None,
        created_at: now,
        updated_at: now,
    };
    let inserted = thread.clone();
    db.write(move |tx| {
        thread_store::insert_thread(tx, &inserted)?;
        Ok(())
    })
    .await?;
    Ok(thread.into())
}

#[tauri::command]
pub async fn list_threads(state: State<'_, AppState>) -> Result<Vec<ThreadPayload>> {
    let db = active_database(&state)?;
    let rows = db.read(move |r| thread_store::list_threads(r)).await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

#[tauri::command]
pub async fn list_messages(
    state: State<'_, AppState>,
    thread_id: Uuid,
) -> Result<Vec<MessagePayload>> {
    let db = active_database(&state)?;
    let rows = db
        .read(move |r| msg_store::list_messages(r, thread_id))
        .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

#[tauri::command]
pub async fn rename_thread(
    state: State<'_, AppState>,
    args: RenameThreadArgs,
) -> Result<ThreadPayload> {
    let thread_id = Uuid::parse_str(&args.thread_id).map_err(|_| HolziError::InvalidInput {
        reason: "threadId must be a valid UUID".into(),
    })?;
    let title = validate_thread_title(&args.title)?;
    let db = active_database(&state)?;
    let row = db
        .write(move |tx| {
            if thread_store::get_thread(tx, thread_id)?.is_none() {
                return Ok(None);
            }
            let updated = thread_store::rename_title(tx, thread_id, &title)?;
            if updated != 1 {
                return Ok(None);
            }
            Ok(thread_store::get_thread(tx, thread_id)?.map(Into::into))
        })
        .await?;

    row.ok_or_else(|| HolziError::NotFound {
        name: thread_id.to_string(),
    })
}

#[tauri::command]
pub async fn delete_thread(state: State<'_, AppState>, args: DeleteThreadArgs) -> Result<()> {
    let thread_id = Uuid::parse_str(&args.thread_id).map_err(|_| HolziError::InvalidInput {
        reason: "threadId must be a valid UUID".into(),
    })?;
    let db = active_database(&state)?;
    let deleted = db
        .write(move |tx| thread_store::delete_thread_and_messages(tx, thread_id))
        .await?;

    if deleted {
        Ok(())
    } else {
        Err(HolziError::NotFound {
            name: thread_id.to_string(),
        })
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadPayload {
    pub id: Uuid,
    pub title: String,
    pub last_model_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl From<ChatThread> for ThreadPayload {
    fn from(t: ChatThread) -> Self {
        ThreadPayload {
            id: t.id,
            title: t.title,
            last_model_id: t.last_model_id,
            created_at: t.created_at,
            updated_at: t.updated_at,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessagePayload {
    pub id: Uuid,
    pub thread_id: Uuid,
    pub parent_id: Option<Uuid>,
    pub role: String,
    pub content: String,
    pub model_id: Option<String>,
    pub prompt_tokens: Option<i64>,
    pub completion_tokens: Option<i64>,
    pub finish_reason: Option<String>,
    pub created_at: i64,
    /// Set only when `role == "tool_call"` (data-model.md).
    pub tool_name: Option<String>,
    /// Correlates a `tool_call` row with its `tool_result` row.
    pub tool_call_id: Option<String>,
    /// JSON text of the tool input. Set only when `role == "tool_call"`.
    pub tool_input: Option<String>,
    /// Set only when `role == "tool_result"`.
    pub tool_is_error: Option<bool>,
    /// `mcp` or `cli`. Set only when `role == "tool_call"`.
    pub tool_source: Option<String>,
    /// Which `AutonomyMode` the turn ran under (spec
    /// 009-autonomous-delegate-mode): `"standard"` / `"ungated"` /
    /// `"gated_permissive"`. `null` for legacy and non-delegate rows.
    pub autonomy_mode: Option<String>,
    /// The extension of an extension's tool call (spec 018); absent otherwise, and for a stored
    /// origin that no longer reads.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_origin: Option<crate::chat::tools::ToolOrigin>,
}

impl From<ChatMessage> for MessagePayload {
    fn from(m: ChatMessage) -> Self {
        MessagePayload {
            id: m.id,
            thread_id: m.thread_id,
            parent_id: m.parent_id,
            role: match m.role {
                crate::storage::chat_messages::MessageRole::User => "user".into(),
                crate::storage::chat_messages::MessageRole::Assistant => "assistant".into(),
                crate::storage::chat_messages::MessageRole::System => "system".into(),
                crate::storage::chat_messages::MessageRole::ToolCall => "tool_call".into(),
                crate::storage::chat_messages::MessageRole::ToolResult => "tool_result".into(),
            },
            content: m.content,
            model_id: m.model_id,
            prompt_tokens: m.prompt_tokens,
            completion_tokens: m.completion_tokens,
            finish_reason: m.finish_reason.map(|r| match r {
                crate::storage::chat_messages::FinishReason::Complete => "complete".into(),
                crate::storage::chat_messages::FinishReason::Cancelled => "cancelled".into(),
                crate::storage::chat_messages::FinishReason::Error => "error".into(),
                crate::storage::chat_messages::FinishReason::ToolLimitReached => {
                    "tool_limit_reached".into()
                }
            }),
            created_at: m.created_at,
            tool_name: m.tool_name,
            tool_call_id: m.tool_call_id,
            tool_input: m.tool_input,
            tool_is_error: m.tool_is_error,
            tool_source: m.tool_source,
            autonomy_mode: m.autonomy_mode,
            tool_origin: m
                .tool_origin
                .as_deref()
                .and_then(|origin| serde_json::from_str(origin).ok()),
        }
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
