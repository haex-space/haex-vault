//! Whether the chat template of a loaded local model takes tools at all (spec 032 R8, FR-018a).
//!
//! mistralrs has no `supports_tools`. A template written as a string silently ignores the tools it
//! is handed when it does not mention them, and one made of named parts refuses with "does not
//! handle tool usage". So the probe renders one fixed message twice, with and without a dummy
//! tool, through the model's own `tokenize` and `detokenize`: the same text, or the refusal, says
//! the template cannot show a tool to this model.

use either::Either;
use mistralrs::{TextMessageRole, TextMessages};

use super::stream::to_mistralrs_tool;
use super::LocalModel;
use crate::adapters::types::{ToolSpec, ToolTemplateProbe};

/// The fixed message both engines render with and without the dummy tool.
pub(crate) const PROBE_MESSAGE: &str = "Please open the settings.";

/// What mistralrs says when a template has no part for tools.
const NO_TOOL_USAGE: &str = "does not handle tool usage";

/// The decision, apart from the model so a test can feed it any pair of renderings: `without` is
/// the text rendered with no tool, `with` the text rendered with one, or the error that came
/// instead.
pub(crate) fn decide(without: Result<&str, &str>, with: Result<&str, &str>) -> ToolTemplateProbe {
    match (without, with) {
        // A template that cannot render a plain message tells nothing about tools.
        (Err(_), _) => ToolTemplateProbe::Inconclusive,
        (Ok(_), Err(error)) if error.contains(NO_TOOL_USAGE) => ToolTemplateProbe::IgnoresTools,
        (Ok(plain), Ok(with_tool)) if plain == with_tool => ToolTemplateProbe::IgnoresTools,
        _ => ToolTemplateProbe::Inconclusive,
    }
}

/// Builds the stable tool definition used to compare the model's template output.
pub(crate) fn dummy_tool() -> ToolSpec {
    ToolSpec {
        name: "holzi_probe_tool".to_owned(),
        description: "A tool that only exists to see whether the template shows tools.".to_owned(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": { "probe": { "type": "string" } },
        }),
    }
}

impl LocalModel {
    /// Renders a fixed message with and without a dummy tool and compares. Never fails: whatever
    /// goes wrong reads as inconclusive, and the self-test takes over.
    pub async fn probe_tool_template(&self) -> ToolTemplateProbe {
        let without = self.render_probe(None).await;
        let with = self.render_probe(Some(dummy_tool())).await;
        decide(
            without.as_deref().map_err(String::as_str),
            with.as_deref().map_err(String::as_str),
        )
    }

    /// Renders the fixed probe message with the requested optional tool.
    async fn render_probe(&self, tool: Option<ToolSpec>) -> Result<String, String> {
        let messages = TextMessages::new().add_message(TextMessageRole::User, PROBE_MESSAGE);
        let tools = tool.as_ref().map(|spec| vec![to_mistralrs_tool(spec)]);
        let model = self.inner();
        let tokens = model
            .tokenize(Either::Left(messages), tools, false, true, Some(false))
            .await
            .map_err(|e| e.to_string())?;
        model
            .detokenize(tokens, false)
            .await
            .map_err(|e| e.to_string())
    }
}
