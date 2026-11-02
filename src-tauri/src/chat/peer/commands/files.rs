//! Files in contact conversations (spec 072 US3, contracts/ipc.md): sending picked files with a
//! message and downloading a file on request (above the automatic limit, or again after a
//! failure). Opening and saving follow with the window in PR E3.

use std::sync::Arc;

use serde::Deserialize;
use tauri::{AppHandle, Manager};
use uuid::Uuid;

use crate::chat::peer::files::crypto::MAX_PLAINTEXT;
use crate::chat::peer::files::transfers::{ChatFiles, Picked};
use crate::chat::peer::session::{PeerRegistry, Session};
use crate::error::{HolziError, Result};
use crate::files::kind::mime_for;
use crate::files::picked::{self, PickedFile};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadArgs {
    pub attachment_id: String,
}

pub(super) fn session(app: &AppHandle) -> Result<Arc<Session>> {
    app.try_state::<Arc<PeerRegistry>>()
        .ok_or_else(|| HolziError::NotFound {
            name: "chat session".into(),
        })?
        .require()
}

fn chat_files(session: &Session) -> Result<Arc<ChatFiles>> {
    session
        .files
        .get()
        .cloned()
        .ok_or_else(|| HolziError::NotFound {
            name: "chat files".into(),
        })
}

/// Reads the picked files, each at most 100 MB (FR-034).
async fn read_picked(app: &AppHandle, files: Vec<PickedFile>) -> Result<Vec<Picked>> {
    let app = app.clone();
    tokio::task::spawn_blocking(move || {
        files
            .iter()
            .map(|file| {
                let name = picked::display_name(&app, file);
                let bytes = picked::read(&app, file, MAX_PLAINTEXT)?;
                if bytes.len() as u64 > MAX_PLAINTEXT {
                    return Err(HolziError::InvalidInput {
                        reason: "chat.file.tooLarge".into(),
                    });
                }
                let mime = mime_for(&name)
                    .unwrap_or("application/octet-stream")
                    .to_owned();
                Ok(Picked { name, mime, bytes })
            })
            .collect()
    })
    .await
    .map_err(|e| HolziError::InvalidInput {
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
) -> Result<Uuid> {
    let session = session(app)?;
    let picked = read_picked(app, files).await?;
    let id = chat_files(&session)?
        .send(&session.me, thread_id, event_id, text, picked)
        .await?;
    session.kick.notify_one();
    Ok(id)
}

/// Downloads a file now (FR-036: above the automatic limit; or again after a failure).
#[tauri::command]
pub async fn chat_attachment_download(app: AppHandle, args: DownloadArgs) -> Result<()> {
    let id = Uuid::parse_str(&args.attachment_id).map_err(|_| HolziError::InvalidInput {
        reason: "attachmentId must be a valid UUID".into(),
    })?;
    let session = session(&app)?;
    let files = chat_files(&session)?;
    files.download(&id.to_string()).await
}
