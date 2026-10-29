//! The commands of the mail openers (spec 063): the window asks holzi to read a link and to hand
//! it to the extension the person chose. The window sends the link text, never the fields, so what
//! reaches an extension is always what [`mail::parse`] made of it.

use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, State};
use uuid::Uuid;

use super::mail::{self, MailRequest};
use super::requests::{park_and_announce, TAKE_TIMEOUT};
use crate::error::{HolziError, Result};
use crate::extensions::bridge::dispatch::Emit;
use crate::extensions::bundle::OpenerKind;
use crate::extensions::commands::frames::WindowEmitter;
use crate::extensions::host::ExtensionHost;
use crate::extensions::registry::list::{list, with_dev_projects};
use crate::state::AppState;
use crate::state_utils::active_database;
use crate::storage::wm_session_commands::current_device_uuid;
use crate::vault_gate::VaultDb;

/// Reads `link` the way an extension would get it. A link that is refused is an error with the
/// reason; nothing is parked.
#[tauri::command]
pub fn opener_mail_check(link: String) -> Result<MailRequest> {
    mail::parse(&link)
}

/// Reads `link` again and parks the fields for the extension `extension_id`, which must be
/// installed, enabled and registered for mail links; the extension hears of it and takes it.
#[tauri::command]
pub async fn opener_mail_open(
    app: AppHandle,
    state: State<'_, AppState>,
    extension_id: String,
    link: String,
) -> Result<()> {
    let extension_id =
        Uuid::parse_str(&extension_id).map_err(|_| HolziError::OpenerNotAvailable)?;
    let db = active_database(&state)?;
    let device = current_device_uuid(&app, &db)?;
    let host = state.extensions();
    let emitter: Arc<dyn Emit> = Arc::new(WindowEmitter(app));
    tauri::async_runtime::spawn_blocking(move || {
        open_mail(
            &db,
            &host,
            emitter,
            device,
            extension_id,
            &link,
            TAKE_TIMEOUT,
        )
    })
    .await
    .map_err(|e| HolziError::InvalidInput {
        reason: format!("opener task: {e}"),
    })?
}

/// [`opener_mail_open`] without the window. Blocking.
pub(crate) fn open_mail(
    db: &VaultDb,
    host: &Arc<ExtensionHost>,
    emitter: Arc<dyn Emit>,
    device: Uuid,
    extension_id: Uuid,
    link: &str,
    timeout: Duration,
) -> Result<()> {
    let request = mail::parse(link)?;
    if !opens_mail(db, device, extension_id)? {
        return Err(HolziError::OpenerNotAvailable);
    }
    park_and_announce(host, emitter, extension_id, request, timeout);
    Ok(())
}

/// Whether `extension_id` is installed and enabled here and registered for mail links, by the
/// same reading the extension list shows (FR-012).
fn opens_mail(db: &VaultDb, device: Uuid, extension_id: Uuid) -> Result<bool> {
    let (listed, dev) = db.read_blocking(move |q| {
        Ok((
            list(q, device)?,
            crate::extensions::dev::registrations(q, device)?,
        ))
    })?;
    Ok(with_dev_projects(listed, &dev).iter().any(|e| {
        e.id == extension_id.to_string()
            && e.openers
                .iter()
                .any(|name| OpenerKind::from_name(name) == Some(OpenerKind::MailCompose))
    }))
}

#[cfg(test)]
#[path = "commands_tests.rs"]
mod tests;
