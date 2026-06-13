//! JSON-RPC 2.0 wire types for the MCP (Model Context Protocol) endpoint.
//!
//! These are the lowest-level message shapes — request, response, error —
//! plus the MCP-specific `ToolDefinition`. No protocol logic lives here;
//! see sibling modules for dispatch and the tool registry.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A JSON-RPC 2.0 request.
///
/// `id` is omitted for notifications (per the JSON-RPC 2.0 spec).
/// `params` is omitted when the method takes no arguments.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

/// A JSON-RPC 2.0 response.
///
/// Exactly one of `result` or `error` must be present on the wire;
/// the types here don't enforce that — callers do.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

/// A JSON-RPC 2.0 error object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// An MCP tool definition, as returned by `tools/list`.
///
/// `input_schema` is a JSON Schema describing the tool's parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}
