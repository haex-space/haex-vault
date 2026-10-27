use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::action_bridge::{
    ActionBridge, ActionCaller, ActionOutcomeWire, ActionReply, ACTION_FAILED_MESSAGE,
    EVENT_ACTION_CALL_REQUEST,
};
use super::ChoiceOption;

fn bridge_with_events(
    timeout: Duration,
) -> (ActionBridge, mpsc::UnboundedReceiver<(String, Value)>) {
    let bridge = ActionBridge::with_timeout(timeout);
    let (sender, receiver) = mpsc::unbounded_channel();
    bridge.set_emitter(Arc::new(move |event: &str, payload: Value| {
        let _ = sender.send((event.to_owned(), payload));
    }));
    (bridge, receiver)
}

#[tokio::test]
async fn a_second_emitter_is_ignored() {
    let bridge = ActionBridge::with_timeout(Duration::from_secs(5));
    let (sender, mut events) = mpsc::unbounded_channel();
    bridge.set_emitter(Arc::new({
        let sender = sender.clone();
        move |_, _| {
            let _ = sender.send("first");
        }
    }));
    bridge.set_emitter(Arc::new(move |_, _| {
        let _ = sender.send("second");
    }));

    let cancel = CancellationToken::new();
    let caller = bridge.clone();
    let token = cancel.clone();
    let call = tokio::spawn(async move { caller.call("x", json!({}), &token).await });
    assert_eq!(events.recv().await, Some("first"));
    cancel.cancel();
    assert!(matches!(
        call.await.expect("the call task"),
        ActionReply::Err { ref code, .. } if code == "tool_call_cancelled"
    ));
    assert!(events.try_recv().is_err());
}

fn request_id(payload: &Value) -> Uuid {
    serde_json::from_value(payload["requestId"].clone()).expect("a request id")
}

#[tokio::test]
async fn a_call_emits_the_request_and_returns_the_answer() {
    let (bridge, mut events) = bridge_with_events(Duration::from_secs(5));
    let caller = bridge.clone();
    let call = tokio::spawn(async move {
        caller
            .call("wm.state.get", json!({ "a": 1 }), &CancellationToken::new())
            .await
    });
    let (event, payload) = events.recv().await.expect("the request event");
    assert_eq!(event, EVENT_ACTION_CALL_REQUEST);
    assert_eq!(payload["actionId"], "wm.state.get");
    assert_eq!(payload["input"], json!({ "a": 1 }));
    assert!(bridge.resolve(
        request_id(&payload),
        ActionReply::Ok {
            result: json!({ "done": true })
        }
    ));
    assert_eq!(
        call.await.expect("the call task"),
        ActionReply::Ok {
            result: json!({ "done": true })
        }
    );
}

/// Spec 021 T009: the window learns who asks, so the runner applies that caller's rules.
#[tokio::test]
async fn a_call_names_its_caller() {
    let (bridge, mut events) = bridge_with_events(Duration::from_secs(5));
    let chat = bridge.clone();
    let builtin = tokio::spawn(async move {
        chat.call("wm.state.get", json!({}), &CancellationToken::new())
            .await
    });
    let (_, payload) = events.recv().await.expect("the chat's request");
    assert_eq!(payload["caller"], json!({ "kind": "builtinAgent" }));
    bridge.resolve(
        request_id(&payload),
        ActionReply::Ok {
            result: Value::Null,
        },
    );
    builtin.await.expect("the chat's call");

    let agent = bridge.clone();
    let external = tokio::spawn(async move {
        agent
            .call_as(
                ActionCaller::ExternalAgent {
                    agent_id: "a1".to_owned(),
                },
                "wm.state.get",
                json!({}),
                &CancellationToken::new(),
            )
            .await
    });
    let (_, payload) = events.recv().await.expect("the agent's request");
    assert_eq!(
        payload["caller"],
        json!({ "kind": "externalAgent", "agentId": "a1" })
    );
    bridge.resolve(
        request_id(&payload),
        ActionReply::Ok {
            result: Value::Null,
        },
    );
    external.await.expect("the agent's call");
}

#[tokio::test]
async fn a_missing_emitter_is_action_unavailable() {
    let reply = ActionBridge::default()
        .call("x", json!({}), &CancellationToken::new())
        .await;
    assert!(matches!(reply, ActionReply::Err { ref code, .. } if code == "action_unavailable"));
}

#[tokio::test]
async fn no_answer_in_time_is_action_timeout_and_a_late_answer_does_nothing() {
    let (bridge, mut events) = bridge_with_events(Duration::from_millis(40));
    let reply = bridge.call("x", json!({}), &CancellationToken::new()).await;
    assert!(matches!(reply, ActionReply::Err { ref code, .. } if code == "action_timeout"));
    let (_, payload) = events.recv().await.expect("the request event");
    assert!(!bridge.resolve(
        request_id(&payload),
        ActionReply::Ok {
            result: Value::Null
        }
    ));
}

#[tokio::test]
async fn a_cancelled_turn_ends_the_wait_and_drops_the_entry() {
    let (bridge, mut events) = bridge_with_events(Duration::from_secs(5));
    let cancel = CancellationToken::new();
    let caller = bridge.clone();
    let token = cancel.clone();
    let call = tokio::spawn(async move { caller.call("x", json!({}), &token).await });
    let (_, payload) = events.recv().await.expect("the request event");
    cancel.cancel();
    let reply = call.await.expect("the call task");
    assert!(matches!(reply, ActionReply::Err { ref code, .. } if code == "tool_call_cancelled"));
    assert!(!bridge.resolve(
        request_id(&payload),
        ActionReply::Ok {
            result: Value::Null
        }
    ));
}

#[tokio::test]
async fn dropping_the_pending_calls_ends_them_as_cancelled() {
    let (bridge, mut events) = bridge_with_events(Duration::from_secs(5));
    let caller = bridge.clone();
    let call =
        tokio::spawn(async move { caller.call("x", json!({}), &CancellationToken::new()).await });
    let _ = events.recv().await.expect("the request event");
    bridge.drop_pending();
    let reply = call.await.expect("the call task");
    assert!(matches!(reply, ActionReply::Err { ref code, .. } if code == "tool_call_cancelled"));
}

#[tokio::test]
async fn two_calls_run_one_after_the_other() {
    let (bridge, mut events) = bridge_with_events(Duration::from_secs(5));
    let order = Arc::new(Mutex::new(Vec::<&'static str>::new()));
    let first = {
        let bridge = bridge.clone();
        let order = Arc::clone(&order);
        tokio::spawn(async move {
            let reply = bridge
                .call("first", json!({}), &CancellationToken::new())
                .await;
            order.lock().unwrap().push("first done");
            reply
        })
    };
    let (_, first_payload) = events.recv().await.expect("the first request");
    let second = {
        let bridge = bridge.clone();
        let order = Arc::clone(&order);
        tokio::spawn(async move {
            let reply = bridge
                .call("second", json!({}), &CancellationToken::new())
                .await;
            order.lock().unwrap().push("second done");
            reply
        })
    };
    // The second call must not be emitted while the first one is still open.
    assert!(
        tokio::time::timeout(Duration::from_millis(80), events.recv())
            .await
            .is_err(),
        "the second call went out before the first was answered"
    );
    bridge.resolve(
        request_id(&first_payload),
        ActionReply::Ok {
            result: Value::Null,
        },
    );
    let (_, second_payload) = events.recv().await.expect("the second request");
    assert_eq!(second_payload["actionId"], "second");
    bridge.resolve(
        request_id(&second_payload),
        ActionReply::Ok {
            result: Value::Null,
        },
    );
    first.await.expect("first");
    second.await.expect("second");
    assert_eq!(*order.lock().unwrap(), ["first done", "second done"]);
}

#[test]
fn a_failed_handler_message_is_replaced_but_a_validation_message_stays() {
    let wire = |json: Value| -> ActionReply {
        serde_json::from_value::<ActionOutcomeWire>(json)
            .expect("a wire outcome")
            .into()
    };
    assert_eq!(
        wire(json!({ "ok": false, "code": "failed", "message": "cannot read /home/x/vault.db" })),
        ActionReply::Err {
            code: "failed".into(),
            field: None,
            message: ACTION_FAILED_MESSAGE.into(),
        }
    );
    assert_eq!(
        wire(
            json!({ "ok": false, "code": "invalid_input", "field": "text", "message": "text must be a string" })
        ),
        ActionReply::Err {
            code: "invalid_input".into(),
            field: Some("text".into()),
            message: "text must be a string".into(),
        }
    );
    assert_eq!(
        wire(json!({ "ok": true, "result": { "done": true } })),
        ActionReply::Ok {
            result: json!({ "done": true })
        }
    );
    assert_eq!(
        wire(json!({ "ok": true })),
        ActionReply::Ok {
            result: Value::Null
        }
    );
}

#[test]
fn a_needs_choice_outcome_keeps_its_message_and_candidates() {
    let reply: ActionReply = serde_json::from_value::<ActionOutcomeWire>(json!({
        "ok": false,
        "code": "needs_choice",
        "field": "appId",
        "message": "no app matches haex unambiguously",
        "options": [
            { "value": "extension.mail", "label": "haex-mail" },
            { "value": "extension.files", "label": "haex-files", "unavailable": "Wird übertragen" }
        ]
    }))
    .expect("a wire outcome")
    .into();
    assert_eq!(
        reply,
        ActionReply::NeedsChoice {
            field: "appId".into(),
            message: "no app matches haex unambiguously".into(),
            options: vec![
                ChoiceOption {
                    value: "extension.mail".into(),
                    label: "haex-mail".into(),
                    unavailable: None,
                },
                ChoiceOption {
                    value: "extension.files".into(),
                    label: "haex-files".into(),
                    unavailable: Some("Wird übertragen".into()),
                },
            ],
        }
    );
}

#[test]
fn a_needs_choice_outcome_without_a_field_stays_an_error() {
    let reply: ActionReply = serde_json::from_value::<ActionOutcomeWire>(json!({
        "ok": false, "code": "needs_choice", "message": "pick one", "options": []
    }))
    .expect("a wire outcome")
    .into();
    assert_eq!(
        reply,
        ActionReply::Err {
            code: "needs_choice".into(),
            field: None,
            message: "pick one".into(),
        }
    );
}
