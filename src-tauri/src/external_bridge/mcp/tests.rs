use super::protocol::*;
use serde_json::json;

#[test]
fn test_jsonrpc_request_roundtrip() {
    let req = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(json!(1)),
        method: "tools/list".to_string(),
        params: None,
    };
    let serialized = serde_json::to_string(&req).unwrap();
    assert!(serialized.contains("\"method\":\"tools/list\""));
    let deserialized: JsonRpcRequest = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized.method, "tools/list");
}

#[test]
fn test_jsonrpc_response_roundtrip() {
    let resp = JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id: json!(1),
        result: Some(json!({"ok": true})),
        error: None,
    };
    let serialized = serde_json::to_string(&resp).unwrap();
    // `error` is None and should be omitted on the wire
    assert!(!serialized.contains("\"error\""));
    let deserialized: JsonRpcResponse = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized.id, json!(1));
    assert_eq!(deserialized.result, Some(json!({"ok": true})));
    assert!(deserialized.error.is_none());
}

#[test]
fn test_jsonrpc_error_roundtrip() {
    let err = JsonRpcError {
        code: -32601,
        message: "Method not found".to_string(),
        data: None,
    };
    let serialized = serde_json::to_string(&err).unwrap();
    assert!(serialized.contains("\"code\":-32601"));
    assert!(!serialized.contains("\"data\""));
    let deserialized: JsonRpcError = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized.code, -32601);
    assert_eq!(deserialized.message, "Method not found");
}

#[test]
fn test_tool_definition_roundtrip() {
    let tool = ToolDefinition {
        name: "memory.recall".to_string(),
        description: "Recall a memory".to_string(),
        input_schema: json!({"type": "object"}),
    };
    let serialized = serde_json::to_string(&tool).unwrap();
    // Verify camelCase rename on `input_schema`
    assert!(serialized.contains("\"inputSchema\""));
    assert!(!serialized.contains("\"input_schema\""));
    let deserialized: ToolDefinition = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized.name, "memory.recall");
    assert_eq!(deserialized.input_schema, json!({"type": "object"}));
}
