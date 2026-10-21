//! The bridge methods `extension_mail_*` (contracts/bridge.md): parameters as the SDK sends
//! them, the permission per server and port (`fetch` for IMAP, `send` for SMTP), and the
//! answers in the SDK's shapes. Building a message talks to no server and needs no permission.

use std::future::Future;
use std::time::Duration;

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use serde::de::DeserializeOwned;
use serde_json::Value;

use super::imap_manage::{self, MAX_UIDS};
use super::{
    check_permission, imap, smtp, FetchRange, MailError, OutgoingMessage, SearchCriteria,
    ServerConfig,
};
use crate::extensions::bridge::blocking::block_on;
use crate::extensions::bridge::dispatch::CallContext;
use crate::extensions::error::{BridgeError, ExtensionErrorCode};
use crate::extensions::permissions::Action;
use crate::extensions::sql::exec::{limits_of, Limits};

pub const MODULE: &str = module_path!();

/// How long one call may talk to a server.
const MAIL_TIMEOUT: Duration = Duration::from_secs(60);

/// Deserializes one required bridge parameter and maps malformed input to validation.
fn field<T: DeserializeOwned>(params: &Value, name: &str) -> Result<T, BridgeError> {
    let value = params.get(name).cloned().unwrap_or(Value::Null);
    serde_json::from_value(value).map_err(|_| {
        BridgeError::new(
            ExtensionErrorCode::Validation,
            format!("{name} not understood"),
        )
    })
}

/// Deserializes an optional bridge parameter, treating absent and null values alike.
fn optional<T: DeserializeOwned>(params: &Value, name: &str) -> Result<Option<T>, BridgeError> {
    match params.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(_) => field(params, name).map(Some),
    }
}

/// Runs a mail operation within [`MAIL_TIMEOUT`].
fn run<T>(work: impl Future<Output = Result<T, MailError>>) -> Result<T, BridgeError> {
    block_on(async {
        tokio::time::timeout(MAIL_TIMEOUT, work)
            .await
            .unwrap_or(Err(MailError::Timeout))
    })
    .map_err(Into::into)
}

/// Serializes an operation result into the bridge's JSON response value, refused (7000) when it
/// is larger than the extension's response limit: a mailbox list, many envelopes, or a message
/// whose text and HTML body both come along.
fn to_json<T: serde::Serialize>(ctx: &CallContext, value: T) -> Result<Value, BridgeError> {
    let bytes = serde_json::to_vec(&value)
        .map_err(|_| BridgeError::new(ExtensionErrorCode::Web, "answer not understood"))?;
    if bytes.len() as u64 > limits(ctx)?.max_response_bytes {
        return Err(MailError::TooLarge.into());
    }
    serde_json::from_slice(&bytes)
        .map_err(|_| BridgeError::new(ExtensionErrorCode::Web, "answer not understood"))
}

/// The IMAP server of the call, after the `fetch` check for its host and port.
fn imap_server(ctx: &CallContext, params: &Value) -> Result<ServerConfig, BridgeError> {
    let config: ServerConfig = field(params, "imap")?;
    check_permission(ctx, Action::Fetch, &config.host, config.port)?;
    Ok(config)
}

/// The limits of the calling extension.
fn limits(ctx: &CallContext) -> Result<Limits, BridgeError> {
    let extension_id = ctx.session.extension_id;
    ctx.db
        .read_blocking(move |q| limits_of(q, extension_id))
        .map_err(|_| BridgeError::new(ExtensionErrorCode::Database, "database unavailable"))
}

/// The largest message the extension's answer can hold (attachments travel as base64).
fn max_bytes(ctx: &CallContext) -> Result<usize, BridgeError> {
    Ok(usize::try_from(limits(ctx)?.max_response_bytes / 4 * 3).unwrap_or(usize::MAX))
}

/// `{imap, reference?, pattern?, includeStatus?}` → `MailboxInfo[]`.
pub fn list_mailboxes(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let config = imap_server(ctx, params)?;
    let reference: Option<String> = optional(params, "reference")?;
    let pattern: Option<String> = optional(params, "pattern")?;
    let status = optional::<bool>(params, "includeStatus")?.unwrap_or(false);
    to_json(
        ctx,
        run(imap::list_mailboxes(
            &config,
            reference.as_deref(),
            pattern.as_deref(),
            status,
        ))?,
    )
}

/// `{imap, mailbox, range}` → `MessageEnvelope[]`.
pub fn fetch_envelopes(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let config = imap_server(ctx, params)?;
    let mailbox: String = field(params, "mailbox")?;
    let range: FetchRange = field(params, "range")?;
    to_json(ctx, run(imap::fetch_envelopes(&config, &mailbox, &range))?)
}

/// `{imap, mailbox, uid}` → `MailMessage`.
pub fn fetch_message(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let config = imap_server(ctx, params)?;
    let mailbox: String = field(params, "mailbox")?;
    let uid: u32 = field(params, "uid")?;
    let max = max_bytes(ctx)?;
    to_json(ctx, run(imap::fetch_message(&config, &mailbox, uid, max))?)
}

/// `{imap, mailbox, uid, partIndex}` → the attachment as base64.
pub fn fetch_attachment(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let config = imap_server(ctx, params)?;
    let mailbox: String = field(params, "mailbox")?;
    let uid: u32 = field(params, "uid")?;
    let part: u32 = field(params, "partIndex")?;
    let max = max_bytes(ctx)?;
    let bytes = run(imap::fetch_attachment(&config, &mailbox, uid, part, max))?;
    Ok(Value::String(STANDARD.encode(bytes)))
}

/// `{imap, mailbox, uids, flags, add}`.
pub fn set_flags(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let config = imap_server(ctx, params)?;
    let mailbox: String = field(params, "mailbox")?;
    let uids: Vec<u32> = field(params, "uids")?;
    let flags: Vec<String> = field(params, "flags")?;
    let add: bool = field(params, "add")?;
    run(imap::set_flags(&config, &mailbox, &uids, &flags, add))?;
    Ok(Value::Null)
}

/// `{imap, sourceMailbox, destinationMailbox, uids}`.
pub fn move_messages(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let config = imap_server(ctx, params)?;
    let source: String = field(params, "sourceMailbox")?;
    let destination: String = field(params, "destinationMailbox")?;
    let uids: Vec<u32> = field(params, "uids")?;
    run(imap::move_messages(&config, &source, &destination, &uids))?;
    Ok(Value::Null)
}

/// `{imap, mailbox, rfc822Base64, flags?}`.
pub fn append_message(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let config = imap_server(ctx, params)?;
    let mailbox: String = field(params, "mailbox")?;
    let encoded: String = field(params, "rfc822Base64")?;
    if encoded.len() as u64 > max_bytes(ctx)? as u64 / 3 * 4 + 4 {
        return Err(BridgeError::new(
            ExtensionErrorCode::LimitExceeded,
            "message too large",
        ));
    }
    let rfc822 = STANDARD.decode(encoded).map_err(|_| {
        BridgeError::new(
            ExtensionErrorCode::Validation,
            "rfc822Base64 must be base64",
        )
    })?;
    let flags: Vec<String> = optional(params, "flags")?.unwrap_or_default();
    run(imap::append_message(&config, &mailbox, &rfc822, &flags))?;
    Ok(Value::Null)
}

/// `{imap, mailbox, criteria, limit?}` → `{uids, total}`: the newest hits (at most `limit`, by
/// default as many as one answer holds), newest first, and how many there are in all.
pub fn search(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let config = imap_server(ctx, params)?;
    let mailbox: String = field(params, "mailbox")?;
    let criteria: SearchCriteria = field(params, "criteria")?;
    let limit = optional::<usize>(params, "limit")?.unwrap_or(MAX_UIDS);
    to_json(
        ctx,
        run(imap_manage::search(&config, &mailbox, &criteria, limit))?,
    )
}

/// `{imap, mailbox}` → `{created}`; `created` is `false`, and no error, when the mailbox was there.
pub fn create_mailbox(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let config = imap_server(ctx, params)?;
    let mailbox: String = field(params, "mailbox")?;
    let created = run(imap_manage::create_mailbox(&config, &mailbox))?;
    Ok(serde_json::json!({ "created": created }))
}

/// `{imap, mailbox, uids}`: removes exactly these messages for good; more than one answer's worth
/// of UIDs is refused (7000).
pub fn delete_messages(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let config = imap_server(ctx, params)?;
    let mailbox: String = field(params, "mailbox")?;
    let uids: Vec<u32> = field(params, "uids")?;
    run(imap_manage::delete_messages(&config, &mailbox, &uids))?;
    Ok(Value::Null)
}

/// `{smtp, message}` → the Message-ID, after the `send` check for the SMTP host and port.
pub fn send_message(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let config: ServerConfig = field(params, "smtp")?;
    let message: OutgoingMessage = field(params, "message")?;
    check_permission(ctx, Action::Send, &config.host, config.port)?;
    Ok(Value::String(run(smtp::send(&config, &message))?))
}

/// `{accountId, mailboxName, intervalSeconds, imap}`: watches the mailbox for new messages, after
/// the `poll` check for the IMAP host and port (`watch.rs`).
pub fn start_watch(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let account: String = field(params, "accountId")?;
    let mailbox: String = field(params, "mailboxName")?;
    let config: ServerConfig = field(params, "imap")?;
    let interval = optional::<u64>(params, "intervalSeconds")?.unwrap_or(300);
    check_permission(ctx, Action::Poll, &config.host, config.port)?;
    super::watch::start(
        &ctx.host,
        &ctx.emitter,
        ctx.session.extension_id,
        account,
        mailbox,
        config,
        Duration::from_secs(interval),
    )
    .map_err(|e| match e {
        MailError::TooLarge => {
            BridgeError::new(ExtensionErrorCode::LimitExceeded, "too many watches")
        }
        other => other.into(),
    })?;
    Ok(Value::Null)
}

/// `{accountId, mailboxName}`: ends a watch of the calling extension; 1001 when it has none.
pub fn stop_watch(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let account: String = field(params, "accountId")?;
    let mailbox: String = field(params, "mailboxName")?;
    if ctx
        .host
        .mail_watches
        .stop(ctx.session.extension_id, &account, &mailbox)
    {
        Ok(Value::Null)
    } else {
        Err(BridgeError::new(ExtensionErrorCode::NotFound, "not found"))
    }
}

/// `{imapHost, message}` → the RFC 822 bytes as base64; nothing is sent.
pub fn build_rfc822(_ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let message: OutgoingMessage = field(params, "message")?;
    let bytes = smtp::build_rfc822(&message).map_err(BridgeError::from)?;
    Ok(Value::String(STANDARD.encode(bytes)))
}
