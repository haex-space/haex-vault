use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::agent_log::Outcome;
use super::dispatch::extension_result;
use crate::chat::tools::action_bridge::ACTION_FAILED_MESSAGE;
use crate::chat::tools::{Tool, ToolResult};
use crate::extensions::agent_tools::defs::ExtensionToolDef;
use crate::extensions::agent_tools::slot::McpLink;
use crate::extensions::agent_tools::tool::{ExtensionTool, Links};
use crate::extensions::bridge::frames::FrameSession;
use crate::extensions::bundle::{ManifestTool, ToolEffect};

struct NoFrame;

#[async_trait]
impl Links for NoFrame {
    async fn open(&self, _: Uuid) -> Result<(Arc<FrameSession>, Arc<McpLink>), String> {
        Err("no frame of the extension could start".to_owned())
    }
}

fn structured(result: &rmcp::model::CallToolResult) -> Value {
    result.structured_content.clone().unwrap_or(Value::Null)
}

/// Contract "unavailable": holzi could not reach the extension.
#[tokio::test]
async fn an_extension_that_cannot_be_reached_is_unavailable() {
    let def = ExtensionToolDef {
        extension_id: Uuid::from_u128(1),
        extension_title: "Mail".to_owned(),
        tool: ManifestTool {
            name: "count".to_owned(),
            title_de: "Zählen".to_owned(),
            title_en: "Count".to_owned(),
            description: "Counts".to_owned(),
            input_schema: json!({ "type": "object" }),
            effect: ToolEffect::Read,
            examples_de: vec![],
            examples_en: vec![],
            supported: true,
        },
        model_name: "x_mail_count".to_owned(),
        always_ask: false,
        dev: false,
    };
    let tool = ExtensionTool::new(def, Arc::new(NoFrame));
    let (result, outcome) =
        extension_result(tool.execute(json!({}), CancellationToken::new()).await);
    assert_eq!(outcome, Outcome::Unavailable);
    assert_eq!(structured(&result)["code"], "unavailable");
}

#[test]
fn the_data_of_the_extension_comes_back_as_it_is() {
    let answer = json!({ "extension": "Mail", "data": "3 new" });
    let (result, outcome) = extension_result(ToolResult::ok(answer.to_string()));
    assert_eq!(outcome, Outcome::Ok);
    assert_eq!(result.is_error, Some(false));
    assert_eq!(structured(&result), answer);
}

/// An error the extension answered keeps its data; a failure inside holzi says nothing internal.
#[test]
fn an_error_keeps_the_extension_s_data_but_never_holzi_s_internals() {
    let (own, outcome) = extension_result(ToolResult::error(
        json!({ "extension": "Mail", "data": "no inbox" }).to_string(),
    ));
    assert_eq!(outcome, Outcome::Error);
    assert_eq!(
        structured(&own),
        json!({
            "code": "failed",
            "message": ACTION_FAILED_MESSAGE,
            "extension": "Mail",
            "data": "no inbox",
        })
    );

    let (internal, _) = extension_result(ToolResult::error(
        json!({ "extension": "Mail", "error": "call_failed", "message": "pipe /run/x closed" })
            .to_string(),
    ));
    assert_eq!(
        structured(&internal),
        json!({ "code": "failed", "message": ACTION_FAILED_MESSAGE })
    );
}
