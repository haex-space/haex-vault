//! The bridge methods of channels (spec 057 T061–T064, contracts/bridge.md §Methoden): the eight
//! `extension_peers_*` calls of an extension frame. Each one is for the calling extension, taken
//! from the frame's session and never from the parameters, and asks for `peers`/`connect`/`*`
//! before anything leaves the device (FR-002). The work itself is in [`invite`], [`lobby`],
//! [`join`] and [`PeerState`](super::PeerState); this file reads the parameters and maps the
//! result. An id of another extension answers like an unknown one (FR-006).
//!
//! There is no method to accept or decline an invitation: that happens only in holzi's prompt
//! (FR-016).

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use serde_json::{json, Value};
use uuid::Uuid;

use super::limits::{FAST_BYTES, RELIABLE_BYTES};
use super::lobby::Visibility;
use super::{invite, join, lobby, permission_decision, ExtensionRef, PeerError};
use crate::extensions::bridge::blocking::block_on;
use crate::extensions::bridge::dispatch::CallContext;
use crate::extensions::error::{BridgeError, ExtensionErrorCode};
use crate::extensions::permissions::store::candidates;
use crate::extensions::permissions::{Decision, PermissionKind};

pub const MODULE: &str = module_path!();

/// The longest id of a contact that is looked up; the address book's ids are far shorter.
const MAX_CONTACT_ID: usize = 128;

fn invalid() -> BridgeError {
    PeerError::InvalidInput.into()
}

/// `peers`/`connect`/`*` for the calling extension, remembered or held for the session. Denied
/// is 1002, undecided 1004 with the question's `details`, which holzi puts before the user.
fn check_permission(ctx: &CallContext) -> Result<(), BridgeError> {
    let (extension_id, device) = (ctx.session.extension_id, ctx.device);
    let mut grants = ctx
        .db
        .read_blocking(move |q| {
            candidates(q, extension_id, PermissionKind::Peers, device).map_err(Into::into)
        })
        .map_err(|_| BridgeError::new(ExtensionErrorCode::Database, "database unavailable"))?;
    grants.extend(
        ctx.host
            .permissions
            .temporary(extension_id, PermissionKind::Peers),
    );
    let code = match permission_decision(&grants, device) {
        Decision::Allow => return Ok(()),
        Decision::Deny => ExtensionErrorCode::PermissionDenied,
        Decision::Prompt => ExtensionErrorCode::PermissionPromptRequired,
    };
    Err(
        BridgeError::new(code, "permission required").with_details(json!({
            "resourceType": "peers",
            "action": "connect",
            "target": "*",
        })),
    )
}

/// The calling extension as the other side names it on the wire.
fn caller(ctx: &CallContext) -> Result<ExtensionRef, BridgeError> {
    let extension_id = ctx.session.extension_id;
    ctx.db
        .read_blocking(move |q| ExtensionRef::of(q, extension_id).map_err(Into::into))
        .map_err(|_| BridgeError::new(ExtensionErrorCode::Database, "database unavailable"))?
        .ok_or_else(|| BridgeError::new(ExtensionErrorCode::NotFound, "not found"))
}

fn text<'a>(params: &'a Value, key: &str) -> Result<&'a str, BridgeError> {
    params.get(key).and_then(Value::as_str).ok_or_else(invalid)
}

/// An id the extension got from holzi. One that is no id at all is unreadable input; a readable
/// one of another extension is looked up like any other (FR-006).
fn id(params: &Value, key: &str) -> Result<Uuid, BridgeError> {
    Uuid::parse_str(text(params, key)?).map_err(|_| invalid())
}

/// The JSON content of an invitation or a join; the size is checked where it is sent. A call
/// without one carries `null`.
fn payload(params: &Value) -> Value {
    params.get("payload").cloned().unwrap_or(Value::Null)
}

/// The bytes of `data`, sent as `{"$bytes": "<base64>"}` (017): more than `limit` is `too_large`,
/// refused by the length of the text before anything is decoded.
fn bytes(params: &Value, limit: usize) -> Result<Vec<u8>, BridgeError> {
    let encoded = params
        .get("data")
        .and_then(Value::as_object)
        .filter(|object| object.len() == 1)
        .and_then(|object| object.get("$bytes"))
        .and_then(Value::as_str)
        .ok_or_else(invalid)?;
    let too_large = || BridgeError::from(PeerError::TooLarge { limit });
    if encoded.len() > limit.div_ceil(3) * 4 {
        return Err(too_large());
    }
    let data = STANDARD.decode(encoded).map_err(|_| invalid())?;
    if data.len() > limit {
        return Err(too_large());
    }
    Ok(data)
}

/// `{contactId, payload}` → `{inviteId}`. Returns at once; the outcome follows as
/// `peers:invite-status`.
pub fn invite(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    check_permission(ctx)?;
    let contact_id = text(params, "contactId")?;
    if contact_id.is_empty() || contact_id.len() > MAX_CONTACT_ID {
        return Err(invalid());
    }
    let extension = caller(ctx)?;
    let invite = block_on(invite::invite(
        &ctx.host.peers,
        ctx.session.extension_id,
        extension,
        contact_id,
        &payload(params),
    ))?;
    Ok(json!({ "inviteId": invite.to_string() }))
}

/// `{inviteId}` → `{}`. No status follows a withdrawal.
pub fn invite_cancel(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    check_permission(ctx)?;
    let invite = id(params, "inviteId")?;
    invite::cancel(&ctx.host.peers, ctx.session.extension_id, invite)?;
    Ok(json!({}))
}

/// `{lobbyId, visibility}` → `{joinCode}`.
pub fn lobby_open(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    check_permission(ctx)?;
    let visibility = match text(params, "visibility")? {
        "contacts" => Visibility::Contacts,
        "public" => Visibility::Public,
        _ => return Err(invalid()),
    };
    let lobby_id = text(params, "lobbyId")?;
    let extension = caller(ctx)?;
    let code = lobby::open(
        &ctx.host.peers,
        ctx.session.extension_id,
        extension,
        lobby_id,
        visibility,
    )?;
    Ok(json!({ "joinCode": code }))
}

/// `{lobbyId}` → `{}`; an unknown lobby is no error.
pub fn lobby_close(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    check_permission(ctx)?;
    let lobby_id = text(params, "lobbyId")?;
    lobby::close(&ctx.host.peers, ctx.session.extension_id, lobby_id);
    Ok(json!({}))
}

/// `{joinCode, displayName, payload}` → `{channelId, lobbyId}`. Waits at most 15 s for the host.
pub fn join(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    check_permission(ctx)?;
    let code = text(params, "joinCode")?;
    let display_name = text(params, "displayName")?;
    let extension = caller(ctx)?;
    let joined = block_on(join::join(
        &ctx.host.peers,
        ctx.session.extension_id,
        extension,
        code,
        display_name,
        &payload(params),
    ))?;
    Ok(json!({
        "channelId": joined.channel.id.to_string(),
        "lobbyId": joined.lobby_id,
    }))
}

/// `{channelId}` → `{}`: closes a channel from one of this extension's own lobbies and bans its
/// device (FR-027).
pub fn remove(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    check_permission(ctx)?;
    let channel = id(params, "channelId")?;
    lobby::remove(&ctx.host.peers, ctx.session.extension_id, channel)?;
    Ok(json!({}))
}

/// `{channelId, data, reliable?}` → `{}`.
///
/// ponytail: one bridge call per message, fast ones included (research R8), each with its own
/// permission read. Upgrade: a call that carries several messages, if the delay of SC-004 is
/// missed.
pub fn send(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    check_permission(ctx)?;
    let channel = id(params, "channelId")?;
    let reliable = match params.get("reliable") {
        None | Some(Value::Null) => false,
        Some(Value::Bool(reliable)) => *reliable,
        Some(_) => return Err(invalid()),
    };
    let channel = ctx.host.peers.channel(ctx.session.extension_id, channel)?;
    if reliable {
        channel.send_reliable(bytes(params, RELIABLE_BYTES)?)?;
    } else {
        channel.send_fast(&bytes(params, FAST_BYTES)?)?;
    }
    Ok(json!({}))
}

/// `{channelId}` → `{}`. Both sides hear `peers:channel-closed` with `closed`, this one too.
pub fn close(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    check_permission(ctx)?;
    let channel = id(params, "channelId")?;
    ctx.host.peers.close(ctx.session.extension_id, channel)?;
    Ok(json!({}))
}

#[cfg(test)]
#[path = "bridge_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "bridge_flow_tests.rs"]
mod flow_tests;
