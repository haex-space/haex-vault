//! The tools the agent may be offered (spec 018, FR-003, FR-009, data-model.md "Wirkung je
//! Werkzeug"): from the manifests of installed, enabled extensions and their `agentTool` rows,
//! without starting any extension.

use std::collections::{HashMap, HashSet};

use uuid::Uuid;

use crate::error::Result;
use crate::extensions::bundle::{ManifestTool, ToolEffect};
use crate::extensions::dev;
use crate::extensions::permissions::store::rows_of;
use crate::extensions::registry::list::effective_manifest;
use crate::storage::query::Query;

/// A tool of an extension the agent may be offered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionToolDef {
    pub extension_id: Uuid,
    /// The extension's title (manifest `displayName`, else its name), shown with every call.
    pub extension_title: String,
    /// The manifest `description`: what the extension is for, which the chat tells the model.
    pub extension_description: Option<String>,
    pub tool: ManifestTool,
    /// The name the model calls: `x_<slug>_<tool>`, with a key suffix after the slug for a later
    /// extension of the same slug (research R8).
    pub model_name: String,
    /// Confirmed with "ask": every call needs the user's consent, in every mode.
    pub always_ask: bool,
    /// A development version (US4).
    pub dev: bool,
    /// The first characters of the extension's public key when another offered extension makes
    /// the same slug: named in the description, the results and the confirmation, so the model
    /// and the user can tell the two apart.
    pub namesake_key: Option<String>,
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
    /// The signer's public key (hex), the stable part of the extension's identity.
    pub public_key: String,
    /// The manifest `name`.
    pub name: String,
    /// When the extension was installed (ms); `None` for a development version.
    pub installed_at: Option<i64>,
    pub title: String,
    /// The manifest `description`.
    pub description: Option<String>,
    /// Empty for an extension that is installed but not offered here (switched off, or not
    /// ready on this device): it offers nothing, but still holds its name (research R8).
    pub tools: Vec<ManifestTool>,
    pub rows: Vec<ToolRow>,
    pub dev: bool,
}

const AGENT_TOOL: &str = "agentTool";
/// Longest slug of an extension name; with a collision suffix and a 32-character tool name the
/// model-facing name stays within 64 characters.
const MAX_SLUG: usize = 20;
const SUFFIX: usize = 6;

/// Which source keeps the bare slug among those sharing it: installed before development
/// versions, then the earliest installed, then the public key, so the result does not depend on
/// the order the sources were read in.
fn seniority(source: &ExtensionSource) -> (bool, i64, &str, Uuid) {
    (
        source.installed_at.is_none(),
        source.installed_at.unwrap_or(i64::MAX),
        source.public_key.as_str(),
        source.id,
    )
}

/// How [`prefixes`] names one source: its slug, its model-facing prefix, and the start of its key.
struct Naming {
    slug: String,
    prefix: String,
    key: String,
}

/// The model-facing prefix of every source (research R8): its slug, and for all but the senior
/// source of a slug the first characters of its public key. The names of an extension stay as
/// they are when another one with the same slug is installed later. A suffix that is still taken
/// (a development version signed with the same key as an installed one, say) falls back to the
/// first characters of the extension id.
fn prefixes(sources: &[ExtensionSource]) -> HashMap<Uuid, Naming> {
    let mut order: Vec<&ExtensionSource> = sources.iter().collect();
    order.sort_by(|a, b| seniority(a).cmp(&seniority(b)));
    let mut taken = HashSet::new();
    let mut prefixes = HashMap::new();
    for source in order {
        let slug = slug(&source.name);
        let key: String = source
            .public_key
            .chars()
            .filter(|c| c.is_ascii_hexdigit())
            .map(|c| c.to_ascii_lowercase())
            .take(SUFFIX)
            .collect();
        let id = source.id.simple().to_string()[..SUFFIX].to_owned();
        let prefix = [slug.clone(), format!("{slug}{key}"), format!("{slug}{id}")]
            .into_iter()
            .find(|candidate| !taken.contains(candidate))
            .unwrap_or_else(|| format!("{slug}{id}"));
        taken.insert(prefix.clone());
        prefixes.insert(source.id, Naming { slug, prefix, key });
    }
    prefixes
}

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
        .map(|c| c.to_ascii_lowercase())
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
    let prefixes = prefixes(&sources);
    let mut defs = Vec::new();
    for source in sources {
        if source
            .rows
            .iter()
            .any(|r| r.target == "*" && r.status == "denied")
        {
            continue;
        }
        let naming = &prefixes[&source.id];
        for tool in &source.tools {
            let Some(always_ask) = confirmation(tool, &source.rows) else {
                continue;
            };
            defs.push(ExtensionToolDef {
                extension_id: source.id,
                extension_title: source.title.clone(),
                extension_description: source.description.clone(),
                model_name: format!("x_{}_{}", naming.prefix, tool.name),
                tool: tool.clone(),
                always_ask,
                dev: source.dev,
                namesake_key: None,
            });
        }
    }
    // Only extensions that offer something here are namesakes: a switched-off one keeps its name
    // but is nothing the model could mix up.
    let mut offering: HashMap<&str, HashSet<Uuid>> = HashMap::new();
    for def in &defs {
        offering
            .entry(prefixes[&def.extension_id].slug.as_str())
            .or_default()
            .insert(def.extension_id);
    }
    let shared: HashSet<Uuid> = offering
        .values()
        .filter(|ids| ids.len() > 1)
        .flatten()
        .copied()
        .collect();
    for def in &mut defs {
        if shared.contains(&def.extension_id) {
            def.namesake_key = Some(prefixes[&def.extension_id].key.clone());
        }
    }
    defs
}

/// The agent-tool rows of an extension, installed or in development.
fn tool_rows(q: &mut impl Query, extension_id: Uuid) -> Result<Vec<ToolRow>> {
    Ok(rows_of(q, extension_id)?
        .into_iter()
        .filter(|r| r.kind == AGENT_TOOL)
        .map(|r| ToolRow {
            action: r.action,
            target: r.target,
            status: r.status,
        })
        .collect())
}

/// The development versions registered on `device` while developer mode is on (spec 018 US4,
/// FR-020). Their tools come from the project's manifest as it reads now, so an edit is offered at
/// once; a project that cannot be read any more offers none. Confirmation is the developer's own,
/// from `dev_extension_permissions_no_sync`.
fn dev_sources(q: &mut impl Query, device: Uuid) -> Result<Vec<ExtensionSource>> {
    if !dev::mode(q, device)? {
        return Ok(Vec::new());
    }
    let mut sources = Vec::new();
    for registration in dev::registrations(q, device)? {
        let project = match dev::current_project(&registration) {
            Ok(project) => project,
            Err(error) => {
                log::warn!(
                    "agent tools: development version {} is not offered: {error}",
                    registration.title
                );
                continue;
            }
        };
        let manifest = project.manifest;
        if manifest.tools.is_empty() {
            continue;
        }
        sources.push(ExtensionSource {
            id: registration.id,
            public_key: registration.prefix.public_key.as_str().to_owned(),
            name: manifest.name.as_str().to_owned(),
            installed_at: None,
            title: manifest.title().to_owned(),
            description: manifest.description.clone(),
            rows: tool_rows(q, registration.id)?,
            tools: manifest.tools,
            dev: true,
        });
    }
    Ok(sources)
}

/// The offerable tools of the vault on `device`: installed, enabled extensions whose state here
/// lets them start (not yet started, or ready), and the development versions of this device.
/// Installed extensions that are switched off or not ready here offer nothing but keep their
/// names, so another extension of the same slug never takes them over in the meantime.
pub fn load(q: &mut impl Query, device: Uuid) -> Result<Vec<ExtensionToolDef>> {
    let rows = q.query_map(
        "SELECT e.id, e.public_key, e.installed_at, \
           e.enabled = 1 AND (s.status IS NULL OR s.status = 'ready') \
         FROM extensions e \
         LEFT JOIN extension_device_status s \
           ON s.extension_id = e.id AND s.vault_device_uuid = ?1 \
         WHERE e.state = 'installed' \
         ORDER BY e.name",
        &[&device.to_string()],
        |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, bool>(3)?,
            ))
        },
    )?;
    let mut sources = Vec::new();
    for (id, public_key, installed_at, offered) in rows {
        let Ok(id) = Uuid::parse_str(&id) else {
            continue;
        };
        let Some((_, manifest)) = effective_manifest(q, id)? else {
            continue;
        };
        if manifest.tools.is_empty() {
            continue;
        }
        let name = manifest.name.as_str().to_owned();
        let title = manifest.title().to_owned();
        let description = manifest.description.clone();
        let (tools, rows) = if offered {
            (manifest.tools, tool_rows(q, id)?)
        } else {
            (Vec::new(), Vec::new())
        };
        sources.push(ExtensionSource {
            id,
            public_key,
            name,
            installed_at: Some(installed_at),
            title,
            description,
            rows,
            tools,
            dev: false,
        });
    }
    sources.extend(dev_sources(q, device)?);
    Ok(tool_defs(sources))
}
