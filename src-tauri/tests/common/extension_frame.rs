//! The test extension `agent-tools.xt` (spec 018) for integration tests: installing it, and its
//! frame as an in-memory MCP server joined to a frame session through JSON, the way the frontend
//! relays the port. No webview runs. Included per binary via
//! `#[path = "common/extension_frame.rs"] mod extension_frame;`.
#![allow(dead_code)]

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

use holzi_lib::extensions::agent_tools::slot::McpLink;
use holzi_lib::extensions::agent_tools::tool::Links;
use holzi_lib::extensions::bridge::frames::{FrameRegistry, FrameSession};
use holzi_lib::extensions::registry::install::{install, PermissionChoice};
use holzi_lib::vault_gate::VaultDb;

/// The frame of the test extension: it serves the tools its page registers and counts two entries.
#[derive(Clone)]
pub struct Frame;

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
pub struct InMemoryFrame {
    pub session: Arc<FrameSession>,
    pub _frames: FrameRegistry,
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

pub fn choice(kind: &str, action: &str, target: &str, granted: bool) -> PermissionChoice {
    PermissionChoice {
        kind: kind.to_owned(),
        action: action.to_owned(),
        target: target.to_owned(),
        granted,
    }
}

/// Installs `agent-tools.xt` on `device` with `choices`.
pub async fn install_agent_tools(vault: &VaultDb, device: Uuid, choices: Vec<PermissionChoice>) {
    let bytes = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/extension_e2e/agent-tools.xt"),
    )
    .expect("agent-tools.xt (HOLZI_WRITE_E2E_FIXTURES=1 builds it)");
    let installing = vault.clone();
    tokio::task::spawn_blocking(move || {
        install(&installing, &bytes, choices, false, device, 1_000)
    })
    .await
    .expect("the install task")
    .expect("install");
}
