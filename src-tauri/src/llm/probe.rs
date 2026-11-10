//! Whether a local model's chat template includes the tools it is offered (spec 032 R8, FR-018a).

use crate::adapters::types::{ToolSpec, ToolTemplateProbe};

/// The fixed message both engines render with and without the dummy tool.
pub(crate) const PROBE_MESSAGE: &str = "Please open the settings.";

/// The decision, apart from the model so a test can feed it any pair of renderings: `without` is
/// the text rendered with no tool, `with` is the text rendered with one, or the error that came
/// instead.
pub(crate) fn decide(without: Result<&str, &str>, with: Result<&str, &str>) -> ToolTemplateProbe {
    match (without, with) {
        // A template that cannot render a plain message tells nothing about tools.
        (Err(_), _) => ToolTemplateProbe::Inconclusive,
        // Some template renderers explicitly refuse to render when tools are present. That is
        // evidence that the template ignores tools, not an inconclusive probe.
        (Ok(_), Err(error)) if error.contains("does not handle tool usage") => {
            ToolTemplateProbe::IgnoresTools
        }
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
