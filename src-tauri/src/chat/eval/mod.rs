//! Measuring how well a model calls holzi's tools (spec 032 US5, research R9). The set of example
//! sentences, the pure scoring and the runner that asks a model; the in-app self-test and the
//! full measurement (`tests/model_tool_eval.rs`) both go through here. Needs no vault and never
//! runs an action (FR-021, FR-022).

pub mod runner;
pub mod scoring;

#[cfg(test)]
#[path = "runner_tests.rs"]
mod runner_tests;
#[cfg(test)]
#[path = "scoring_tests.rs"]
mod scoring_tests;
#[cfg(test)]
pub(crate) mod testing;

use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

use crate::chat::tools::action_tool::{schema_in_subset, AgentActionDef};
use crate::extensions::agent_tools::defs::{tool_defs, ExtensionSource, ExtensionToolDef, ToolRow};
use crate::extensions::bundle::{ManifestTool, ToolEffect};

use scoring::EvalSet;

/// The versioned set of example sentences (contracts/eval-format.md).
const EVAL_SET_JSON: &str = include_str!("eval_set.json");

/// Snapshot of the tools offered to the built-in agent, written by `scripts/export-eval-tools.ts`.
/// `pnpm check:agent-actions` fails when it differs from the catalog.
const TOOLS_JSON: &str = include_str!("tools.json");

/// Thirty extensions as their manifests would declare them (spec 018 R14, FR-019): twenty-nine
/// invented ones with two to four tools each and haex-mail with its fifteen real tools (spec 019
/// FR-035, copied from the manifest of haex-mail). The tools the sentences of the kind `extension`
/// are measured against.
const EXTENSION_TOOLS_JSON: &str = include_str!("extension_tools.json");

/// The embedded set. Panics on a malformed file: it is built in, and a test parses it.
pub fn embedded_set() -> EvalSet {
    serde_json::from_str(EVAL_SET_JSON).expect("the embedded eval_set.json is valid")
}

/// The embedded tool definitions the sentences are measured against.
pub fn embedded_tools() -> Vec<AgentActionDef> {
    serde_json::from_str(TOOLS_JSON).expect("the embedded tools.json is valid")
}

#[derive(Deserialize)]
struct Fixture {
    extensions: Vec<FixtureExtension>,
}

#[derive(Deserialize)]
struct FixtureExtension {
    name: String,
    title: String,
    tools: Vec<FixtureTool>,
}

#[derive(Deserialize)]
struct Localized<T> {
    de: T,
    en: T,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FixtureTool {
    name: String,
    title: Localized<String>,
    description: String,
    effect: ToolEffect,
    input_schema: Value,
    examples: Localized<Vec<String>>,
}

/// The `agentTool` action of an effect, as the confirmation rows spell it.
fn effect_action(effect: ToolEffect) -> &'static str {
    match effect {
        ToolEffect::Read => "read",
        ToolEffect::Change => "change",
        ToolEffect::Destructive => "destructive",
    }
}

/// The tools of the embedded extensions as the app would offer them with every tool confirmed:
/// the same definitions, names (`x_<slug>_<tool>`) and texts as for installed extensions. Panics
/// on a malformed file: it is built in, and a test parses it.
pub fn embedded_extension_tools() -> Vec<ExtensionToolDef> {
    let fixture: Fixture = serde_json::from_str(EXTENSION_TOOLS_JSON)
        .expect("the embedded extension_tools.json is valid");
    let sources = fixture
        .extensions
        .into_iter()
        .enumerate()
        .map(|(index, extension)| {
            let rows = extension
                .tools
                .iter()
                .map(|tool| ToolRow {
                    action: effect_action(tool.effect).to_owned(),
                    target: tool.name.clone(),
                    status: "granted".to_owned(),
                })
                .collect();
            let tools = extension
                .tools
                .into_iter()
                .map(|tool| ManifestTool {
                    supported: schema_in_subset(&tool.input_schema),
                    name: tool.name,
                    title_de: tool.title.de,
                    title_en: tool.title.en,
                    description: tool.description,
                    input_schema: tool.input_schema,
                    effect: tool.effect,
                    examples_de: tool.examples.de,
                    examples_en: tool.examples.en,
                })
                .collect();
            ExtensionSource {
                id: Uuid::from_u128(index as u128 + 1),
                name: extension.name,
                title: extension.title,
                tools,
                rows,
                dev: false,
            }
        })
        .collect();
    tool_defs(sources)
}

#[cfg(test)]
#[path = "extension_tools_tests.rs"]
mod extension_tools_tests;
