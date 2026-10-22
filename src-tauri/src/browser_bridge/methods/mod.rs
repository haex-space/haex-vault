//! The methods a client may call (spec 049, FR-018, FR-028, FR-029, research R6,
//! `contracts/wire-protocol.md`). Every method runs with the client as caller and the grant of its
//! approval, read again for every request so a change or a revocation holds at once (FR-038).
//! Unsupported and unknown methods and every target but the password manager are refused without a
//! question and without effect.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use serde_json::Value;
use tokio_util::sync::CancellationToken;

use super::approvals::{Approval, ClientIdentity, Decision};
use super::protocol::{ErrorCode, Reply, RequestEnvelope};
use super::runtime::BridgeCore;
use super::write_access::ensure_write;
use crate::caller::Caller;
use crate::error::HolziError;
use crate::passwords::access::GrantAction;
use crate::passwords::service::PasswordsService;

pub mod items;
pub mod passkeys;
mod presence;

/// What a method gets: the bridge, the client and its caller, the approval in force and the
/// request's payload.
pub struct Call<'a> {
    pub core: &'a Arc<BridgeCore>,
    pub client: &'a ClientIdentity,
    pub caller: Caller,
    pub approval: Approval,
    pub payload: Value,
    pub cancel: &'a CancellationToken,
}

pub type Outcome = Result<Value, ErrorCode>;
pub type BoxedOutcome<'a> = Pin<Box<dyn Future<Output = Outcome> + Send + 'a>>;
pub type Handler = for<'a> fn(Call<'a>) -> BoxedOutcome<'a>;

/// One method: its name on the wire, the kind of grant it needs, its handler and the module the
/// handler lives in, which the contract test checks against the wall around the model (FR-041).
pub struct Method {
    pub name: &'static str,
    pub need: GrantAction,
    pub handler: Handler,
    pub module: &'static str,
}

/// The methods of the password manager (FR-018).
pub const METHODS: &[Method] = &[
    Method {
        name: "get-items",
        need: GrantAction::Read,
        handler: items::get_items,
        module: items::MODULE,
    },
    Method {
        name: "get-totp",
        need: GrantAction::Read,
        handler: items::get_totp,
        module: items::MODULE,
    },
    Method {
        name: "create-item",
        need: GrantAction::ReadWrite,
        handler: items::create_item,
        module: items::MODULE,
    },
    Method {
        name: "get-password-config",
        need: GrantAction::Read,
        handler: items::get_password_config,
        module: items::MODULE,
    },
    Method {
        name: "get-password-presets",
        need: GrantAction::Read,
        handler: items::get_password_presets,
        module: items::MODULE,
    },
    Method {
        name: "passkey-create",
        need: GrantAction::ReadWrite,
        handler: passkeys::passkey_create,
        module: passkeys::MODULE,
    },
    Method {
        name: "passkey-get",
        need: GrantAction::Read,
        handler: passkeys::passkey_get,
        module: passkeys::MODULE,
    },
    Method {
        name: "passkey-list",
        need: GrantAction::Read,
        handler: passkeys::passkey_list,
        module: passkeys::MODULE,
    },
];

/// The password service over the bridge's vault.
fn service_of(core: &BridgeCore) -> PasswordsService {
    PasswordsService::new(core.db.clone())
}

/// A service error as the client hears it: fixed codes, never the service's text (FR-039), and
/// "not found" the same as "outside the scope" (FR-042).
fn to_code(error: HolziError) -> ErrorCode {
    match error {
        HolziError::PasswordsNotFound => ErrorCode::NotFound,
        HolziError::PasswordsForbidden => ErrorCode::PermissionDenied,
        HolziError::InvalidInput { .. } => ErrorCode::InvalidRequest,
        HolziError::PasswordsPasskeyOriginMismatch => ErrorCode::OriginMismatch,
        HolziError::PasswordsPasskeyExcluded => ErrorCode::PasskeyExcluded,
        HolziError::PasswordsPasskeyUnsupportedAlgorithm => ErrorCode::UnsupportedAlgorithm,
        other => {
            log::warn!(
                "browser bridge: the password service failed ({})",
                error_kind(&other)
            );
            ErrorCode::ServerError
        }
    }
}

/// The variant name of an error, without its fields, for a log line without content.
fn error_kind(error: &HolziError) -> String {
    let debug = format!("{error:?}");
    debug
        .split(|c: char| !c.is_alphanumeric())
        .next()
        .unwrap_or("unknown")
        .to_owned()
}

/// Methods the extension knows that holzi answers with `NOT_SUPPORTED` (FR-028).
pub const NOT_SUPPORTED: &[&str] = &[
    "update-item",
    "bookmarks-collections-list",
    "bookmarks-collection-create",
    "bookmarks-list",
    "bookmarks-upsert",
    "bookmarks-delete",
    "bookmarks-device-upsert",
];

/// Answers one request.
pub(super) async fn dispatch(
    core: &Arc<BridgeCore>,
    client: &ClientIdentity,
    request: &RequestEnvelope,
    request_id: String,
    payload: Value,
    cancel: &CancellationToken,
) -> Reply {
    let short = super::connection::short_id(&client.client_id);
    let Some(method) = request
        .targets_core()
        .then(|| METHODS.iter().find(|m| m.name == request.action))
        .flatten()
    else {
        log::info!(
            "browser bridge: {short} called an unsupported method or target ({})",
            if NOT_SUPPORTED.contains(&request.action.as_str()) {
                request.action.as_str()
            } else {
                "unknown"
            }
        );
        return Reply::fail(request_id, ErrorCode::NotSupported);
    };
    let (approval, remembered) = match core.decide(client).await {
        Ok(Decision::Authorized {
            approval,
            remembered,
        }) => (
            core.session().effective(&client.client_id, approval),
            remembered,
        ),
        Ok(_) => return Reply::fail(request_id, ErrorCode::PermissionDenied),
        Err(error) => {
            log::warn!("browser bridge: could not read the approval: {error}");
            return Reply::fail(request_id, ErrorCode::ServerError);
        }
    };
    let approval = if method.need == GrantAction::ReadWrite {
        match ensure_write(core, client, approval, remembered, method.name, cancel).await {
            Ok(approval) => approval,
            Err(code) => {
                log::info!(
                    "browser bridge: {short} {} -> {}",
                    method.name,
                    code.message()
                );
                return Reply::fail(request_id, code);
            }
        }
    } else {
        approval
    };
    let call = Call {
        core,
        client,
        caller: Caller::BrowserClient {
            id: client.client_id.clone(),
        },
        approval,
        payload,
        cancel,
    };
    let outcome = (method.handler)(call).await;
    log::info!(
        "browser bridge: {short} {} -> {}",
        method.name,
        match outcome {
            Ok(_) => "ok",
            Err(code) => code.message(),
        }
    );
    match outcome {
        Ok(data) => Reply::ok(request_id, data),
        Err(code) => Reply::fail(request_id, code),
    }
}

/// The answer when a connection already runs as many requests as it may (research R4).
pub(super) fn busy(request_id: String) -> Reply {
    Reply::fail(request_id, ErrorCode::ServerError)
}
