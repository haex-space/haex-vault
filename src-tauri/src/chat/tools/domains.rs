//! The areas of holzi a request can be about (spec 082 FR-010, research R4, contracts/domains.md):
//! the domains the frontend declares for its actions (`AGENT_DOMAINS`), and one per installed
//! extension. A routing call names a request's domains, and the first step offers their tools.

use serde::Deserialize;

use super::action_tool::ActionTitles;
use super::offer::words;
use super::{Tool, ToolRegistry};

/// The answer of the routing call for a request no domain fits.
pub const NO_DOMAIN: &str = "none";

/// The window action that opens an app; an extension is an app, so its domain also needs it
/// ("open my mail" routed to the mail extension).
pub const OPEN_APP_TOOL: &str = "wm_app_open";

/// One domain as the frontend declares it (`AgentDomain` in `src/lib/actions/agentTools.ts`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentDomain {
    /// The first segment of the action ids it covers.
    pub key: String,
    /// The word the routing prompt names it by.
    pub name: String,
    #[serde(default)]
    pub titles: ActionTitles,
    pub description: String,
    /// Tools outside the domain that its chains need.
    #[serde(default)]
    pub also_needs: Vec<String>,
}

/// One installed domain, as the routing prompt and the routed offer use it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Domain {
    /// `contacts` for holzi's contacts actions, `x_<prefix>` for an extension.
    pub key: String,
    /// The word the routing prompt uses; unique among the installed domains.
    pub name: String,
    pub description: String,
    pub also_needs: Vec<String>,
}

/// The part of an action id before the first dot (`contacts.search` → `contacts`).
pub fn action_domain(action_id: &str) -> &str {
    action_id.split('.').next().unwrap_or(action_id)
}

/// The prefix of an extension tool's name `x_<prefix>_<tool>`.
fn extension_prefix(tool_name: &str) -> Option<&str> {
    tool_name.strip_prefix("x_")?.split('_').next()
}

/// The domain `tool` belongs to: an action by its id's first segment, an extension's tool by its
/// extension. The search, `ask_user`, the command tool and MCP tools belong to none.
pub fn domain_of(tool: &dyn Tool) -> Option<String> {
    if let Some(def) = tool.action_definition() {
        return Some(action_domain(&def.action_id).to_owned());
    }
    tool.origin()?;
    extension_prefix(tool.name()).map(|prefix| format!("x_{prefix}"))
}

/// Words of a tool title that say what a tool does or join words, not what it is about (words
/// under three letters never count).
const NOT_A_TOPIC: [&str; 24] = [
    "the", "and", "for", "from", "with", "into", "all", "list", "create", "delete", "remove",
    "find", "search", "read", "get", "set", "add", "move", "mark", "send", "open", "show",
    "update", "import",
];

/// What an extension's tools are about: the word most of their `titles` share, singular and
/// plural counted as one, shown as written most often (the shorter on a tie); the earlier word
/// when two words share as many titles. `None` when no
/// word is in two titles. The routing prompt names the extension's domain by it, so a request
/// reaches the extension whatever the extension is called (haex-mail, Thunderbird).
pub fn topic_word(titles: &[String]) -> Option<String> {
    let mut stems: Vec<Stem> = Vec::new();
    for title in titles {
        let mut seen: Vec<String> = Vec::new();
        for word in words(title) {
            if word.chars().count() < 3 || NOT_A_TOPIC.contains(&word.as_str()) {
                continue;
            }
            let stem = word.strip_suffix('s').unwrap_or(&word).to_owned();
            let index = match stems.iter().position(|known| known.stem == stem) {
                Some(index) => index,
                None => {
                    stems.push(Stem {
                        stem: stem.clone(),
                        titles: 0,
                        spellings: Vec::new(),
                    });
                    stems.len() - 1
                }
            };
            let entry = &mut stems[index];
            if !seen.contains(&stem) {
                seen.push(stem);
                entry.titles += 1;
            }
            match entry
                .spellings
                .iter_mut()
                .find(|(spelling, _)| *spelling == word)
            {
                Some(spelling) => spelling.1 += 1,
                None => entry.spellings.push((word, 1)),
            }
        }
    }
    let best = stems.iter().map(|stem| stem.titles).max()?;
    let stem = stems.iter().find(|stem| stem.titles == best)?;
    if stem.titles < 2 {
        return None;
    }
    stem.spellings
        .iter()
        .max_by(|(a, n), (b, m)| n.cmp(m).then(b.len().cmp(&a.len())))
        .map(|(spelling, _)| spelling.clone())
}

/// One word of [`topic_word`], singular and plural as one: the titles it is in, and how often
/// each spelling occurs.
struct Stem {
    stem: String,
    titles: usize,
    spellings: Vec<(String, usize)>,
}

/// How many example requests of an extension's tools its domain shows. Its tools' titles say what it can do,
/// not what a person asks: with two examples a small model routes a request that names no tool word ("Was
/// schreibt die Hausverwaltung?") to the mail extension far more often (Qwen3.5 4B: 25 to 39 of 52 routing
/// probes, 2026-11-11). English like the rest of the prompt; German examples did no better.
const EXAMPLES: usize = 2;

/// The installed domains: every declared domain that has a tool in `registry`, in the declared
/// order, then one per extension in the registry's order. An extension is described by its tools'
/// titles (the manifests have no description to rely on) and the first [`EXAMPLES`] example requests
/// of its tools, and named by their [`topic_word`], else
/// by its prefix; a word another domain already uses falls back to the prefix, then to the key.
/// It also needs [`OPEN_APP_TOOL`].
pub fn domains(registry: &ToolRegistry) -> Vec<Domain> {
    let tools: Vec<(String, &dyn Tool)> = registry
        .iter()
        .filter_map(|tool| domain_of(tool.as_ref()).map(|key| (key, tool.as_ref())))
        .collect();
    let mut out: Vec<Domain> = registry
        .declared_domains()
        .iter()
        .filter(|declared| tools.iter().any(|(key, _)| *key == declared.key))
        .map(|declared| Domain {
            key: declared.key.clone(),
            name: declared.name.clone(),
            description: declared.description.clone(),
            also_needs: declared.also_needs.clone(),
        })
        .collect();
    for (key, tool) in &tools {
        if !key.starts_with("x_") || out.iter().any(|domain| domain.key == *key) {
            continue;
        }
        let Some(origin) = tool.origin() else {
            continue;
        };
        let titles: Vec<String> = tools
            .iter()
            .filter(|(other, _)| other == key)
            .filter_map(|(_, other)| other.origin())
            .map(|origin| origin.tool_title_en.to_lowercase())
            .collect();
        let prefix = key.trim_start_matches("x_");
        let free = |word: &str| word != NO_DOMAIN && !out.iter().any(|domain| domain.name == word);
        let name = topic_word(&titles)
            .filter(|topic| free(topic))
            .or_else(|| free(prefix).then(|| prefix.to_owned()))
            .unwrap_or_else(|| key.clone());
        let examples: Vec<String> = tools
            .iter()
            .filter(|(other, _)| other == key)
            .filter_map(|(_, other)| other.origin()?.example_en)
            .take(EXAMPLES)
            .map(|example| format!("\"{example}\""))
            .collect();
        let mut description = format!("{}: {}", origin.extension_name, titles.join(", "));
        if !examples.is_empty() {
            description.push_str(&format!(" (e.g. {})", examples.join(", ")));
        }
        // One line per domain in the routing prompt: a manifest's text may hold line breaks.
        let description = description.split_whitespace().collect::<Vec<_>>().join(" ");
        out.push(Domain {
            key: key.clone(),
            name,
            description,
            also_needs: vec![OPEN_APP_TOOL.to_owned()],
        });
    }
    out
}
