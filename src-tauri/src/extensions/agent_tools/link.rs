//! The MCP connection to one extension frame (spec 018, research R1, R2). haex-vault is the client; its
//! messages leave as JSON through `send`, which the caller hands to the frontend for the frame's
//! port, and the frame's answers arrive as JSON on `receive`.

use std::future::{ready, Future};

use futures::channel::mpsc::{unbounded, UnboundedReceiver};
use futures::StreamExt;
#[allow(deprecated)]
use rmcp::model::ListRootsResult;
use rmcp::model::{
    ClientCapabilities, ClientConfig, ElicitRequestParams, ElicitResult,
    ElicitationCreateRequestMethod, Implementation, ListRootsRequestMethod, ProtocolVersion,
};
use rmcp::service::{
    ClientInitializeError, MaybeSendFuture, RequestContext, RoleClient, RunningService,
    RxJsonRpcMessage, TxJsonRpcMessage,
};
use rmcp::{ClientHandler, ErrorData as McpError, ServiceExt};
use serde_json::Value;

/// The client side of a tool connection. It declares no capabilities and answers every request of
/// the extension except `ping` with "method not found" (research R2): this connection carries
/// haex-vault's calls of the extension's tools and nothing else. A route from an extension to the agent, with a model the
/// user chose, is later work (spec 047 and after) and does not run over this connection.
#[derive(Clone, Copy, Debug, Default)]
pub struct ToolClient;

/// The MCP version of a tool connection: the newest with the `initialize` handshake, which the
/// vault-sdk server implements (contracts/mcp-port.md). Pinned so an rmcp update cannot move it.
pub const PROTOCOL_VERSION: ProtocolVersion = ProtocolVersion::V_2025_11_25;

impl ClientHandler for ToolClient {
    fn get_info(&self) -> ClientConfig {
        ClientConfig::new(
            ClientCapabilities::default(),
            Implementation::new("haex", env!("CARGO_PKG_VERSION")),
        )
        .with_protocol_version(PROTOCOL_VERSION)
    }

    // `sampling/createMessage` keeps rmcp's default, which already answers "method not found".

    // MCP deprecates roots (SEP-2577), but rmcp's default still answers `roots/list` with an empty
    // list instead of refusing it.
    #[allow(deprecated)]
    fn list_roots(
        &self,
        _context: RequestContext<RoleClient>,
    ) -> impl Future<Output = Result<ListRootsResult, McpError>> + MaybeSendFuture + '_ {
        ready(Err(McpError::method_not_found::<ListRootsRequestMethod>()))
    }

    // The default declines instead of refusing.
    fn create_elicitation(
        &self,
        _request: ElicitRequestParams,
        _context: RequestContext<RoleClient>,
    ) -> impl Future<Output = Result<ElicitResult, McpError>> + MaybeSendFuture + '_ {
        ready(Err(McpError::method_not_found::<
            ElicitationCreateRequestMethod,
        >()))
    }
}

/// A running connection to one extension frame.
pub type Link = RunningService<RoleClient, ToolClient>;

/// Connects to an extension and runs the MCP handshake. Messages that are not JSON-RPC are dropped;
/// the connection ends when `receive` ends. The handshake has no time limit of its own: a frame
/// that stays open but never answers `initialize` keeps this pending, so the caller bounds it
/// (research R9).
pub async fn connect<F>(
    send: F,
    receive: UnboundedReceiver<Value>,
) -> Result<Link, Box<ClientInitializeError>>
where
    F: Fn(Value) + Send + 'static,
{
    let (sink, mut outbound) = unbounded::<TxJsonRpcMessage<RoleClient>>();
    tokio::spawn(async move {
        while let Some(message) = outbound.next().await {
            match serde_json::to_value(&message) {
                Ok(value) => send(value),
                Err(error) => {
                    log::warn!("agent tools: a message to an extension is not JSON: {error}")
                }
            }
        }
    });
    let stream = receive.filter_map(|value| {
        ready(
            serde_json::from_value::<RxJsonRpcMessage<RoleClient>>(value)
                .map_err(|error| {
                    log::warn!(
                        "agent tools: dropped a malformed message from an extension: {error}"
                    )
                })
                .ok(),
        )
    });
    ToolClient.serve((sink, stream)).await.map_err(Box::new)
}
