//! The safe meta-tool that searches the complete built-in action catalog and the extension tools
//! the agent may use (spec 018, research R7).

use std::sync::{Arc, Mutex, Weak};

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;
use tokio_util::sync::CancellationToken;

use super::action_tool::{ActionEffect, AgentActionDef, ACTION_SOURCE};
use super::offer::{search_entries, tool_spec, SearchEntry, MAX_SEARCH_RESULTS};
use super::{RiskClass, Tool, ToolRegistry, ToolResult};

pub const FIND_ACTIONS_TOOL_NAME: &str = super::offer::FIND_ACTIONS_TOOL_NAME;

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct FindActionsInput {
    query: Option<String>,
    cursor: Option<String>,
    limit: Option<usize>,
}

pub struct FindActionsTool {
    actions: Vec<SearchEntry>,
    /// Where the extension tools are read at each search; weak, since this tool lives in that
    /// registry. They change while the vault is open, the actions only with the frontend.
    extensions: Option<Weak<Mutex<ToolRegistry>>>,
}

impl FindActionsTool {
    pub fn new(defs: Vec<AgentActionDef>) -> Self {
        Self {
            actions: defs.iter().map(SearchEntry::action).collect(),
            extensions: None,
        }
    }

    /// Also searches the extension tools of `registry`.
    pub fn with_extensions(defs: Vec<AgentActionDef>, registry: &Arc<Mutex<ToolRegistry>>) -> Self {
        Self {
            extensions: Some(Arc::downgrade(registry)),
            ..Self::new(defs)
        }
    }

    fn entries(&self) -> Vec<SearchEntry> {
        let mut entries = self.actions.clone();
        let Some(registry) = self.extensions.as_ref().and_then(Weak::upgrade) else {
            return entries;
        };
        let registry = registry.lock().unwrap_or_else(|e| e.into_inner());
        entries.extend(registry.iter().filter_map(|tool| {
            let entry = tool.match_entry()?;
            let mut texts = entry.texts;
            texts.push(entry.tool_name.clone());
            texts.push(entry.extension_title);
            Some(SearchEntry {
                key: entry.tool_name,
                tool: tool_spec(tool.as_ref()),
                texts,
            })
        }));
        entries
    }
}

#[async_trait]
impl Tool for FindActionsTool {
    fn name(&self) -> &str {
        FIND_ACTIONS_TOOL_NAME
    }

    fn description(&self) -> &str {
        "Search holzi actions by keywords, or enumerate the catalog when query is omitted. Matching actions become tools in the next step."
    }

    fn source(&self) -> &'static str {
        ACTION_SOURCE
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "query": { "type": "string" },
                "cursor": { "type": "string" },
                "limit": { "type": "integer" },
            },
        })
    }

    fn risk_class(&self) -> RiskClass {
        ActionEffect::Read.risk_class()
    }

    async fn execute(&self, input: serde_json::Value, _cancel: CancellationToken) -> ToolResult {
        let input = match serde_json::from_value::<FindActionsInput>(input) {
            Ok(input) => input,
            Err(_) => return ToolResult::error("invalid_input"),
        };
        let entries = self.entries();
        let (found, next_cursor) = search_entries(
            &entries,
            input.query.as_deref(),
            input.cursor.as_deref(),
            input.limit.unwrap_or(MAX_SEARCH_RESULTS),
        );
        let mut result = json!({
            "actions": found.iter().map(|entry| entry.result()).collect::<Vec<_>>(),
            "nextCursor": next_cursor,
        });
        if found.is_empty() && input.query.is_some() {
            result["hint"] = json!(
                "no action matches these keywords; this is about actions, not about the user's data. \
                 Try other keywords, e.g. in English, or omit query to list every action."
            );
        }
        ToolResult::ok(result.to_string())
    }
}
