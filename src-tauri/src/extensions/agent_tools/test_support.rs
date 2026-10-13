//! Test support for the tool connection: an in-memory MCP server in place of an extension, joined
//! to the client through JSON exactly like the frontend relay joins the frame's port (research R1).
//! Every message is recorded with its direction, which the contract test writes as transcripts.

use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures::channel::mpsc::{unbounded, UnboundedSender};
use futures::StreamExt;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation,
    ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool as McpToolInfo,
};
use rmcp::service::{
    MaybeSendFuture, RequestContext, RoleServer, RunningService, RxJsonRpcMessage, TxJsonRpcMessage,
};
use rmcp::{ErrorData as McpError, ServerHandler, ServiceExt};
use serde_json::{json, Value};

use super::link::{connect, Link};

/// Who sent a recorded message: `c2s` holzi (client) to the extension (server), `s2c` back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    ClientToServer,
    ServerToClient,
}

impl Direction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ClientToServer => "c2s",
            Self::ServerToClient => "s2c",
        }
    }
}

pub type Recording = Arc<Mutex<Vec<(Direction, Value)>>>;

/// The extension's side: `echo` returns its `text`, `slow` waits until cancelled, `fail` reports a
/// tool error.
#[derive(Clone, Default)]
pub struct TestServer;

impl ServerHandler for TestServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("agent-tools", "1.0.0"))
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListToolsResult, McpError>> + MaybeSendFuture + '_ {
        let schema = |value: Value| value.as_object().expect("object literal").clone();
        let text = schema(json!({
            "type": "object",
            "properties": { "text": { "type": "string" } },
            "required": ["text"],
        }));
        let tools = vec![
            McpToolInfo::new("echo", "Echoes the given text back", text.clone()),
            McpToolInfo::new("slow", "Waits until it is cancelled", text),
            McpToolInfo::new("fail", "Always fails", schema(json!({ "type": "object" }))),
        ];
        std::future::ready(Ok(ListToolsResult {
            tools,
            ..Default::default()
        }))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        let text = request
            .arguments
            .as_ref()
            .and_then(|args| args.get("text"))
            .and_then(Value::as_str)
            .map(str::to_owned);
        match (request.name.as_ref(), text) {
            ("echo", Some(text)) => {
                Ok(CallToolResult::success(vec![ContentBlock::text(text)]).into())
            }
            // Invalid input is a tool error the model can correct, as the vault-sdk server
            // answers it (contracts/mcp-port.md), not a protocol error.
            ("echo" | "slow", None) => Ok(CallToolResult::error(vec![ContentBlock::text(
                "invalid input: text is required",
            )])
            .into()),
            ("slow", Some(_)) => {
                context.ct.cancelled().await;
                Ok(CallToolResult::error(vec![ContentBlock::text("cancelled")]).into())
            }
            _ => Ok(CallToolResult::error(vec![ContentBlock::text("the tool failed")]).into()),
        }
    }
}

/// A client link to [`TestServer`] through a recording JSON pipe.
pub struct Pipe {
    pub link: Link,
    pub recording: Recording,
    to_client: UnboundedSender<Value>,
    /// Keeps the server running for as long as the pipe lives.
    pub _server: RunningService<RoleServer, TestServer>,
}

impl Pipe {
    /// Sends a raw JSON message to the client as if the extension had sent it, and records it.
    pub fn inject(&self, message: Value) {
        self.recording
            .lock()
            .expect("recording")
            .push((Direction::ServerToClient, message.clone()));
        self.to_client.unbounded_send(message).expect("inject");
    }

    /// Every message that crossed the pipe, in order.
    pub fn messages(&self) -> Vec<(Direction, Value)> {
        self.recording.lock().expect("recording").clone()
    }

    /// Waits until the client sent a message with this JSON-RPC `id`, and returns it.
    pub async fn client_reply(&self, id: &Value) -> Value {
        for _ in 0..200 {
            let found = self
                .messages()
                .into_iter()
                .find_map(|(direction, message)| {
                    (direction == Direction::ClientToServer && message.get("id") == Some(id))
                        .then_some(message)
                });
            if let Some(message) = found {
                return message;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("the client never answered request {id}");
    }
}

/// Starts [`TestServer`] and connects a client [`Link`] to it through JSON.
pub async fn pipe() -> Pipe {
    let recording: Recording = Arc::default();

    // Server side: typed channels, fed from and drained to JSON.
    let (server_in, server_rx) = unbounded::<RxJsonRpcMessage<RoleServer>>();
    let (server_tx, mut server_out) = unbounded::<TxJsonRpcMessage<RoleServer>>();
    // Client side: the JSON the frontend would relay.
    let (to_client, client_rx) = unbounded::<Value>();

    let record = recording.clone();
    let to_client_from_server = to_client.clone();
    tokio::spawn(async move {
        while let Some(message) = server_out.next().await {
            let value = serde_json::to_value(&message).expect("server message as JSON");
            record
                .lock()
                .expect("recording")
                .push((Direction::ServerToClient, value.clone()));
            if to_client_from_server.unbounded_send(value).is_err() {
                break;
            }
        }
    });

    let record = recording.clone();
    let send = move |value: Value| {
        record
            .lock()
            .expect("recording")
            .push((Direction::ClientToServer, value.clone()));
        if let Ok(message) = serde_json::from_value::<RxJsonRpcMessage<RoleServer>>(value) {
            let _ = server_in.unbounded_send(message);
        }
    };

    let server = tokio::spawn(TestServer.serve((server_tx, server_rx)));
    let link = connect(send, client_rx).await.expect("the client connects");
    let server = server
        .await
        .expect("server task")
        .expect("the server starts");
    let pipe = Pipe {
        link,
        recording,
        to_client,
        _server: server,
    };
    // The client sends `notifications/initialized` after `connect` returns; wait for it, so every
    // test starts after the whole handshake.
    for _ in 0..200 {
        if pipe.messages().len() >= 3 {
            return pipe;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the handshake did not finish");
}

/// Starts [`TestServer`] with raw JSON ends, the way a frame's server sits behind the frontend
/// relay: `deliver` gets every message of the server, the returned function hands it one. The
/// server runs until its input is dropped.
pub fn serve_json<D>(deliver: D) -> impl Fn(Value) + Send + Sync + 'static
where
    D: Fn(Value) + Send + 'static,
{
    let (server_in, server_rx) = unbounded::<RxJsonRpcMessage<RoleServer>>();
    let (server_tx, mut server_out) = unbounded::<TxJsonRpcMessage<RoleServer>>();
    tokio::spawn(async move {
        while let Some(message) = server_out.next().await {
            deliver(serde_json::to_value(&message).expect("server message as JSON"));
        }
    });
    tokio::spawn(async move {
        if let Ok(server) = TestServer.serve((server_tx, server_rx)).await {
            let _ = server.waiting().await;
        }
    });
    move |value: Value| {
        if let Ok(message) = serde_json::from_value::<RxJsonRpcMessage<RoleServer>>(value) {
            let _ = server_in.unbounded_send(message);
        }
    }
}
