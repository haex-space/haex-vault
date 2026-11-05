//! The areas of holzi a request can be about (spec 082 FR-010, research R4, contracts/domains.md):
//! the domains the frontend declares for its actions (`AGENT_DOMAINS`), and one per installed
//! extension. A routing call names a request's domains, and the first step offers their tools.

use serde::Deserialize;

use super::action_tool::ActionTitles;
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

/// The installed domains: every declared domain that has a tool in `registry`, in the declared
/// order, then one per extension in the registry's order. An extension is described by its tools'
/// titles (the manifests have no description to rely on); its name is its prefix unless a
/// declared domain already uses that word; it also needs [`OPEN_APP_TOOL`].
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
        let taken = prefix == NO_DOMAIN || out.iter().any(|domain| domain.name == prefix);
        out.push(Domain {
            key: key.clone(),
            name: if taken {
                key.clone()
            } else {
                prefix.to_owned()
            },
            description: format!("{}: {}", origin.extension_name, titles.join(", ")),
            also_needs: vec![OPEN_APP_TOOL.to_owned()],
        });
    }
    out
}
