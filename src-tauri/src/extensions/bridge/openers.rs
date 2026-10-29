//! The bridge method of the openers (spec 063, contracts/bridge.md): an extension takes the one
//! request a person gave it. It needs no permission, because registering for a kind of link grants
//! no access (FR-010) and the request exists only because the person chose this extension (FR-005).

use serde_json::Value;

use super::dispatch::CallContext;
use super::methods::manifest_of;
use crate::extensions::bundle::OpenerKind;
use crate::extensions::error::BridgeError;
use crate::openers::requests::answer_of;

pub const MODULE: &str = module_path!();

/// `null`, or `{action, mail}` of the calling extension's own request, which is gone after this
/// call. An extension that did not register for mail links, or has nothing waiting, hears `null`
/// alike: it learns nothing about requests that are not for it.
pub fn take(ctx: &CallContext, _params: &Value) -> Result<Value, BridgeError> {
    if !manifest_of(ctx)?.opens(OpenerKind::MailCompose) {
        return Ok(Value::Null);
    }
    Ok(ctx
        .host
        .openers
        .take(ctx.session.extension_id)
        .map_or(Value::Null, |request| answer_of(&request)))
}
