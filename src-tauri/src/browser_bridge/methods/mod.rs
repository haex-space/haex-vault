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
use crate::caller::Caller;
use crate::passwords::access::GrantAction;

/// What a method gets: the bridge, the caller, the approval in force and the request's payload.
pub struct Call<'a> {
    pub core: &'a Arc<BridgeCore>,
    pub caller: Caller,
    pub approval: Approval,
    pub payload: Value,
    pub cancel: &'a CancellationToken,
}

pub type Outcome = Result<Value, ErrorCode>;
pub type Handler = for<'a> fn(Call<'a>) -> Pin<Box<dyn Future<Output = Outcome> + Send + 'a>>;

/// One method: its name on the wire, the kind of grant it needs, its handler and the module the
/// handler lives in, which the contract test checks against the wall around the model (FR-041).
pub struct Method {
    pub name: &'static str,
    pub need: GrantAction,
    pub handler: Handler,
    pub module: &'static str,
}

/// The methods of the password manager (FR-018). Filled by the deliveries of US2 to US4.
pub const METHODS: &[Method] = &[];

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
    let approval = match core.decide(client).await {
        Ok(Decision::Authorized { approval, .. }) => approval,
        Ok(_) => return Reply::fail(request_id, ErrorCode::PermissionDenied),
        Err(error) => {
            log::warn!("browser bridge: could not read the approval: {error}");
            return Reply::fail(request_id, ErrorCode::ServerError);
        }
    };
    let call = Call {
        core,
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
