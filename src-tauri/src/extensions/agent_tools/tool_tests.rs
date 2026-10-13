use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::defs::ExtensionToolDef;
use super::slot::McpLink;
use super::test_support::serve_json;
use super::tool::{ExtensionTool, Links, MAX_RESULT_BYTES, SOURCE};
use crate::chat::tools::{RiskClass, Tool};
use crate::extensions::bridge::frames::{FrameRegistry, FrameSession};
use crate::extensions::bundle::{ManifestTool, ToolEffect};

/// A frame of the test extension, served by the in-memory server (`echo`, `slow`, `fail`).
struct TestLinks {
    session: Arc<FrameSession>,
    _frames: FrameRegistry,
}

impl TestLinks {
    fn new() -> Arc<Self> {
        let frames = FrameRegistry::default();
        let session = frames.open(Uuid::new_v4(), Uuid::new_v4(), "tab-1");
        Arc::new(Self {
            session,
            _frames: frames,
        })
    }
}

#[async_trait]
impl Links for TestLinks {
    async fn open(&self, _: Uuid) -> Result<(Arc<FrameSession>, Arc<McpLink>), String> {
        let to_holzi = Arc::clone(&self.session);
        let to_extension = serve_json(move |message| to_holzi.mcp.deliver(message));
        let link = self
            .session
            .mcp
            .link(to_extension)
            .await
            .map_err(|e| e.to_string())?;
        Ok((Arc::clone(&self.session), link))
    }
}

struct NoFrame;

#[async_trait]
impl Links for NoFrame {
    async fn open(&self, _: Uuid) -> Result<(Arc<FrameSession>, Arc<McpLink>), String> {
        Err("no frame of the extension could be opened".to_owned())
    }
}

fn def(name: &str, effect: ToolEffect, always_ask: bool) -> ExtensionToolDef {
    ExtensionToolDef {
        extension_id: Uuid::new_v4(),
        extension_title: "Test-Erweiterung".to_owned(),
        tool: ManifestTool {
            name: name.to_owned(),
            title_de: "Echo de".to_owned(),
            title_en: "Echo en".to_owned(),
            description: "Echoes the text".to_owned(),
            input_schema: json!({ "type": "object" }),
            effect,
            examples_de: vec![],
            examples_en: vec![],
            supported: true,
        },
        model_name: format!("x_test_{name}"),
        always_ask,
        dev: false,
    }
}

fn content(result: &crate::chat::tools::ToolResult) -> Value {
    serde_json::from_str(&result.content).expect("JSON content")
}

#[tokio::test]
async fn a_call_returns_the_extensions_text_as_data() {
    let links = TestLinks::new();
    let tool = ExtensionTool::new(def("echo", ToolEffect::Read, false), links.clone());
    let result = tool
        .execute(json!({ "text": "hallo" }), CancellationToken::new())
        .await;
    assert!(!result.is_error, "{}", result.content);
    assert_eq!(
        content(&result),
        json!({ "extension": "Test-Erweiterung", "data": "hallo" })
    );
    assert_eq!(links.session.mcp.calls_in_flight(), 0);
}

#[tokio::test]
async fn invalid_input_reported_by_the_extension_is_an_error_the_model_sees() {
    let tool = ExtensionTool::new(def("echo", ToolEffect::Read, false), TestLinks::new());
    let result = tool.execute(json!({}), CancellationToken::new()).await;
    assert!(result.is_error);
    assert_eq!(content(&result)["data"], "invalid input: text is required");
}

/// A handler that throws, e.g. because a host function is missing on this device (`desktop_only`).
#[tokio::test]
async fn a_failing_handler_is_an_error_with_its_message() {
    let tool = ExtensionTool::new(def("fail", ToolEffect::Read, false), TestLinks::new());
    let result = tool.execute(json!({}), CancellationToken::new()).await;
    assert!(result.is_error);
    assert_eq!(content(&result)["data"], "the tool failed");
}

#[tokio::test]
async fn a_tool_the_extension_does_not_serve_is_unavailable() {
    let tool = ExtensionTool::new(def("missing", ToolEffect::Read, false), TestLinks::new());
    let result = tool.execute(json!({}), CancellationToken::new()).await;
    assert!(result.is_error);
    assert_eq!(content(&result)["error"], "tool_unavailable");
}

#[tokio::test]
async fn no_frame_means_the_extension_is_unavailable() {
    let tool = ExtensionTool::new(def("echo", ToolEffect::Read, false), Arc::new(NoFrame));
    let result = tool
        .execute(json!({ "text": "x" }), CancellationToken::new())
        .await;
    assert!(result.is_error);
    assert_eq!(content(&result)["error"], "extension_unavailable");
}

#[tokio::test]
async fn a_call_that_takes_too_long_times_out() {
    let links = TestLinks::new();
    let tool = ExtensionTool::with_timeout(
        def("slow", ToolEffect::Read, false),
        links.clone(),
        Duration::from_millis(100),
    );
    let result = tool
        .execute(json!({ "text": "x" }), CancellationToken::new())
        .await;
    assert!(result.is_error);
    assert_eq!(content(&result)["error"], "timeout");
    assert_eq!(links.session.mcp.calls_in_flight(), 0);
}

#[tokio::test]
async fn stopping_the_turn_cancels_the_call() {
    let tool = ExtensionTool::new(def("slow", ToolEffect::Read, false), TestLinks::new());
    let cancel = CancellationToken::new();
    let stop = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        stop.cancel();
    });
    let result = tokio::time::timeout(
        Duration::from_secs(5),
        tool.execute(json!({ "text": "x" }), cancel),
    )
    .await
    .expect("the call ends when the turn stops");
    assert_eq!(content(&result)["error"], "cancelled");
}

#[tokio::test]
async fn a_long_result_is_cut_at_64_kib() {
    let tool = ExtensionTool::new(def("echo", ToolEffect::Read, false), TestLinks::new());
    let long = "ä".repeat(MAX_RESULT_BYTES);
    let result = tool
        .execute(json!({ "text": long }), CancellationToken::new())
        .await;
    let data = content(&result)["data"].as_str().unwrap().to_owned();
    assert!(data.ends_with("[truncated: the result was longer than 64 KiB]"));
    assert!(data.len() < MAX_RESULT_BYTES + 100);
}

#[test]
fn the_tool_says_where_it_comes_from_and_how_it_is_approved() {
    let read = ExtensionTool::new(def("echo", ToolEffect::Read, false), Arc::new(NoFrame));
    let change = ExtensionTool::new(def("add", ToolEffect::Change, true), Arc::new(NoFrame));
    let destroy = ExtensionTool::new(
        def("drop", ToolEffect::Destructive, false),
        Arc::new(NoFrame),
    );

    assert_eq!(read.name(), "x_test_echo");
    assert_eq!(read.source(), SOURCE);
    assert_eq!(read.risk_class(), RiskClass::Safe);
    assert_eq!(change.risk_class(), RiskClass::Change);
    assert_eq!(destroy.risk_class(), RiskClass::Risky);
    assert!(!read.always_ask());
    assert!(change.always_ask());
    assert!(read.description().contains("not instructions"));
    let origin = read.origin().expect("an extension's tool has an origin");
    assert_eq!(origin.extension_name, "Test-Erweiterung");
    assert_eq!(origin.tool_title_de, "Echo de");
    assert!(!origin.dev);
}
