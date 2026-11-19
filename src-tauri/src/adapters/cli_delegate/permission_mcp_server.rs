use std::io;
use std::path::Path;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// Minimal stdio MCP server used by Claude Code's permission-prompt-tool.
/// It deliberately keeps the child process independent of Tauri and relays
/// only the approval payload over the invocation's local Unix socket.
pub(crate) async fn run_bridge_process(socket_path: &Path) -> io::Result<()> {
    let stdin = tokio::io::stdin();
    let mut lines = BufReader::new(stdin).lines();
    let mut stdout = tokio::io::stdout();

    while let Some(line) = lines.next_line().await? {
        let Ok(request) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let Some(id) = request.get("id").cloned() else {
            continue;
        };
        let result = match request.get("method").and_then(Value::as_str) {
            Some("initialize") => json!({
                "protocolVersion": request["params"]["protocolVersion"].as_str().unwrap_or("2025-06-18"),
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "holzi-approve", "version": env!("CARGO_PKG_VERSION")},
            }),
            Some("notifications/initialized") => continue,
            Some("tools/list") => json!({
                "tools": [{
                    "name": "approve",
                    "description": "Ask holzi to approve a delegate tool call.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "tool_name": {"type": "string"},
                            "input": {"type": "object"}
                        },
                        "required": ["tool_name", "input"]
                    }
                }]
            }),
            Some("tools/call") => call_approval(socket_path, request.get("params")).await?,
            _ => json!({}),
        };
        let response = json!({"jsonrpc": "2.0", "id": id, "result": result});
        let mut bytes = serde_json::to_vec(&response)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        bytes.push(b'\n');
        stdout.write_all(&bytes).await?;
        stdout.flush().await?;
    }
    Ok(())
}

#[cfg(unix)]
async fn call_approval(socket_path: &Path, params: Option<&Value>) -> io::Result<Value> {
    let arguments = params
        .and_then(|params| params.get("arguments"))
        .and_then(Value::as_object)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "missing approval arguments"))?;
    let tool_name = arguments
        .get("tool_name")
        .and_then(Value::as_str)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "missing tool_name"))?;
    let input = arguments.get("input").cloned().unwrap_or(Value::Null);

    let stream = tokio::net::UnixStream::connect(socket_path).await?;
    let (read_half, mut write_half) = stream.into_split();
    let request = json!({"tool_name": tool_name, "input": input.clone()});
    let mut bytes = serde_json::to_vec(&request)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    bytes.push(b'\n');
    write_half.write_all(&bytes).await?;
    let mut response = String::new();
    BufReader::new(read_half).read_line(&mut response).await?;
    let response_value = serde_json::from_str::<Value>(&response)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let decision = response_value
        .get("decision")
        .and_then(Value::as_str)
        .unwrap_or("deny");
    if decision == "allow" {
        Ok(json!({
            "content": [{"type": "text", "text": serde_json::to_string(&json!({
                "behavior": "allow",
                "updatedInput": input,
            })).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?}]
        }))
    } else {
        Ok(json!({
            "content": [{"type": "text", "text": "{\"behavior\":\"deny\",\"message\":\"Denied by holzi\"}"}],
            "isError": false
        }))
    }
}

#[cfg(not(unix))]
async fn call_approval(_socket_path: &Path, _params: Option<&Value>) -> io::Result<Value> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "CLI delegate approval sockets are not implemented on this target",
    ))
}
