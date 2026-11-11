//! Curated action offers and the bounded action search (spec 032, R6).
//!
//! The registry keeps every callable action so a search result can execute, but
//! the model only sees the fixed core plus `find_actions` initially. Search
//! results replace the previous non-core additions the turn has not called for the next step.

use std::collections::HashSet;

use serde_json::Value;

use crate::adapters::types::ToolSpec;

use super::action_tool::{AgentActionDef, ACTION_SOURCE};
use super::ask_user::ASK_USER_TOOL_NAME;
use super::availability::ToolAvailability;
use super::domains::{domain_of, Domain};
use super::extension_offer::{matched_offer_in, EXTENSION_SOURCE};
use super::{RiskClass, Tool, ToolRegistry};

pub const FIND_ACTIONS_TOOL_NAME: &str = "find_actions";
/// One page of the search, and the most hits one search adds to the next step. Eight, since the
/// few that fit the words are rarely the ones a step needs (the browser's tools for a page were
/// missed at five).
pub const MAX_SEARCH_RESULTS: usize = 8;
/// What one step offers at most: the core (11 today), `find_actions`, `ask_user`, the command tool,
/// the tools kept for the turn and a page of search hits. Thirty leaves room for the tools that are
/// still to come; a small model does better with fewer, which `MAX_DOMAIN_TOOLS` and the routing
/// of spec 082 keep in view.
pub const MAX_ACTION_OFFER: usize = 30;

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
    first_offer_in(availability, registry, &[], message)
}

/// [`first_offer`] for a message in a conversation: the user's `earlier` messages count for the
/// matched extension tools (spec 018, amendment 2026-11-09).
pub fn first_offer_in(
    availability: ToolAvailability,
    registry: &ToolRegistry,
    earlier: &[String],
    message: &str,
) -> Vec<ToolSpec> {
    if !availability.offers_tools() {
        return Vec::new();
    }
    let mut offer = core_offer(registry);
    for name in matched_offer_in(earlier, message, &registry.match_entries()) {
        if let Some(tool) = registry.get(&name) {
            offer.push(tool_spec(tool.as_ref()));
        }
    }
    offer
}

/// The most characters of earlier user messages the first offer reads: the matching compares every
/// word with every tool, so a very long conversation is bounded, newest messages first.
pub const EARLIER_CONTEXT_CHARS: usize = 6000;

/// The user's messages before the current one, oldest first, for [`first_offer_in`] and the
/// routing call (spec 018, amendment 2026-11-09). `users` are all the user messages of the
/// conversation, oldest first, the current one last. Messages are kept from the newest back until
/// [`EARLIER_CONTEXT_CHARS`] is reached; the one that crosses the limit is left out whole.
pub fn earlier_user_messages(users: &[&str]) -> Vec<String> {
    let Some((_, before)) = users.split_last() else {
        return Vec::new();
    };
    let mut kept: Vec<String> = Vec::new();
    let mut chars = 0;
    for text in before.iter().rev() {
        chars += text.chars().count();
        if chars > EARLIER_CONTEXT_CHARS {
            break;
        }
        kept.push((*text).to_owned());
    }
    kept.reverse();
    kept
}

/// The extension tools a search may add (spec 018, research R7), and the tools that stay for the
/// whole turn: those its first step offered and those it called (#840).
#[derive(Debug, Default)]
pub struct ExtensionScope {
    pub names: HashSet<String>,
    pub pinned: HashSet<String>,
}

/// Replaces earlier search additions with the latest result while retaining the core action
/// tools, the scope's pinned tools and every other tool.
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

    // Count retained tools in the actual offer; called results can already use the search budget.
    let max_found = MAX_SEARCH_RESULTS.min(MAX_ACTION_OFFER.saturating_sub(request_tools.len()));
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

/// The browser tools that open a page and read it.
pub const PAGE_OPENERS: [&str; 2] = ["browser_open", "browser_search"];
/// What works on an open page. They are no core tools (the core stays small for small models), but
/// a model that has opened a page has to see them without searching for them: asked to "press play"
/// it otherwise answers that it cannot click.
pub const PAGE_TOOLS: [&str; 5] = [
    "browser_page_read",
    "browser_click",
    "browser_type",
    "browser_scroll",
    "browser_follow_link",
];

/// After a successful page opener, adds the tools that work on the page to `request_tools` and
/// returns the names it added; the caller keeps them for the rest of the turn (they are pinned
/// against later searches). A tool the registry lacks or the offer already has is skipped. Which
/// of them the turn may call is decided per step by the agent profile as for every other tool.
pub fn page_tools_after(
    registry: &ToolRegistry,
    request_tools: &mut Vec<ToolSpec>,
    called: &[(&str, bool)],
) -> Vec<String> {
    let opened = called
        .iter()
        .any(|(name, failed)| !failed && PAGE_OPENERS.contains(name));
    if !opened {
        return Vec::new();
    }
    let mut added = Vec::new();
    let remaining = MAX_ACTION_OFFER.saturating_sub(request_tools.len());
    for name in PAGE_TOOLS.into_iter().take(remaining) {
        if request_tools.iter().any(|tool| tool.name == name) {
            continue;
        }
        if let Some(tool) = registry.get(name) {
            request_tools.push(tool_spec(tool.as_ref()));
            added.push(name.to_owned());
        }
    }
    added
}

/// After a successful search, widens `request_tools` with the `found` tools as a chat turn does: the
/// registry's actions and extension tools are searchable, the `kept` tools stay (the turn's first
/// offer and the tools it called; spec 082 research R2: shared with the everyday measurement).
pub fn widen_offer(
    registry: &ToolRegistry,
    request_tools: &mut Vec<ToolSpec>,
    found: &[ToolSpec],
    kept: &HashSet<String>,
) {
    let scope = ExtensionScope {
        names: registry
            .match_entries()
            .into_iter()
            .map(|entry| entry.tool_name)
            .collect(),
        pinned: kept.clone(),
    };
    extend_offer(request_tools, found, &registry.action_defs(), &scope);
}

/// Adds the tool search to an offer after its first step, once, for an offer that lost it (a
/// first step offers it, operator 2026-11-11, amending spec 087 research R6.3). It comes on top of
/// [`MAX_ACTION_OFFER`], which bounds the first step. Returns whether it was added.
pub fn with_search(registry: &ToolRegistry, request_tools: &mut Vec<ToolSpec>) -> bool {
    if request_tools
        .iter()
        .any(|tool| tool.name == FIND_ACTIONS_TOOL_NAME)
    {
        return false;
    }
    let Some(search) = registry.get(FIND_ACTIONS_TOOL_NAME) else {
        return false;
    };
    request_tools.push(tool_spec(search.as_ref()));
    true
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

/// Lowercase words, also split where camelCase turns upper; umlauts belong to the word ("Einträge").
pub(crate) fn words(text: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut word = String::new();
    let mut previous_lower = false;
    for character in text.chars() {
        if !character.is_alphanumeric() {
            if !word.is_empty() {
                result.push(std::mem::take(&mut word));
            }
            previous_lower = false;
        } else {
            if character.is_uppercase() && previous_lower && !word.is_empty() {
                result.push(std::mem::take(&mut word));
            }
            word.extend(character.to_lowercase());
            previous_lower = character.is_lowercase();
        }
    }
    if !word.is_empty() {
        result.push(word);
    }
    result
}

/// At most this many tools of one routed domain (spec 082 research R5).
pub const MAX_DOMAIN_TOOLS: usize = 12;

/// What a message is compared with for one tool: an action's search texts, an extension tool's
/// titles, description and examples.
fn texts_of(tool: &dyn Tool) -> Vec<String> {
    if let Some(def) = tool.action_definition() {
        return SearchEntry::action(def).texts;
    }
    tool.match_entry()
        .map(|entry| entry.texts)
        .unwrap_or_else(|| vec![tool.name().to_owned(), tool.description().to_owned()])
}

/// The tools of the domain `key` for `message`: all of a small domain; of a larger one its reading
/// tools first, then the others by the words they share with the message, core actions before the
/// rest, up to [`MAX_DOMAIN_TOOLS`].
fn domain_tools<'a>(registry: &'a ToolRegistry, key: &str, message: &str) -> Vec<&'a dyn Tool> {
    let tools: Vec<&dyn Tool> = registry
        .iter()
        .map(|tool| tool.as_ref())
        .filter(|tool| domain_of(*tool).as_deref() == Some(key))
        .collect();
    if tools.len() <= MAX_DOMAIN_TOOLS {
        return tools;
    }
    let query = words(message);
    let mut ranked: Vec<(bool, usize, bool, usize, &dyn Tool)> = tools
        .into_iter()
        .enumerate()
        .map(|(position, tool)| {
            let reads = tool.risk_class() == RiskClass::Safe;
            let score = search_score(&texts_of(tool), &query);
            let core = tool.action_definition().is_some_and(|def| def.core);
            (reads, score, core, position, tool)
        })
        .collect();
    ranked.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then(b.1.cmp(&a.1))
            .then(b.2.cmp(&a.2))
            .then(a.3.cmp(&b.3))
    });
    ranked
        .into_iter()
        .take(MAX_DOMAIN_TOOLS)
        .map(|(.., tool)| tool)
        .collect()
}

/// The first step's offer for a request routed to `routed` (spec 082 FR-005, FR-007, research R5):
/// the tools of each domain, what the domains also need, the tool search, `ask_user`, and the tools
/// of no domain (the command tool, MCP) as today. The domains' part, with the search and
/// `ask_user`, stays within [`MAX_ACTION_OFFER`]. No routed domain gives today's [`first_offer`].
pub fn routed_offer(
    availability: ToolAvailability,
    registry: &ToolRegistry,
    domains: &[Domain],
    routed: &[String],
    message: &str,
) -> Vec<ToolSpec> {
    routed_offer_in(availability, registry, domains, routed, &[], message)
}

/// [`routed_offer`] for a message in a conversation (`earlier`: the user's earlier messages).
pub fn routed_offer_in(
    availability: ToolAvailability,
    registry: &ToolRegistry,
    domains: &[Domain],
    routed: &[String],
    earlier: &[String],
    message: &str,
) -> Vec<ToolSpec> {
    let routed: Vec<&Domain> = routed
        .iter()
        .filter_map(|key| domains.iter().find(|domain| domain.key == *key))
        .collect();
    if !availability.offers_tools() || routed.is_empty() {
        return first_offer_in(availability, registry, earlier, message);
    }
    let mut extra: Vec<&str> = Vec::new();
    for name in routed
        .iter()
        .flat_map(|domain| domain.also_needs.iter().map(String::as_str))
        // The tool search is always offered, so a misrouted request can still find its tools
        // (operator 2026-11-11, amends spec 087 research R6.3).
        .chain([FIND_ACTIONS_TOOL_NAME, ASK_USER_TOOL_NAME])
    {
        if registry.get(name).is_some() && !extra.contains(&name) {
            extra.push(name);
        }
    }
    let budget = MAX_ACTION_OFFER.saturating_sub(extra.len());
    let mut names: Vec<&str> = Vec::new();
    for domain in &routed {
        for tool in domain_tools(registry, &domain.key, message) {
            if names.len() < budget
                && !names.contains(&tool.name())
                && !extra.contains(&tool.name())
            {
                names.push(tool.name());
            }
        }
    }
    names.extend(extra);
    // The command tool and MCP tools belong to no domain; they come as in today's offer.
    let mut offer: Vec<ToolSpec> = names
        .iter()
        .filter_map(|name| registry.get(name))
        .map(|tool| tool_spec(tool.as_ref()))
        .collect();
    offer.extend(
        registry
            .iter()
            .filter(|tool| tool.source() != ACTION_SOURCE && tool.source() != EXTENSION_SOURCE)
            .map(|tool| tool_spec(tool.as_ref())),
    );
    offer
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
    shorter == longer || (shorter.chars().count() >= MIN_PREFIX_LEN && longer.starts_with(shorter))
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
