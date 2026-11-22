//! Files in contact conversations (spec 072 US3, contracts/ipc.md): sending picked files with a
//! message and downloading a file on request (above the automatic limit, or again after a
//! failure). Opening and saving follow with the window in PR E3.

use std::io::Read;
use std::sync::Arc;

use serde::Deserialize;
use tauri::{AppHandle, Manager, State};
use uuid::Uuid;

use crate::chat::peer::files::transfers::{ChatFiles, Picked, Source};
use crate::chat::peer::session::{PeerRegistry, Session};
use crate::error::{HaexError, Result};
use crate::files::kind::mime_for;
use crate::files::picked::{self, PickedFile};
use crate::files::state::FilesState;
use crate::passwords::commands::attachments::OpenSystemOutcome;
use crate::state::AppState;

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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenSystemArgs {
    pub attachment_id: String,
    /// The user confirmed the notice about the decrypted copy.
    #[serde(default)]
    pub confirmed: bool,
    /// "Nicht mehr fragen" was ticked in the notice (this device only).
    #[serde(default)]
    pub dont_ask_again: bool,
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

/// "Mit System-App öffnen" for a received or sent file (spec 072 US3 scenario 4): a decrypted copy
/// in a folder of its own, named as the sender named the file, handed to the system's app by the
/// same last step as the file browser and the password manager. The first time the notice about the
/// decrypted copy comes first (`needsNotice`); the copies go when the vault closes and after a crash.
/// Not on mobiles (spec 044 R11).
#[tauri::command]
pub async fn chat_attachment_open_system(
    app: AppHandle,
    state: State<'_, AppState>,
    files: State<'_, FilesState>,
    args: OpenSystemArgs,
) -> Result<OpenSystemOutcome> {
    #[cfg(mobile)]
    {
        let _ = (app, state, files, args);
        Err(HaexError::InvalidInput {
            reason: "unsupported".to_owned(),
        })
    }
    #[cfg(desktop)]
    {
        let id = Uuid::parse_str(&args.attachment_id).map_err(|_| HaexError::InvalidInput {
            reason: "attachmentId must be a valid UUID".into(),
        })?;
        // Arms the clean-up on close also when the file browser never ran in this session.
        files.forget_on_close(state.gate());
        let session = session(&app)?;
        let copy = chat_files(&session)?
            .copy_for_app(
                &id.to_string(),
                &files.opened_encrypted,
                args.confirmed,
                args.dont_ask_again,
            )
            .await?;
        let Some(path) = copy else {
            return Ok(OpenSystemOutcome::NeedsNotice);
        };
        if crate::files::open_with::open_copy(&app, &path, None).is_err() {
            log::warn!("chat: the system could not open a file");
            crate::passwords::attachment_view::remove_copy(&path).await;
            return Err(HaexError::InvalidInput {
                reason: "no_app".to_owned(),
            });
        }
        Ok(OpenSystemOutcome::Opened)
    }
}
