//! Curated action offers and the bounded action search (spec 032, R6).
//!
//! The registry keeps every callable action so a search result can execute, but
//! the model only sees the fixed core plus `find_actions` initially. Search
//! results replace the previous non-core additions for the next step.

use serde_json::Value;

use crate::adapters::types::ToolSpec;

use super::action_tool::{AgentActionDef, ACTION_SOURCE};
use super::ask_user::ASK_USER_TOOL_NAME;
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

/// Keeps all non-action tools and only the fixed core action tools plus search and `ask_user`.
pub fn core_offer(registry: &ToolRegistry) -> Vec<ToolSpec> {
    registry
        .iter()
        .filter(|tool| {
            tool.source() != ACTION_SOURCE
                || tool.name() == FIND_ACTIONS_TOOL_NAME
                || tool.name() == ASK_USER_TOOL_NAME
                || tool.action_definition().is_some_and(|def| def.core)
        })
        .map(|tool| tool_spec(tool.as_ref()))
        .collect()
}

/// Replaces earlier search additions with the latest result while retaining
/// the core action tools and every non-action tool.
pub fn extend_offer(
    request_tools: &mut Vec<ToolSpec>,
    found: &[ToolSpec],
    action_defs: &[AgentActionDef],
) {
    let core_names: std::collections::HashSet<&str> = action_defs
        .iter()
        .filter(|def| def.core)
        .map(|def| def.tool_name.as_str())
        .collect();
    let action_names: std::collections::HashSet<&str> = action_defs
        .iter()
        .map(|def| def.tool_name.as_str())
        .collect();

    request_tools.retain(|tool| {
        !action_names.contains(tool.name.as_str()) || core_names.contains(tool.name.as_str())
    });

    // One slot each stays for `find_actions` and `ask_user`.
    let max_found = MAX_ACTION_OFFER.saturating_sub(core_names.len() + 2);
    let mut added = 0;
    for tool in found {
        if added >= max_found
            || core_names.contains(tool.name.as_str())
            || !action_names.contains(tool.name.as_str())
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

fn search_score(def: &AgentActionDef, query: &[String]) -> usize {
    let haystack = [
        def.action_id.as_str(),
        def.tool_name.as_str(),
        def.description.as_str(),
        def.titles.de.as_str(),
        def.titles.en.as_str(),
    ]
    .into_iter()
    .flat_map(words)
    .collect::<std::collections::HashSet<_>>();
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

/// Searches all registered built-in actions, with stable id ordering for ties.
/// The cursor is an opaque decimal offset owned by this implementation.
pub fn search_actions(
    defs: &[AgentActionDef],
    query: Option<&str>,
    cursor: Option<&str>,
    limit: usize,
) -> (Vec<AgentActionDef>, Option<String>) {
    let offset = cursor
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let mut ranked: Vec<(usize, &AgentActionDef)> = defs
        .iter()
        .filter_map(|def| {
            let score = query
                .map(|text| search_score(def, &words(text)))
                .unwrap_or(1);
            (score > 0).then_some((score, def))
        })
        .collect();
    ranked.sort_by(|(left_score, left), (right_score, right)| {
        right_score
            .cmp(left_score)
            .then_with(|| left.action_id.cmp(&right.action_id))
    });

    let page_size = limit.clamp(1, MAX_SEARCH_RESULTS);
    let total = ranked.len();
    let page = ranked
        .into_iter()
        .skip(offset)
        .take(page_size)
        .map(|(_, def)| def.clone())
        .collect::<Vec<_>>();
    let next = (offset + page.len() < total).then(|| (offset + page.len()).to_string());
    (page, next)
}

pub fn action_result(def: &AgentActionDef) -> Value {
    serde_json::json!({
        "tool": def.tool_name,
        "description": def.description,
        "inputSchema": def.input_schema,
    })
}
