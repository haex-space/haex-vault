use std::sync::Arc;

use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use super::action_bridge::{ActionBridge, ActionReply};
use super::action_tool::{
    is_valid_tool_name, schema_in_subset, ActionEffect, ActionTool, AgentActionDef,
};
use super::{ChoiceOption, ChoiceRequest, RiskClass, Tool};

fn def(effect: ActionEffect) -> AgentActionDef {
    serde_json::from_value(json!({
        "toolName": "wm_tab_close",
        "actionId": "wm.tab.close",
        "description": "Close a tab.",
        "inputSchema": { "type": "object", "properties": { "tabId": { "type": "string" } } },
        "effect": match effect {
            ActionEffect::Read => "read",
            ActionEffect::Write => "write",
            ActionEffect::Destructive => "destructive",
        },
        "core": true,
        "titles": { "de": "Tab schließen", "en": "Close tab" },
    }))
    .expect("a valid definition")
}

#[test]
fn the_effect_decides_the_risk_class() {
    for (effect, risk) in [
        (ActionEffect::Read, RiskClass::Safe),
        (ActionEffect::Write, RiskClass::Change),
        (ActionEffect::Destructive, RiskClass::Risky),
    ] {
        let tool = ActionTool::new(def(effect), ActionBridge::default());
        assert_eq!(tool.risk_class(), risk, "{effect:?}");
    }
}

#[test]
fn name_source_description_and_schema_pass_through_unchanged() {
    let definition = def(ActionEffect::Write);
    let tool = ActionTool::new(definition.clone(), ActionBridge::default());
    assert_eq!(tool.name(), "wm_tab_close");
    assert_eq!(tool.source(), "action");
    assert_eq!(tool.description(), "Close a tab.");
    assert_eq!(tool.input_schema(), definition.input_schema);
    assert!(tool.def().core);
    assert_eq!(tool.def().titles.de, "Tab schließen");
}

#[test]
fn a_definition_without_core_and_titles_still_deserializes() {
    let parsed: AgentActionDef = serde_json::from_value(json!({
        "toolName": "a",
        "actionId": "a",
        "description": "d",
        "inputSchema": { "type": "object" },
        "effect": "read",
    }))
    .expect("optional fields may be absent");
    assert!(!parsed.core);
    assert_eq!(parsed.titles.en, "");
}

#[test]
fn tool_names_keep_to_the_provider_pattern() {
    assert!(is_valid_tool_name("wm_tab_back"));
    assert!(is_valid_tool_name("settings_appearance_setColorScheme"));
    assert!(is_valid_tool_name(&"a".repeat(64)));
    assert!(!is_valid_tool_name(""));
    assert!(!is_valid_tool_name(&"a".repeat(65)));
    assert!(!is_valid_tool_name("wm.tab.back"));
    assert!(!is_valid_tool_name("mcp:server:tool"));
    assert!(!is_valid_tool_name("tab schließen"));
}

#[test]
fn schemas_outside_the_runner_subset_are_rejected() {
    assert!(schema_in_subset(&json!({ "type": "object" })));
    assert!(schema_in_subset(&json!({
        "type": "object",
        "properties": {
            "tabId": { "type": "string", "description": "x" },
            "mode": { "type": "string", "enum": ["a", "b"] },
            "ids": { "type": "array", "items": { "type": "integer" } },
        },
        "required": ["tabId"],
    })));
    assert!(!schema_in_subset(
        &json!({ "type": "object", "additionalProperties": false })
    ));
    assert!(!schema_in_subset(&json!({ "type": "null" })));
    assert!(!schema_in_subset(&json!({
        "type": "object",
        "properties": { "x": { "type": "string", "pattern": "^a" } },
    })));
    assert!(!schema_in_subset(&Value::Null));
}

#[tokio::test]
async fn an_action_tool_without_emitter_reports_the_marker() {
    let tool = ActionTool::new(def(ActionEffect::Read), ActionBridge::default());
    let result = tool.execute(json!({}), CancellationToken::new()).await;
    assert!(result.is_error);
    let error: Value = serde_json::from_str(&result.content).expect("a JSON error");
    assert_eq!(error["error"]["code"], "action_unavailable");
    assert!(error["error"]["next"].is_string(), "{}", result.content);
    let _registry: Arc<dyn Tool> = Arc::new(tool);
}

#[tokio::test]
async fn an_action_asking_to_choose_carries_the_choice_to_the_tool_round() {
    let bridge = ActionBridge::default();
    let (sender, mut requests) = tokio::sync::mpsc::unbounded_channel();
    bridge.set_emitter(Arc::new(move |_event: &str, payload: Value| {
        let _ = sender.send(payload);
    }));
    let tool = ActionTool::new(def(ActionEffect::Write), bridge.clone());
    let call = tokio::spawn(async move {
        tool.execute(json!({ "tabId": "haex" }), CancellationToken::new())
            .await
    });
    let request = requests.recv().await.expect("the action request");
    let options = vec![ChoiceOption {
        value: "tab-1".into(),
        label: "haex-mail".into(),
        unavailable: None,
    }];
    assert!(bridge.resolve(
        serde_json::from_value(request["requestId"].clone()).expect("a request id"),
        ActionReply::NeedsChoice {
            field: "tabId".into(),
            message: "no tab matches haex".into(),
            options: options.clone(),
        },
    ));
    let result = call.await.expect("the call task");
    assert!(result.is_error);
    assert_eq!(
        result.choice,
        Some(ChoiceRequest {
            question: None,
            field: Some("tabId".into()),
            value: "haex".into(),
            options,
        })
    );
    let content: Value = serde_json::from_str(&result.content).expect("JSON content");
    assert_eq!(content["error"]["code"], "needs_choice");
    assert_eq!(content["error"]["field"], "tabId");
    assert_eq!(content["error"]["options"][0]["label"], "haex-mail");
}
