//! Spec 018 (T031, US1): the test extension `agent-tools.xt` is installed with its tools confirmed;
//! a message it fits offers its tool, the agent calls it and the answer is the extension's data.
//! The frame is an in-memory MCP server on the link; no webview runs.

#[allow(dead_code)]
#[path = "common/tool_loop_fixture.rs"]
mod tool_loop_fixture;
use tool_loop_fixture::*;

use std::sync::Arc;

use async_trait::async_trait;
use futures::channel::mpsc::unbounded;
use futures::StreamExt;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, ListToolsResult,
    PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool as McpToolInfo,
};
use rmcp::service::{RequestContext, RoleServer, RxJsonRpcMessage, TxJsonRpcMessage};
use rmcp::{ErrorData as McpError, ServerHandler, ServiceExt};
use serde_json::{json, Value};
use uuid::Uuid;

use holzi_lib::adapters::types::ToolCall as LlmToolCall;
use holzi_lib::adapters::StreamChunk;
use holzi_lib::chat::session::ChatState;
use holzi_lib::chat::tools::availability::ToolAvailability;
use holzi_lib::chat::tools::offer::first_offer;
use holzi_lib::extensions::agent_tools::defs::load;
use holzi_lib::extensions::agent_tools::registration::register_extension_tools;
use holzi_lib::extensions::agent_tools::slot::McpLink;
use holzi_lib::extensions::agent_tools::tool::Links;
use holzi_lib::extensions::bridge::frames::{FrameRegistry, FrameSession};
use holzi_lib::extensions::registry::install::{install, PermissionChoice};
use holzi_lib::storage::chat_messages::MessageRole;
use holzi_lib::storage::known_devices;

const QUESTION: &str = "Wie viele Einträge habe ich?";
const COUNT: &str = "x_agenttools_count_entries";

/// The frame of the test extension: it serves the tools its page registers and counts two entries.
#[derive(Clone)]
struct Frame;

impl ServerHandler for Frame {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        let object = json!({ "type": "object" }).as_object().cloned().unwrap();
        let tools = ["count_entries", "add_entry", "slow_echo"]
            .map(|name| McpToolInfo::new(name, "from the manifest", object.clone()))
            .to_vec();
        Ok(ListToolsResult {
            tools,
            ..Default::default()
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        let text = match request.name.as_ref() {
            "count_entries" => r#"{"entries":2}"#,
            _ => "unexpected tool",
        };
        Ok(CallToolResult::success(vec![ContentBlock::text(text)]).into())
    }
}

/// Joins the session's link to [`Frame`] through JSON, the way the frontend relays the port.
struct InMemoryFrame {
    session: Arc<FrameSession>,
    _frames: FrameRegistry,
}

#[async_trait]
impl Links for InMemoryFrame {
    async fn open(&self, extension_id: Uuid) -> Result<(Arc<FrameSession>, Arc<McpLink>), String> {
        assert_eq!(extension_id, self.session.extension_id);
        let (server_in, server_rx) = unbounded::<RxJsonRpcMessage<RoleServer>>();
        let (server_tx, mut server_out) = unbounded::<TxJsonRpcMessage<RoleServer>>();
        let to_holzi = Arc::clone(&self.session);
        tokio::spawn(async move {
            while let Some(message) = server_out.next().await {
                to_holzi
                    .mcp
                    .deliver(serde_json::to_value(&message).unwrap());
            }
        });
        tokio::spawn(async move {
            if let Ok(server) = Frame.serve((server_tx, server_rx)).await {
                let _ = server.waiting().await;
            }
        });
        let to_extension = move |value: Value| {
            if let Ok(message) = serde_json::from_value::<RxJsonRpcMessage<RoleServer>>(value) {
                let _ = server_in.unbounded_send(message);
            }
        };
        let link = self
            .session
            .mcp
            .link(to_extension)
            .await
            .map_err(|e| e.to_string())?;
        Ok((Arc::clone(&self.session), link))
    }
}

fn choice(kind: &str, action: &str, target: &str, granted: bool) -> PermissionChoice {
    PermissionChoice {
        kind: kind.to_owned(),
        action: action.to_owned(),
        target: target.to_owned(),
        granted,
    }
}

#[tokio::test]
async fn the_agent_answers_with_a_confirmed_tool_of_the_test_extension() {
    let db = open_db();
    set_permission_mode(&db, "auto");
    let vault = vault_db(&db);
    let device = vault
        .read(|q| known_devices::list_devices(q))
        .await
        .unwrap()
        .first()
        .expect("the bootstrap registers this device")
        .vault_device_uuid;

    // Install with every tool ticked; the notification stays "ask" (quickstart step 1).
    let bytes = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/extension_e2e/agent-tools.xt"),
    )
    .expect("agent-tools.xt (HOLZI_WRITE_E2E_FIXTURES=1 builds it)");
    let choices = vec![
        choice("agentTool", "read", "count_entries", true),
        choice("agentTool", "change", "add_entry", true),
        choice("agentTool", "read", "slow_echo", true),
        choice("agentTool", "read", "optional_note", true),
        choice("notifications", "show", "*", false),
    ];
    let installing = vault.clone();
    tokio::task::spawn_blocking(move || {
        install(&installing, &bytes, choices, false, device, 1_000)
    })
    .await
    .unwrap()
    .expect("install");

    // The tool outside holzi's schema subset is installed but never offered.
    let defs = vault
        .read(move |q| load(q, device).map_err(Into::into))
        .await
        .unwrap();
    let names: Vec<&str> = defs.iter().map(|d| d.model_name.as_str()).collect();
    assert_eq!(
        names,
        [COUNT, "x_agenttools_add_entry", "x_agenttools_slow_echo"]
    );
    assert!(defs.iter().all(|d| d.extension_title == "Test-Erweiterung"));

    let chat_state = ChatState::new();
    let frames = FrameRegistry::default();
    let session = frames.open(defs[0].extension_id, Uuid::new_v4(), "tab-1");
    let links: Arc<dyn Links> = Arc::new(InMemoryFrame {
        session,
        _frames: frames,
    });
    let offer = {
        let mut registry = chat_state.tool_registry.lock().unwrap();
        register_extension_tools(&mut registry, defs, &links);
        first_offer(ToolAvailability::Offered, &registry, QUESTION)
    };
    assert!(
        offer.iter().any(|tool| tool.name == COUNT),
        "the question offers the count tool without a search"
    );

    let (thread_id, user_message_id, assistant_message_id) =
        (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    seed_thread(&db, thread_id, user_message_id);
    let adapter = StubAdapter::new(vec![
        vec![Ok(StreamChunk::ToolCalls(vec![LlmToolCall {
            id: "call-1".to_owned(),
            name: COUNT.to_owned(),
            input: json!({}),
        }]))],
        vec![
            Ok(StreamChunk::Delta {
                content: "Du hast 2 Einträge.".to_owned(),
                reasoning: None,
            }),
            Ok(StreamChunk::Done {
                finish_reason: Some("end_turn".to_owned()),
                prompt_tokens: None,
                completion_tokens: None,
                ttft_ms: None,
                total_ms: 1,
            }),
        ],
    ]);
    let session = session_with(adapter).await;
    let mut request = base_request();
    request.tools = offer;

    let (rows, _) = run_scripted_turn_with(
        &db,
        &chat_state,
        &session,
        thread_id,
        user_message_id,
        assistant_message_id,
        request,
    )
    .await;

    // holzi keeps a call's source on its call row.
    let call = rows
        .iter()
        .find(|row| row.role == MessageRole::ToolCall)
        .expect("a tool call row");
    assert_eq!(call.tool_name.as_deref(), Some(COUNT));
    assert_eq!(call.tool_source.as_deref(), Some("haextension"));
    let origin: Value = serde_json::from_str(call.tool_origin.as_deref().expect("stored origin"))
        .expect("origin JSON");
    assert_eq!(origin["extensionName"], "Test-Erweiterung");
    assert_eq!(origin["toolTitleDe"], "Einträge zählen");
    assert_eq!(origin["dev"], false);
    let result = rows
        .iter()
        .find(|row| row.role == MessageRole::ToolResult)
        .expect("a tool result row");
    assert_eq!(result.tool_is_error, Some(false), "{}", result.content);
    let content: Value = serde_json::from_str(&result.content).expect("JSON result");
    assert_eq!(
        content,
        json!({ "extension": "Test-Erweiterung", "data": r#"{"entries":2}"# })
    );
    assert_eq!(rows.last().unwrap().content, "Du hast 2 Einträge.");
}
