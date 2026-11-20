use futures::channel::mpsc::unbounded;
use rmcp::model::{CallToolRequestParams, ErrorCode};
use serde_json::{json, Value};

use super::link::connect;
use super::test_support::{pipe, Direction};

#[tokio::test]
async fn the_client_lists_and_calls_tools_through_json() {
    let pipe = pipe().await;
    let tools = pipe.link.peer().list_all_tools().await.expect("tools/list");
    let names: Vec<_> = tools.iter().map(|tool| tool.name.to_string()).collect();
    assert_eq!(names, ["echo", "slow", "fail"]);

    let mut arguments = serde_json::Map::new();
    arguments.insert("text".to_owned(), json!("hallo"));
    let result = pipe
        .link
        .peer()
        .call_tool(CallToolRequestParams::new("echo").with_arguments(arguments))
        .await
        .expect("tools/call");
    let text = serde_json::to_value(&result).expect("result as JSON");
    assert_eq!(text["content"][0]["text"], "hallo");
}

#[tokio::test]
async fn the_client_declares_no_capabilities() {
    let pipe = pipe().await;
    let initialize = pipe
        .messages()
        .into_iter()
        .find(|(direction, message)| {
            *direction == Direction::ClientToServer && message["method"] == "initialize"
        })
        .expect("initialize was sent")
        .1;
    assert_eq!(initialize["params"]["capabilities"], json!({}));
}

/// Every request of the extension to haex-vault ends with "method not found" (research R2): the tool
/// connection is no route from an extension to the model, the user's roots or the user.
#[tokio::test]
async fn every_request_of_the_extension_is_refused() {
    let pipe = pipe().await;
    let requests = [
        json!({ "jsonrpc": "2.0", "id": 901, "method": "sampling/createMessage",
                "params": { "messages": [{ "role": "user",
                    "content": { "type": "text", "text": "hi" } }], "maxTokens": 10 } }),
        json!({ "jsonrpc": "2.0", "id": 902, "method": "roots/list" }),
        json!({ "jsonrpc": "2.0", "id": 903, "method": "elicitation/create",
                "params": { "mode": "form", "message": "name?",
                    "requestedSchema": { "type": "object", "properties": {} } } }),
        json!({ "jsonrpc": "2.0", "id": 904, "method": "haex-vault/anything", "params": {} }),
    ];
    for request in requests {
        let id = request["id"].clone();
        pipe.inject(request);
        let reply = pipe.client_reply(&id).await;
        assert_eq!(
            reply["error"]["code"],
            json!(ErrorCode::METHOD_NOT_FOUND.0),
            "reply to {id}: {reply}"
        );
        assert!(reply.get("result").is_none(), "reply to {id}: {reply}");
    }
}

/// `ping` is the one request haex-vault answers (contracts/mcp-port.md).
#[tokio::test]
async fn a_ping_of_the_extension_is_answered() {
    let pipe = pipe().await;
    let id = json!(905);
    pipe.inject(json!({ "jsonrpc": "2.0", "id": 905, "method": "ping" }));
    let reply = pipe.client_reply(&id).await;
    assert_eq!(reply["result"], json!({}), "{reply}");
    assert!(reply.get("error").is_none(), "{reply}");
}

#[tokio::test]
async fn a_malformed_message_from_the_extension_is_dropped() {
    let pipe = pipe().await;
    for junk in [
        json!("not a message"),
        json!({ "jsonrpc": "1.0" }),
        Value::Null,
    ] {
        pipe.inject(junk);
    }
    let tools = pipe
        .link
        .peer()
        .list_all_tools()
        .await
        .expect("still works");
    assert_eq!(tools.len(), 3);
}

#[tokio::test]
async fn a_closed_extension_fails_the_connection_instead_of_hanging() {
    let (to_client, client_rx) = unbounded::<Value>();
    drop(to_client);
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        connect(|_| {}, client_rx),
    )
    .await
    .expect("connect returns");
    assert!(result.is_err());
}
