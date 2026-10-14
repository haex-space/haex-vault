//! Curated action offers and the bounded action search (spec 032, R6).
//!
//! The registry keeps every callable action so a search result can execute, but
//! the model only sees the fixed core plus `find_actions` initially. Search
//! results replace the previous non-core additions for the next step.

use std::collections::HashSet;

use serde_json::Value;

use crate::adapters::types::ToolSpec;

use super::action_tool::{AgentActionDef, ACTION_SOURCE};
use super::ask_user::ASK_USER_TOOL_NAME;
use super::availability::ToolAvailability;
use super::extension_offer::{matched_offer, EXTENSION_SOURCE};
use super::{Tool, ToolRegistry};

pub const FIND_ACTIONS_TOOL_NAME: &str = "find_actions";
pub const MAX_SEARCH_RESULTS: usize = 5;
/// Core (at most 9), `find_actions`, `ask_user` and a full page of search hits.
pub const MAX_ACTION_OFFER: usize = 16;

pub fn tool_spec(tool: &dyn Tool) -> ToolSpec {
    ToolSpec {
        name: tool.name().to_string(),
        description: tool.description().to_string(),
        input_schema: tool.input_schema(),
    }
}

/// Keeps all other tools and only the fixed core action tools plus search and `ask_user`; an
/// extension's tool only comes with a message it fits ([`first_offer`]).
pub fn core_offer(registry: &ToolRegistry) -> Vec<ToolSpec> {
    registry
        .iter()
        .filter(|tool| tool.source() != EXTENSION_SOURCE)
        .filter(|tool| {
            tool.source() != ACTION_SOURCE
                || tool.name() == FIND_ACTIONS_TOOL_NAME
                || tool.name() == ASK_USER_TOOL_NAME
                || tool.action_definition().is_some_and(|def| def.core)
        })
        .map(|tool| tool_spec(tool.as_ref()))
        .collect()
}

/// The first step's offer: the core plus up to three extension tools that fit `message` (spec
/// 018, research R6), at most 14 tools; nothing for a model that cannot use tools or a delegate
/// turn (FR-017). The matched tools stay for every step, since [`extend_offer`] keeps them.
pub fn first_offer(
    availability: ToolAvailability,
    registry: &ToolRegistry,
    message: &str,
) -> Vec<ToolSpec> {
    if !availability.offers_tools() {
        return Vec::new();
    }
    let mut offer = core_offer(registry);
    for name in matched_offer(message, &registry.match_entries()) {
        if let Some(tool) = registry.get(&name) {
            offer.push(tool_spec(tool.as_ref()));
        }
    }
    offer
}

/// The extension tools a search may add (spec 018, research R7), and the tools the turn's first
/// step offered, which stay for the whole turn.
#[derive(Debug, Default)]
pub struct ExtensionScope {
    pub names: HashSet<String>,
    pub pinned: HashSet<String>,
}

/// Replaces earlier search additions with the latest result while retaining the core action
/// tools, the first step's tools and every other tool.
pub fn extend_offer(
    request_tools: &mut Vec<ToolSpec>,
    found: &[ToolSpec],
    action_defs: &[AgentActionDef],
    extensions: &ExtensionScope,
) {
    let core_names: HashSet<&str> = action_defs
        .iter()
        .filter(|def| def.core)
        .map(|def| def.tool_name.as_str())
        .collect();
    let action_names: HashSet<&str> = action_defs
        .iter()
        .map(|def| def.tool_name.as_str())
        .collect();
    let searchable = |name: &str| {
        (action_names.contains(name) && !core_names.contains(name))
            || extensions.names.contains(name)
    };

    request_tools.retain(|tool| !searchable(&tool.name) || extensions.pinned.contains(&tool.name));

    // One slot each stays for `find_actions` and `ask_user`.
    let max_found = MAX_ACTION_OFFER.saturating_sub(core_names.len() + 2);
    let mut added = 0;
    for tool in found {
        if added >= max_found
            || !searchable(&tool.name)
            || request_tools
                .iter()
                .any(|existing| existing.name == tool.name)
        {
            continue;
        }
        request_tools.push(tool.clone());
        added += 1;
    }
}

/// Extracts the `find_actions` result into adapter tool specs for the next
/// request. Invalid results are ignored so the prior offer remains available.
pub fn found_tools(result: &str) -> Option<Vec<ToolSpec>> {
    let value = serde_json::from_str::<Value>(result).ok()?;
    let actions = value.get("actions")?.as_array()?;
    Some(
        actions
            .iter()
            .filter_map(|action| {
                Some(ToolSpec {
                    name: action.get("tool")?.as_str()?.to_owned(),
                    description: action.get("description")?.as_str()?.to_owned(),
                    input_schema: action.get("inputSchema")?.clone(),
                })
            })
            .collect(),
    )
}

fn words(text: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut word = String::new();
    let mut previous_lower = false;
    for character in text.chars() {
        if !character.is_ascii_alphanumeric() {
            if !word.is_empty() {
                result.push(std::mem::take(&mut word));
            }
            previous_lower = false;
        } else {
            if character.is_ascii_uppercase() && previous_lower && !word.is_empty() {
                result.push(std::mem::take(&mut word));
            }
            word.push(character.to_ascii_lowercase());
            previous_lower = character.is_ascii_lowercase();
        }
    }
    if !word.is_empty() {
        result.push(word);
    }
    result
}

/// One searchable tool: a built-in action or an extension's tool (spec 018, research R7).
#[derive(Debug, Clone)]
pub struct SearchEntry {
    /// Orders ties: the action id, or the tool name.
    pub key: String,
    pub tool: ToolSpec,
    /// What a query is compared with.
    pub texts: Vec<String>,
}

impl SearchEntry {
    pub fn action(def: &AgentActionDef) -> Self {
        Self {
            key: def.action_id.clone(),
            tool: ToolSpec {
                name: def.tool_name.clone(),
                description: def.description.clone(),
                input_schema: def.input_schema.clone(),
            },
            texts: vec![
                def.action_id.clone(),
                def.tool_name.clone(),
                def.description.clone(),
                def.titles.de.clone(),
                def.titles.en.clone(),
            ],
        }
    }

    /// The search result of this tool, the same form for actions and extension tools.
    pub fn result(&self) -> Value {
        serde_json::json!({
            "tool": self.tool.name,
            "description": self.tool.description,
            "inputSchema": self.tool.input_schema,
        })
    }
}

fn search_score(texts: &[String], query: &[String]) -> usize {
    let haystack = texts
        .iter()
        .flat_map(|text| words(text))
        .collect::<HashSet<_>>();
    query
        .iter()
        .filter(|word| haystack.iter().any(|known| word_matches(word, known)))
        .count()
}

/// Shortest word that may match as a prefix, so "extension" finds "extensions" and
/// "Erweiterung" finds "Erweiterungen" without "set" finding "settings".
const MIN_PREFIX_LEN: usize = 5;

pub(super) fn word_matches(query: &str, known: &str) -> bool {
    let (shorter, longer) = if query.len() <= known.len() {
        (query, known)
    } else {
        (known, query)
    };
    shorter == longer || (shorter.len() >= MIN_PREFIX_LEN && longer.starts_with(shorter))
}

/// Searches `entries`, with stable key ordering for ties. The cursor is an opaque decimal offset
/// owned by this implementation.
pub fn search_entries<'a>(
    entries: &'a [SearchEntry],
    query: Option<&str>,
    cursor: Option<&str>,
    limit: usize,
) -> (Vec<&'a SearchEntry>, Option<String>) {
    let offset = cursor
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let query = query.map(words);
    let mut ranked: Vec<(usize, &SearchEntry)> = entries
        .iter()
        .filter_map(|entry| {
            let score = query
                .as_ref()
                .map(|query| search_score(&entry.texts, query))
                .unwrap_or(1);
            (score > 0).then_some((score, entry))
        })
        .collect();
    ranked.sort_by(|(left_score, left), (right_score, right)| {
        right_score
            .cmp(left_score)
            .then_with(|| left.key.cmp(&right.key))
    });

    let page_size = limit.clamp(1, MAX_SEARCH_RESULTS);
    let total = ranked.len();
    let page = ranked
        .into_iter()
        .skip(offset)
        .take(page_size)
        .map(|(_, entry)| entry)
        .collect::<Vec<_>>();
    let next = (offset + page.len() < total).then(|| (offset + page.len()).to_string());
    (page, next)
}

/// Searches all registered built-in actions, with stable id ordering for ties.
pub fn search_actions(
    defs: &[AgentActionDef],
    query: Option<&str>,
    cursor: Option<&str>,
    limit: usize,
) -> (Vec<AgentActionDef>, Option<String>) {
    let entries: Vec<SearchEntry> = defs.iter().map(SearchEntry::action).collect();
    let (page, next) = search_entries(&entries, query, cursor, limit);
    let page = page
        .into_iter()
        .filter_map(|entry| defs.iter().find(|def| def.action_id == entry.key).cloned())
        .collect();
    (page, next)
}
