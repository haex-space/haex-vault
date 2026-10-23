//! The round trip between a tool call in Rust and the action runner in the webview (spec 032,
//! research R2, ADR-0006).
//!
//! A model calls an [`super::action_tool::ActionTool`]; [`ActionBridge::call`] emits
//! `action-call-request`, parks on `prompt_park.rs` and waits for the frontend to answer through
//! the `respond_action_call` command ([`ActionBridge::resolve`]). The park brings the timeout
//! (nobody may be listening) and the race against the turn's cancellation token; the bridge adds a
//! lock that runs the actions of one round one after the other (`execute_plans` starts the calls of
//! a round together, and two window actions must not race).

use std::sync::Arc;
use std::time::Duration;

use serde::Deserialize;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::ChoiceOption;
use crate::adapters::cli_delegate::EventEmitter;
use crate::prompt_park::{PromptEnd, PromptPark};

/// Event a model's action call goes out on (contracts/tauri-commands.md).
pub const EVENT_ACTION_CALL_REQUEST: &str = "action-call-request";

/// How long to wait for the webview. Tab-bound actions wait up to 5 s for their app to open;
/// actions such as a model download must not be cut short for nothing.
pub const DEFAULT_ACTION_TIMEOUT: Duration = Duration::from_secs(60);

/// Shown to the model instead of the message of a handler that threw: that message can carry
/// paths or other internals (spec 032 FR-006).
pub const ACTION_FAILED_MESSAGE: &str = "The action failed.";

/// What the frontend answers for one action call (`respond_action_call`). Plain JSON, never the
/// raw error a handler threw: the frontend strips it before it sends.
#[derive(Debug, Clone, PartialEq)]
pub enum ActionReply {
    Ok {
        result: Value,
    },
    Err {
        code: String,
        field: Option<String>,
        message: String,
    },
    /// The action could not resolve `field` and offers candidates (`needs_choice`, spec 046).
    NeedsChoice {
        field: String,
        message: String,
        options: Vec<ChoiceOption>,
    },
}

impl ActionReply {
    /// Creates a bridge error without an input-field association.
    fn error(code: &str, message: &str) -> Self {
        Self::Err {
            code: code.to_owned(),
            field: None,
            message: message.to_owned(),
        }
    }
}

/// The answer as it travels over the command (contracts/tauri-commands.md
/// `ActionOutcomeWire`): `{ ok: true, result }` or `{ ok: false, code, field?, message }`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionOutcomeWire {
    pub ok: bool,
    #[serde(default)]
    pub result: Option<Value>,
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub field: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
    /// The candidates of `needs_choice`.
    #[serde(default)]
    pub options: Option<Vec<ChoiceOption>>,
}

impl From<ActionOutcomeWire> for ActionReply {
    /// Converts a frontend outcome, defaulting absent results to null and hiding handler errors.
    fn from(wire: ActionOutcomeWire) -> Self {
        if wire.ok {
            return Self::Ok {
                result: wire.result.unwrap_or(Value::Null),
            };
        }
        let code = wire.code.unwrap_or_else(|| "failed".to_owned());
        // A choice needs the field the answer goes into; without one it is an ordinary error.
        if code == "needs_choice" {
            if let Some(field) = wire.field.clone() {
                return Self::NeedsChoice {
                    field,
                    message: wire.message.unwrap_or_default(),
                    options: wire.options.unwrap_or_default(),
                };
            }
        }
        // A failed handler's message is replaced; the other codes carry validation text the model
        // needs to correct its input.
        let message = if code == "failed" {
            ACTION_FAILED_MESSAGE.to_owned()
        } else {
            wire.message.unwrap_or_default()
        };
        Self::Err {
            code,
            field: wire.field,
            message,
        }
    }
}

/// One action call while it waits; the park gives it its request id.
struct ActionCall {
    action_id: String,
    input: Value,
}

/// Shared by every `ActionTool` and by the two commands. Cheap to clone.
#[derive(Clone)]
pub struct ActionBridge {
    park: PromptPark<ActionCall, ActionReply>,
    run_lock: Arc<tokio::sync::Mutex<()>>,
}

impl Default for ActionBridge {
    /// Creates an unattached bridge with the default webview response timeout.
    fn default() -> Self {
        Self::with_timeout(DEFAULT_ACTION_TIMEOUT)
    }
}

impl ActionBridge {
    /// Creates a bridge with no emitter or pending calls and the given response timeout.
    /// The timeout starts after a call acquires the execution lock and emits its request.
    pub fn with_timeout(timeout: Duration) -> Self {
        Self {
            park: PromptPark::new(timeout),
            run_lock: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    /// Set once from `setup()`, where the `AppHandle` exists (`ChatState` is managed without one).
    /// A second call is ignored.
    pub fn set_emitter(&self, emitter: EventEmitter) {
        if self.park.has_emitter() {
            return;
        }
        self.park
            .set_emitter(Arc::new(move |request_id: &str, call: &ActionCall| {
                emitter(
                    EVENT_ACTION_CALL_REQUEST,
                    json!({ "requestId": request_id, "actionId": call.action_id, "input": call.input }),
                );
                true
            }));
    }

    /// Emits one call and waits for the answer. Never fails: a missing emitter, a timeout or a
    /// cancelled turn are replies like any other, so a model sees them as tool errors.
    pub async fn call(
        &self,
        action_id: &str,
        input: Value,
        cancel: &CancellationToken,
    ) -> ActionReply {
        if !self.park.has_emitter() {
            return unavailable_reply();
        }
        // One action at a time: the calls of a round start together, their effects must not.
        let _turn = tokio::select! {
            biased;
            _ = cancel.cancelled() => return cancelled_reply(),
            guard = self.run_lock.lock() => guard,
        };
        let call = ActionCall {
            action_id: action_id.to_owned(),
            input,
        };
        // Whatever ends the wait, the call is gone from the park: a late answer finds nothing.
        match self.park.ask(&call, cancel, |_| {}).await {
            Ok(reply) => reply,
            Err(PromptEnd::Expired) => {
                ActionReply::error("action_timeout", "the action did not answer in time")
            }
            Err(PromptEnd::NoWindow) => unavailable_reply(),
            // The turn was cancelled, or the vault closed while the call was open.
            Err(PromptEnd::Cancelled) => cancelled_reply(),
        }
    }

    /// The frontend's answer. `false` for an unknown or late `request_id`, which is not an error
    /// for the caller (the turn may have been cancelled meanwhile).
    pub fn resolve(&self, request_id: Uuid, reply: ActionReply) -> bool {
        self.park.answer(&request_id.to_string(), reply)
    }

    /// Drops every open call; their waiters end as cancelled. For the vault close.
    pub fn drop_pending(&self) {
        self.park.cancel_all();
    }
}

/// The reply before `setup()` gave the bridge a window to call.
fn unavailable_reply() -> ActionReply {
    ActionReply::error("action_unavailable", "actions are not available yet")
}

/// Builds the cancellation reply shared by cancelled turns and dropped pending calls.
fn cancelled_reply() -> ActionReply {
    ActionReply::error("tool_call_cancelled", "the call was cancelled")
}
