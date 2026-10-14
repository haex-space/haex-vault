use std::sync::{Arc, Mutex};

use serde_json::{json, Value};
use uuid::Uuid;

use super::commands::{deliver, to_frame, MCP_MESSAGE};
use crate::extensions::agent_tools::test_support::serve_json;
use crate::extensions::bridge::dispatch::Emit;
use crate::extensions::bridge::frames::FrameRegistry;

#[derive(Clone, Default)]
struct Recorded(Arc<Mutex<Vec<(String, Value)>>>);

impl Emit for Recorded {
    fn emit(&self, event: &str, payload: Value) {
        self.0.lock().unwrap().push((event.to_owned(), payload));
    }
}

#[test]
fn holzis_messages_go_to_the_frame_as_events() {
    let recorded = Recorded::default();
    let send = to_frame(recorded.clone(), "frame-1".to_owned());
    send(json!({ "jsonrpc": "2.0", "id": 1, "method": "ping" }));
    assert_eq!(
        recorded.0.lock().unwrap().as_slice(),
        [(
            MCP_MESSAGE.to_owned(),
            json!({ "frame": "frame-1",
                    "message": { "jsonrpc": "2.0", "id": 1, "method": "ping" } })
        )]
    );
}

#[test]
fn a_message_for_an_unknown_frame_is_dropped() {
    let frames = FrameRegistry::default();
    assert!(!deliver(
        &frames,
        "nope",
        json!({ "jsonrpc": "2.0", "id": 1, "result": {} })
    ));
}

/// The extension's answers reach the link of their own frame session only.
#[tokio::test]
async fn the_extensions_answers_reach_the_link_of_its_frame() {
    let frames = Arc::new(FrameRegistry::default());
    let session = frames.open(Uuid::new_v4(), Uuid::new_v4(), "tab-1");
    let other = frames.open(Uuid::new_v4(), Uuid::new_v4(), "tab-2");
    let (relay, frame) = (Arc::clone(&frames), session.frame.clone());
    let to_extension = serve_json(move |message| {
        assert!(deliver(&relay, &frame, message));
    });

    let link = session
        .mcp
        .link(to_extension)
        .await
        .expect("the link opens");
    assert_eq!(link.tools(), ["echo", "slow", "fail"]);
    assert_eq!(other.mcp.calls_in_flight(), 0);
    assert!(
        tokio::time::timeout(
            std::time::Duration::from_millis(200),
            other.mcp.link(|_| {})
        )
        .await
        .is_err(),
        "the other frame got no answers"
    );
}
