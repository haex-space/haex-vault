use super::*;

#[test]
fn a_session_is_found_by_frame_and_by_its_token_for_its_extension_only() {
    let frames = FrameRegistry::default();
    let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
    let session = frames.open(a, Uuid::new_v4(), "tab-1");
    frames.open(b, Uuid::new_v4(), "tab-2");

    assert_eq!(frames.get(&session.frame).unwrap().tab_id, "tab-1");
    assert!(frames.by_token(a, &session.token).is_some());
    assert!(
        frames.by_token(b, &session.token).is_none(),
        "another extension's token"
    );
    assert!(frames.by_token(a, "nope").is_none());
    assert_eq!(frames.of_extension(a).len(), 1);

    frames.close(&session.frame);
    assert!(frames.get(&session.frame).is_none());
    assert!(frames.by_token(a, &session.token).is_none());
}

/// A frame's tool connection lives as long as its session (spec 018, research R10): closing the
/// session ends the connection, and a call still waiting for the extension fails.
#[tokio::test]
async fn closing_a_session_ends_its_tool_connection_and_fails_pending_calls() {
    use rmcp::model::CallToolRequestParams;

    use crate::extensions::agent_tools::test_support::serve_json;

    let frames = FrameRegistry::default();
    let session = frames.open(Uuid::new_v4(), Uuid::new_v4(), "tab-1");
    let to_holzi = Arc::clone(&session);
    let to_extension = serve_json(move |message| to_holzi.mcp.deliver(message));

    let link = session
        .mcp
        .link(to_extension)
        .await
        .expect("the link opens");
    assert_eq!(link.tools(), ["echo", "slow", "fail"]);
    let again = session
        .mcp
        .link(|_| panic!("an open link is reused"))
        .await
        .expect("the same link");
    assert!(Arc::ptr_eq(&link, &again));

    let mut arguments = serde_json::Map::new();
    arguments.insert("text".to_owned(), serde_json::json!("x"));
    let peer = link.peer().clone();
    let pending = tokio::spawn(async move {
        peer.call_tool(CallToolRequestParams::new("slow").with_arguments(arguments))
            .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    frames.close(&session.frame);
    let result = tokio::time::timeout(std::time::Duration::from_secs(5), pending)
        .await
        .expect("the pending call ends")
        .expect("call task");
    assert!(result.is_err(), "the pending call fails");
    assert!(
        session.mcp.link(|_| {}).await.is_err(),
        "a closed session opens no new link"
    );
}

#[test]
fn a_session_counts_the_tool_calls_in_flight() {
    let frames = FrameRegistry::default();
    let session = frames.open(Uuid::new_v4(), Uuid::new_v4(), "tab-1");
    assert_eq!(session.mcp.calls_in_flight(), 0);
    let first = session.mcp.begin_call();
    let second = session.mcp.begin_call();
    assert_eq!(session.mcp.calls_in_flight(), 2);
    drop(first);
    assert_eq!(session.mcp.calls_in_flight(), 1);
    drop(second);
    assert_eq!(session.mcp.calls_in_flight(), 0);
}
