//! Files in contact conversations (spec 072 US3, contracts/ipc.md): sending picked files with a
//! message and downloading a file on request (above the automatic limit, or again after a
//! failure). Opening and saving follow with the window in PR E3.

use std::io::Read;
use std::sync::Arc;

use serde::Deserialize;
use tauri::{AppHandle, Manager};
use uuid::Uuid;

use crate::chat::peer::files::transfers::{ChatFiles, Picked, Source};
use crate::chat::peer::session::{PeerRegistry, Session};
use crate::error::{HaexError, Result};
use crate::files::kind::mime_for;
use crate::files::picked::{self, PickedFile};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadArgs {
    pub attachment_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextArgs {
    pub attachment_id: String,
}

pub(super) fn session(app: &AppHandle) -> Result<Arc<Session>> {
    app.try_state::<Arc<PeerRegistry>>()
        .ok_or_else(|| HaexError::NotFound {
            name: "chat session".into(),
        })?
        .require()
}

fn chat_files(session: &Session) -> Result<Arc<ChatFiles>> {
    session
        .files
        .get()
        .cloned()
        .ok_or_else(|| HaexError::NotFound {
            name: "chat files".into(),
        })
}

/// The picked files as sources for sending. Nothing is read here: every pass over a file opens it
/// again, so a file of up to the limit (FR-034) is never held whole.
async fn pick_files(
    app: &AppHandle,
    files: Vec<PickedFile>,
    inline_text: Option<Picked>,
) -> Result<Vec<Picked>> {
    let app = app.clone();
    tokio::task::spawn_blocking(move || {
        let mut picked: Vec<Picked> = files
            .into_iter()
            .map(|file| {
                let name = picked::display_name(&app, &file);
                let mime = mime_for(&name)
                    .unwrap_or("application/octet-stream")
                    .to_owned();
                let opener = app.clone();
                Picked {
                    name,
                    mime,
                    source: Source::opened_by(move || {
                        Ok(Box::new(picked::open_read(&opener, &file)?) as Box<dyn Read + Send>)
                    }),
                    inline: false,
                }
            })
            .collect();
        picked.extend(inline_text);
        Ok(picked)
    })
    .await
    .map_err(|e| HaexError::InvalidInput {
        reason: e.to_string(),
    })?
}

/// Uploads the picked files and queues the message with them (FR-031).
pub(super) async fn send_with_files(
    app: &AppHandle,
    thread_id: Uuid,
    event_id: &str,
    text: &str,
    files: Vec<PickedFile>,
    inline_text: Option<Picked>,
) -> Result<Uuid> {
    let session = session(app)?;
    let picked = pick_files(app, files, inline_text).await?;
    let id = chat_files(&session)?
        .send(&session.me, thread_id, event_id, text, picked)
        .await?;
    session.kick.notify_one();
    Ok(id)
}

/// Downloads a file now (FR-036: above the automatic limit; or again after a failure).
#[tauri::command]
pub async fn chat_attachment_download(app: AppHandle, args: DownloadArgs) -> Result<()> {
    let id = Uuid::parse_str(&args.attachment_id).map_err(|_| HaexError::InvalidInput {
        reason: "attachmentId must be a valid UUID".into(),
    })?;
    let session = session(&app)?;
    let files = chat_files(&session)?;
    files.download(&id.to_string()).await
}

/// The full text of a long message from this device's copy of its `inline` file (FR-012a).
#[tauri::command]
pub async fn chat_attachment_text(app: AppHandle, args: TextArgs) -> Result<String> {
    let id = Uuid::parse_str(&args.attachment_id).map_err(|_| HaexError::InvalidInput {
        reason: "attachmentId must be a valid UUID".into(),
    })?;
    let session = session(&app)?;
    chat_files(&session)?.text(&id.to_string()).await
}
