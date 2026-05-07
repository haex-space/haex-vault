//! Tauri commands for extension mail operations.
//!
//! One command per operation, used by both WebView and iframe modes —
//! `resolve_extension_id` figures out which mode it is and returns the
//! correct `extension_id`. The IMAP/SMTP work itself is delegated to
//! `crate::mail`.

use tauri::{AppHandle, State, WebviewWindow};

use crate::extension::error::ExtensionError;
use crate::extension::permissions::manager::PermissionManager;
use crate::extension::permissions::types::MailAction;
use crate::extension::utils::{emit_permission_prompt_if_needed, resolve_extension_id};
use crate::mail::error::MailError;
use crate::mail::types::{
    FetchRange, ImapConfig, MailboxInfo, Message, MessageEnvelope, OutgoingMessage, SmtpConfig,
};
use crate::AppState;

/// Convert mail-module errors to ExtensionError. Auth failures map to
/// PermissionDenied so the SDK / UI can show the right message ("wrong
/// credentials") vs. a generic "web/imap error".
fn map_mail_error(err: MailError) -> ExtensionError {
    match err {
        MailError::ImapAuth { username, reason } => ExtensionError::WebError {
            reason: format!("IMAP authentication failed for {username}: {reason}"),
        },
        MailError::SmtpAuth { username, reason } => ExtensionError::WebError {
            reason: format!("SMTP authentication failed for {username}: {reason}"),
        },
        other => ExtensionError::WebError {
            reason: other.to_string(),
        },
    }
}

/// Check fetch permission for the given IMAP host. Emits a permission
/// prompt event if status is Ask / no permission exists.
async fn check_fetch_permission(
    app_handle: &AppHandle,
    state: &State<'_, AppState>,
    extension_id: &str,
    host: &str,
) -> Result<(), ExtensionError> {
    let result =
        PermissionManager::check_mail_permission(state, extension_id, MailAction::Fetch, host)
            .await;
    if let Err(ref e) = result {
        emit_permission_prompt_if_needed(app_handle, e);
    }
    result
}

/// Check send permission for the given SMTP host.
async fn check_send_permission(
    app_handle: &AppHandle,
    state: &State<'_, AppState>,
    extension_id: &str,
    host: &str,
) -> Result<(), ExtensionError> {
    let result =
        PermissionManager::check_mail_permission(state, extension_id, MailAction::Send, host)
            .await;
    if let Err(ref e) = result {
        emit_permission_prompt_if_needed(app_handle, e);
    }
    result
}

// ---------------------------------------------------------------------------
// IMAP operations (require MailAction::Fetch on imap.host)
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn extension_mail_list_mailboxes(
    app_handle: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    imap: ImapConfig,
    reference: Option<String>,
    pattern: Option<String>,
    include_status: Option<bool>,
    public_key: Option<String>,
    name: Option<String>,
) -> Result<Vec<MailboxInfo>, ExtensionError> {
    let extension_id = resolve_extension_id(&window, &state, public_key, name)?;
    check_fetch_permission(&app_handle, &state, &extension_id, &imap.host).await?;

    crate::mail::imap::list_mailboxes(
        &imap,
        reference.as_deref(),
        pattern.as_deref(),
        include_status.unwrap_or(false),
    )
    .await
    .map_err(map_mail_error)
}

#[tauri::command]
pub async fn extension_mail_fetch_envelopes(
    app_handle: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    imap: ImapConfig,
    mailbox: String,
    range: FetchRange,
    public_key: Option<String>,
    name: Option<String>,
) -> Result<Vec<MessageEnvelope>, ExtensionError> {
    let extension_id = resolve_extension_id(&window, &state, public_key, name)?;
    check_fetch_permission(&app_handle, &state, &extension_id, &imap.host).await?;

    crate::mail::imap::fetch_envelopes(&imap, &mailbox, &range)
        .await
        .map_err(map_mail_error)
}

#[tauri::command]
pub async fn extension_mail_fetch_message(
    app_handle: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    imap: ImapConfig,
    mailbox: String,
    uid: u32,
    public_key: Option<String>,
    name: Option<String>,
) -> Result<Message, ExtensionError> {
    let extension_id = resolve_extension_id(&window, &state, public_key, name)?;
    check_fetch_permission(&app_handle, &state, &extension_id, &imap.host).await?;

    crate::mail::imap::fetch_message(&imap, &mailbox, uid)
        .await
        .map_err(map_mail_error)
}

#[tauri::command]
pub async fn extension_mail_set_flags(
    app_handle: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    imap: ImapConfig,
    mailbox: String,
    uids: Vec<u32>,
    flags: Vec<String>,
    add: bool,
    public_key: Option<String>,
    name: Option<String>,
) -> Result<(), ExtensionError> {
    let extension_id = resolve_extension_id(&window, &state, public_key, name)?;
    check_fetch_permission(&app_handle, &state, &extension_id, &imap.host).await?;

    crate::mail::imap::set_flags(&imap, &mailbox, &uids, &flags, add)
        .await
        .map_err(map_mail_error)
}

#[tauri::command]
pub async fn extension_mail_move_messages(
    app_handle: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    imap: ImapConfig,
    source_mailbox: String,
    destination_mailbox: String,
    uids: Vec<u32>,
    public_key: Option<String>,
    name: Option<String>,
) -> Result<(), ExtensionError> {
    let extension_id = resolve_extension_id(&window, &state, public_key, name)?;
    check_fetch_permission(&app_handle, &state, &extension_id, &imap.host).await?;

    crate::mail::imap::move_messages(&imap, &source_mailbox, &destination_mailbox, &uids)
        .await
        .map_err(map_mail_error)
}

/// APPEND a base64-encoded RFC822 message into a mailbox. Used for
/// "save copy to Sent folder" after a successful SMTP send.
#[tauri::command]
pub async fn extension_mail_append_message(
    app_handle: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    imap: ImapConfig,
    mailbox: String,
    rfc822_base64: String,
    flags: Option<Vec<String>>,
    public_key: Option<String>,
    name: Option<String>,
) -> Result<(), ExtensionError> {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine as _;

    let extension_id = resolve_extension_id(&window, &state, public_key, name)?;
    check_fetch_permission(&app_handle, &state, &extension_id, &imap.host).await?;

    let bytes = STANDARD
        .decode(&rfc822_base64)
        .map_err(|e| ExtensionError::ValidationError {
            reason: format!("invalid base64 in rfc822_base64: {e}"),
        })?;
    let flags_vec = flags.unwrap_or_default();
    crate::mail::imap::append_message(&imap, &mailbox, &bytes, &flags_vec)
        .await
        .map_err(map_mail_error)
}

// ---------------------------------------------------------------------------
// SMTP operations (require MailAction::Send on smtp.host)
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn extension_mail_send_message(
    app_handle: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    smtp: SmtpConfig,
    message: OutgoingMessage,
    public_key: Option<String>,
    name: Option<String>,
) -> Result<String, ExtensionError> {
    let extension_id = resolve_extension_id(&window, &state, public_key, name)?;
    check_send_permission(&app_handle, &state, &extension_id, &smtp.host).await?;

    crate::mail::smtp::send_message(&smtp, &message)
        .await
        .map_err(map_mail_error)
}

/// Build the RFC822 bytes for a message WITHOUT sending — useful when
/// the extension wants to APPEND a draft to a "Drafts" folder via IMAP
/// without going through SMTP first. Permission-wise this is a fetch
/// operation (no SMTP host involved), so we require Fetch on the IMAP
/// host the extension is about to APPEND to.
///
/// Caller passes the IMAP host alongside; the wrapper only needs it
/// for the permission check, NOT for any actual IMAP work.
#[tauri::command]
pub async fn extension_mail_build_rfc822(
    app_handle: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    imap_host: String,
    message: OutgoingMessage,
    public_key: Option<String>,
    name: Option<String>,
) -> Result<String, ExtensionError> {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine as _;

    let extension_id = resolve_extension_id(&window, &state, public_key, name)?;
    check_fetch_permission(&app_handle, &state, &extension_id, &imap_host).await?;

    let bytes = crate::mail::smtp::build_message_bytes(&message).map_err(map_mail_error)?;
    Ok(STANDARD.encode(&bytes))
}
