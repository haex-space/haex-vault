//! An action of the window manager or the settings as a tool of the built-in agent (spec 032,
//! ADR-0006). The frontend owns the definitions (`src/lib/actions/`) and pushes them with
//! `set_agent_actions`; one [`ActionTool`] per definition lives in the [`super::ToolRegistry`]
//! under the source `action`. Running it goes through the [`ActionBridge`] to the same action
//! runner a click uses.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use super::action_bridge::{ActionBridge, ActionReply};
use super::{ChoiceRequest, RiskClass, Tool, ToolResult};

/// Registry `source` of these tools, persisted into `chat_messages.tool_source`.
pub const ACTION_SOURCE: &str = "action";

/// What an action does to the user's data (spec 020 `ActionEffect`), which decides its risk class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ActionEffect {
    Read,
    Write,
    Destructive,
}

impl ActionEffect {
    /// `read` only looks, `write` changes something the user can change back, `destructive` does
    /// not come back.
    pub fn risk_class(self) -> RiskClass {
        match self {
            Self::Read => RiskClass::Safe,
            Self::Write => RiskClass::Change,
            Self::Destructive => RiskClass::Risky,
        }
    }
}

/// Localized titles the search of `find_actions` looks through next to id and description.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct ActionTitles {
    #[serde(default)]
    pub de: String,
    #[serde(default)]
    pub en: String,
}

/// Where an action runs (ADR 0011): in the window, or in Rust without one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ActionRunner {
    Native,
}

/// One action as the frontend pushes it (data-model.md §2).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentActionDef {
    /// The action id with `.` replaced by `_`; unique within the registry.
    pub tool_name: String,
    pub action_id: String,
    pub description: String,
    pub input_schema: Value,
    pub effect: ActionEffect,
    /// Part of the fixed core offer every model sees (research R6).
    #[serde(default)]
    pub core: bool,
    #[serde(default)]
    pub titles: ActionTitles,
    /// `native`: a Rust executor runs it ([`super::native_action`]); absent: the window does.
    #[serde(default)]
    pub runner: Option<ActionRunner>,
    /// The permission scope (spec 020 FR-031); external agents get rights per scope (spec 021).
    #[serde(default)]
    pub scope: String,
    /// `false`: only external agents may call it; it would re-enter the running chat turn
    /// (ADR-0006, `builtinAgentCallable`).
    #[serde(default = "builtin_by_default")]
    pub builtin_agent_callable: bool,
}

fn builtin_by_default() -> bool {
    true
}

pub struct ActionTool {
    def: AgentActionDef,
    bridge: ActionBridge,
}

impl ActionTool {
    /// Binds an action definition to the bridge that will dispatch its calls to the webview.
    pub fn new(def: AgentActionDef, bridge: ActionBridge) -> Self {
        Self { def, bridge }
    }

    /// The definition, for the offer and the search (`core` and `titles`).
    pub fn def(&self) -> &AgentActionDef {
        &self.def
    }
}

#[async_trait]
impl Tool for ActionTool {
    /// Returns the provider-facing tool name supplied in the action definition.
    fn name(&self) -> &str {
        &self.def.tool_name
    }

    /// Returns the catalog description offered to the model.
    fn description(&self) -> &str {
        &self.def.description
    }

    /// Identifies this tool as an app action in persisted chat messages.
    fn source(&self) -> &'static str {
        ACTION_SOURCE
    }

    /// Copies the action's input schema for the model's tool definition.
    fn input_schema(&self) -> Value {
        self.def.input_schema.clone()
    }

    /// Maps the action's effect to the risk class used by the permission gate.
    fn risk_class(&self) -> RiskClass {
        self.def.effect.risk_class()
    }

    fn action_definition(&self) -> Option<&AgentActionDef> {
        Some(&self.def)
    }

    /// Dispatches through the bridge and converts its reply into model-visible tool content.
    async fn execute(&self, input: Value, cancel: CancellationToken) -> ToolResult {
        let reply = self
            .bridge
            .call(&self.def.action_id, input.clone(), &cancel)
            .await;
        into_tool_result(reply, &input)
    }
}

/// Bridge-level failures come back as the plain markers the tool loop already uses
/// (`denied_by_user`, `tool_call_cancelled`); everything the runner reports is a small JSON the
/// model can act on: `code`, the offending `field` and a `message`.
fn into_tool_result(reply: ActionReply, input: &Value) -> ToolResult {
    match reply {
        ActionReply::Ok { result } => ToolResult::ok(result.to_string()),
        ActionReply::Err { code, .. }
            if matches!(
                code.as_str(),
                "action_timeout" | "action_unavailable" | "tool_call_cancelled"
            ) =>
        {
            ToolResult::error(code)
        }
        ActionReply::Err {
            code,
            field,
            message,
        } => ToolResult::error(error_content(&code, field.as_deref(), &message)),
        // The tool round asks the user; the JSON is what a caller without a chat would see.
        ActionReply::NeedsChoice {
            field,
            message,
            options,
        } => {
            let content = json!({ "error": {
                "code": "needs_choice",
                "message": message,
                "field": field,
                "options": options,
            }})
            .to_string();
            let value = match input.get(&field) {
                Some(Value::String(text)) => text.clone(),
                Some(other) => other.to_string(),
                None => String::new(),
            };
            ToolResult::needs_choice(
                content,
                ChoiceRequest {
                    question: None,
                    field: Some(field),
                    value,
                    options,
                },
            )
        }
    }
}

/// What the model sees of a failed action: `{"error": {code, message, field?}}`.
pub(super) fn error_content(code: &str, field: Option<&str>, message: &str) -> String {
    let mut error = json!({ "code": code, "message": message });
    if let Some(field) = field {
        error["field"] = Value::String(field.to_owned());
    }
    json!({ "error": error }).to_string()
}

/// Whether a schema uses only what the action runner's validator knows (`src/lib/actions/schema.ts`):
/// `type`, `description`, `properties`, `required`, `items`, `enum`.
pub fn schema_in_subset(schema: &Value) -> bool {
    let Some(object) = schema.as_object() else {
        return false;
    };
    let allowed = [
        "type",
        "description",
        "properties",
        "required",
        "items",
        "enum",
    ];
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return false;
    }
    let type_ok = matches!(
        object.get("type").and_then(Value::as_str),
        Some("object" | "string" | "number" | "integer" | "boolean" | "array")
    );
    let properties_ok = object.get("properties").is_none_or(|properties| {
        properties
            .as_object()
            .is_some_and(|map| map.values().all(schema_in_subset))
    });
    let items_ok = object.get("items").is_none_or(schema_in_subset);
    type_ok && properties_ok && items_ok
}

/// Tool names keep to what providers accept: `^[A-Za-z0-9_-]{1,64}$` (research R3).
pub fn is_valid_tool_name(name: &str) -> bool {
    (1..=64).contains(&name.len())
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
