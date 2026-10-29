//! A catalog action that runs in Rust (ADR 0011, spec 044 research R1). The definition comes from
//! the frontend like every other action (`runner: 'native'`); the call goes to an executor
//! registered at startup instead of through the [`super::action_bridge::ActionBridge`], so it works
//! without a window (FR-034). Source and risk class are those of an
//! [`super::action_tool::ActionTool`]. Each call carries who calls and in which approval mode
//! ([`NativeCall`], spec 065 research R8): the chat passes the built-in agent and the chat's mode,
//! the MCP access an external agent and its own mode.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use futures::future::BoxFuture;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use super::permission::PermissionMode;
use crate::caller::Caller;

use super::action_tool::{error_content, AgentActionDef, ACTION_SOURCE};
use super::{RiskClass, Tool, ToolImage, ToolResult};

/// Why a native action failed, as the model sees it: the same `code`, `field` and `message` an
/// action of the window reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeError {
    pub code: String,
    pub field: Option<String>,
    pub message: String,
}

impl NativeError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            field: None,
            message: message.into(),
        }
    }

    /// Input the executor could not use; `field` names it when one field is at fault.
    pub fn invalid_input(field: Option<&str>, message: impl Into<String>) -> Self {
        Self {
            code: "invalid_input".to_owned(),
            field: field.map(str::to_owned),
            message: message.into(),
        }
    }
}

/// What a native action gives the model: its result, and images to show it (`files.read`).
#[derive(Debug, Clone, PartialEq)]
pub struct NativeReply {
    pub result: Value,
    pub images: Vec<ToolImage>,
    /// What the agent log names as the call's target, when the input does not say it well (the
    /// website of a browser tab, spec 065).
    pub target: Option<String>,
}

impl From<Value> for NativeReply {
    fn from(result: Value) -> Self {
        Self {
            result,
            images: Vec::new(),
            target: None,
        }
    }
}

/// Who calls a native action, and in which approval mode. Set by the entrance (the chat or the MCP
/// access), never taken from the input (ADR 0007).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeCall {
    pub caller: Caller,
    pub mode: PermissionMode,
}

impl NativeCall {
    /// A call of the built-in agent in `mode`.
    pub fn builtin(mode: PermissionMode) -> Self {
        Self {
            caller: Caller::BuiltinAgent,
            mode,
        }
    }
}

/// Refuses a call that is not the built-in agent's, for executors that serve only it so far (the
/// file and share actions until spec 021 PR E gives external agents their own).
pub fn builtin_only(call: &NativeCall) -> Result<(), NativeError> {
    if call.caller == Caller::BuiltinAgent {
        Ok(())
    } else {
        Err(NativeError::new(
            "forbidden_for_agents",
            "This action is not available to this agent.",
        ))
    }
}

/// Reads the chat's approval mode when a call of the built-in agent starts.
pub type ModeSource = Arc<dyn Fn() -> BoxFuture<'static, PermissionMode> + Send + Sync>;

/// Runs the native actions of one area (`files.*`). The caller comes with each call
/// ([`NativeCall`]), never from the input (ADR 0007).
#[async_trait]
pub trait NativeExecutor: Send + Sync {
    /// The action ids it runs.
    fn action_ids(&self) -> &'static [&'static str];

    /// The action ids for which the executor performs confirmation itself on every call.
    fn confirming_action_ids(&self) -> &'static [&'static str] {
        &[]
    }

    async fn run(
        &self,
        action_id: &str,
        input: Value,
        call: &NativeCall,
        cancel: &CancellationToken,
    ) -> Result<NativeReply, NativeError>;
}

/// The executors of this process by action id; filled in `setup()`. Cheap to clone.
#[derive(Clone, Default)]
pub struct NativeExecutors {
    by_id: Arc<Mutex<HashMap<&'static str, Arc<dyn NativeExecutor>>>>,
    mode: Arc<Mutex<Option<ModeSource>>>,
}

impl NativeExecutors {
    /// Where calls of the built-in agent read the chat's approval mode; set in `setup()`.
    pub fn set_mode_source(&self, source: ModeSource) {
        *self.mode.lock().unwrap_or_else(PoisonError::into_inner) = Some(source);
    }

    /// The chat's approval mode now; Manuell, the safest, when no source is set.
    pub async fn builtin_mode(&self) -> PermissionMode {
        let source = self
            .mode
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        match source {
            Some(source) => source().await,
            None => PermissionMode::Manual,
        }
    }

    /// Adds `executor` under each of its ids; a later one wins an id.
    pub fn add(&self, executor: Arc<dyn NativeExecutor>) {
        let mut by_id = self.by_id.lock().unwrap_or_else(PoisonError::into_inner);
        for id in executor.action_ids() {
            by_id.insert(id, executor.clone());
        }
    }

    pub fn get(&self, action_id: &str) -> Option<Arc<dyn NativeExecutor>> {
        self.by_id
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(action_id)
            .cloned()
    }

    /// Every id with an executor.
    pub fn ids(&self) -> Vec<&'static str> {
        self.by_id
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .keys()
            .copied()
            .collect()
    }
}

pub struct NativeActionTool {
    def: AgentActionDef,
    executor: Arc<dyn NativeExecutor>,
    executors: NativeExecutors,
}

impl NativeActionTool {
    pub fn new(
        def: AgentActionDef,
        executor: Arc<dyn NativeExecutor>,
        executors: NativeExecutors,
    ) -> Self {
        Self {
            def,
            executor,
            executors,
        }
    }
}

#[async_trait]
impl Tool for NativeActionTool {
    fn name(&self) -> &str {
        &self.def.tool_name
    }

    fn description(&self) -> &str {
        &self.def.description
    }

    fn source(&self) -> &'static str {
        ACTION_SOURCE
    }

    fn input_schema(&self) -> Value {
        self.def.input_schema.clone()
    }

    fn risk_class(&self) -> RiskClass {
        self.def.effect.risk_class()
    }

    fn action_definition(&self) -> Option<&AgentActionDef> {
        Some(&self.def)
    }

    fn confirms_itself(&self) -> bool {
        self.def.confirms_itself
            && self
                .executor
                .confirming_action_ids()
                .contains(&self.def.action_id.as_str())
    }

    async fn execute(&self, input: Value, cancel: CancellationToken) -> ToolResult {
        let call = NativeCall::builtin(self.executors.builtin_mode().await);
        let run = self
            .executor
            .run(&self.def.action_id, input, &call, &cancel);
        let outcome = tokio::select! {
            biased;
            _ = cancel.cancelled() => return ToolResult::error("tool_call_cancelled"),
            outcome = run => outcome,
        };
        match outcome {
            Ok(reply) => ToolResult::ok(reply.result.to_string()).with_images(reply.images),
            Err(error) => ToolResult::error(error_content(
                &error.code,
                error.field.as_deref(),
                &error.message,
            )),
        }
    }
}
