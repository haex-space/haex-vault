//! The tools the agent may be offered (spec 018, FR-003, FR-009, data-model.md "Wirkung je
//! Werkzeug"): from the manifests of installed, enabled extensions and their `agentTool` rows,
//! without starting any extension.

use std::collections::HashMap;

use uuid::Uuid;

use crate::error::Result;
use crate::extensions::bundle::{ManifestTool, ToolEffect};
use crate::extensions::permissions::store::rows_of;
use crate::extensions::registry::list::effective_manifest;
use crate::storage::query::Query;

/// A tool of an extension the agent may be offered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionToolDef {
    pub extension_id: Uuid,
    /// The extension's title (manifest `displayName`, else its name), shown with every call.
    pub extension_title: String,
    pub tool: ManifestTool,
    /// The name the model calls: `x_<slug>_<tool>` (research R8).
    pub model_name: String,
    /// Confirmed with "ask": every call needs the user's consent, in every mode.
    pub always_ask: bool,
    /// A development version (US4).
    pub dev: bool,
}

/// One `agentTool` row of an extension.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolRow {
    pub action: String,
    pub target: String,
    pub status: String,
}

/// What [`tool_defs`] reads about one extension.
#[derive(Debug, Clone)]
pub struct ExtensionSource {
    pub id: Uuid,
    /// The manifest `name`.
    pub name: String,
    pub title: String,
    pub tools: Vec<ManifestTool>,
    pub rows: Vec<ToolRow>,
    pub dev: bool,
}

const AGENT_TOOL: &str = "agentTool";
/// Longest slug of an extension name; with a collision suffix and a 32-character tool name the
/// model-facing name stays within 64 characters.
const MAX_SLUG: usize = 20;
const SUFFIX: usize = 6;

fn effect_action(effect: ToolEffect) -> &'static str {
    match effect {
        ToolEffect::Read => "read",
        ToolEffect::Change => "change",
        ToolEffect::Destructive => "destructive",
    }
}

fn slug(name: &str) -> String {
    let slug: String = name
        .chars()
        .filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        .take(MAX_SLUG)
        .collect();
    if slug.is_empty() {
        "ext".to_owned()
    } else {
        slug
    }
}

/// Whether `tool` is offered, and whether it always asks; `None` when it is not offered.
fn confirmation(tool: &ManifestTool, rows: &[ToolRow]) -> Option<bool> {
    if !tool.supported {
        return None;
    }
    let action = effect_action(tool.effect);
    let row = rows
        .iter()
        .find(|r| r.target == tool.name && r.action == action)?;
    match row.status.as_str() {
        "granted" => Some(false),
        "ask" => Some(true),
        _ => None,
    }
}

/// The offerable tools of `sources`, in their order and manifest order.
pub fn tool_defs(sources: Vec<ExtensionSource>) -> Vec<ExtensionToolDef> {
    let mut per_slug: HashMap<String, usize> = HashMap::new();
    for source in &sources {
        *per_slug.entry(slug(&source.name)).or_default() += 1;
    }
    let mut defs = Vec::new();
    for source in sources {
        if source
            .rows
            .iter()
            .any(|r| r.target == "*" && r.status == "denied")
        {
            continue;
        }
        let mut prefix = slug(&source.name);
        if per_slug.get(&prefix).copied().unwrap_or(0) > 1 {
            prefix.push_str(&source.id.simple().to_string()[..SUFFIX]);
        }
        for tool in &source.tools {
            let Some(always_ask) = confirmation(tool, &source.rows) else {
                continue;
            };
            defs.push(ExtensionToolDef {
                extension_id: source.id,
                extension_title: source.title.clone(),
                model_name: format!("x_{prefix}_{}", tool.name),
                tool: tool.clone(),
                always_ask,
                dev: source.dev,
            });
        }
    }
    defs
}

/// The offerable tools of the vault on `device`: installed, enabled extensions whose state here
/// lets them start (not yet started, or ready).
pub fn load(q: &mut impl Query, device: Uuid) -> Result<Vec<ExtensionToolDef>> {
    let ids = q.query_map(
        "SELECT e.id FROM extensions e \
         LEFT JOIN extension_device_status s \
           ON s.extension_id = e.id AND s.vault_device_uuid = ?1 \
         WHERE e.state = 'installed' AND e.enabled = 1 \
           AND (s.status IS NULL OR s.status = 'ready') \
         ORDER BY e.name",
        &[&device.to_string()],
        |r| r.get::<_, String>(0),
    )?;
    let mut sources = Vec::new();
    for id in ids {
        let Ok(id) = Uuid::parse_str(&id) else {
            continue;
        };
        let Some((_, manifest)) = effective_manifest(q, id)? else {
            continue;
        };
        if manifest.tools.is_empty() {
            continue;
        }
        let rows = rows_of(q, id)?
            .into_iter()
            .filter(|r| r.kind == AGENT_TOOL)
            .map(|r| ToolRow {
                action: r.action,
                target: r.target,
                status: r.status,
            })
            .collect();
        sources.push(ExtensionSource {
            id,
            name: manifest.name.as_str().to_owned(),
            title: manifest.title().to_owned(),
            tools: manifest.tools,
            rows,
            dev: false,
        });
    }
    Ok(tool_defs(sources))
}
