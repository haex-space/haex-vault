use super::*;

fn thread(origin: Option<ThreadOrigin>) -> ChatThread {
    ChatThread {
        id: Uuid::nil(),
        title: "Wetter".into(),
        last_provider_id: None,
        last_model_id: None,
        created_at: 1,
        updated_at: 2,
        origin,
    }
}

#[test]
fn the_payload_tells_a_spoken_conversation_from_a_typed_one() {
    let voice =
        serde_json::to_value(ThreadPayload::from(thread(Some(ThreadOrigin::Voice)))).unwrap();
    assert_eq!(voice["origin"], "voice");
    let typed = serde_json::to_value(ThreadPayload::from(thread(None))).unwrap();
    assert!(typed["origin"].is_null());
}
