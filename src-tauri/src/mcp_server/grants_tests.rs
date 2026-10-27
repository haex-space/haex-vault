use serde_json::json;

use super::grants::{
    check, default_rights, offers, validate, InvalidRights, OperateLevel, Rights, OPERATE_SCOPES,
};
use crate::chat::tools::action_tool::AgentActionDef;

fn action(id: &str, scope: &str, effect: &str, builtin: bool) -> AgentActionDef {
    serde_json::from_value(json!({
        "toolName": id.replace('.', "_"),
        "actionId": id,
        "description": "d",
        "inputSchema": { "type": "object" },
        "effect": effect,
        "scope": scope,
        "builtinAgentCallable": builtin,
    }))
    .expect("definition")
}

/// One read, one write and one destructive action per scope of "holzi bedienen".
fn catalog() -> Vec<AgentActionDef> {
    let mut all = Vec::new();
    for scope in OPERATE_SCOPES {
        all.push(action(&format!("{scope}.read"), scope, "read", true));
        all.push(action(&format!("{scope}.write"), scope, "write", true));
        all.push(action(
            &format!("{scope}.destroy"),
            scope,
            "destructive",
            true,
        ));
    }
    all
}

fn names(offered: Vec<&AgentActionDef>) -> Vec<String> {
    offered.iter().map(|def| def.action_id.clone()).collect()
}

fn only(scope: &str, level: OperateLevel) -> Rights {
    Rights {
        operate: [(scope.to_owned(), level)].into_iter().collect(),
    }
}

#[test]
fn each_scope_and_level_offers_exactly_its_effects() {
    let catalog = catalog();
    for scope in OPERATE_SCOPES {
        assert_eq!(
            names(offers(&only(scope, OperateLevel::Read), &catalog, true)),
            [format!("{scope}.read")],
            "{scope}: reading offers only reading"
        );
        assert_eq!(
            names(offers(&only(scope, OperateLevel::Write), &catalog, true)),
            [
                format!("{scope}.read"),
                format!("{scope}.write"),
                format!("{scope}.destroy")
            ],
            "{scope}: changing offers read, write and destructive"
        );
    }
    assert!(
        offers(&Rights::default(), &catalog, true).is_empty(),
        "no rights, no tools"
    );
}

#[test]
fn without_prompts_only_reading_is_offered() {
    let catalog = catalog();
    let rights = only("settings.device", OperateLevel::Write);
    assert_eq!(
        names(offers(&rights, &catalog, false)),
        ["settings.device.read"]
    );
    assert!(check(&rights, &catalog, "settings_device_write", false).is_none());
    assert!(check(&rights, &catalog, "settings_device_write", true).is_some());
}

#[test]
fn guardrails_are_never_offered_and_never_granted() {
    let mut catalog = catalog();
    catalog.push(action(
        "settings.agents.mcp.enable",
        "guardrails",
        "write",
        false,
    ));
    let mut rights = default_rights();
    rights
        .operate
        .insert("guardrails".to_owned(), OperateLevel::Write);
    assert!(check(&rights, &catalog, "settings_agents_mcp_enable", true).is_none());
    assert!(
        !names(offers(&rights, &catalog, true)).contains(&"settings.agents.mcp.enable".to_owned())
    );
    assert_eq!(
        validate(&rights),
        Err(InvalidRights::Scope("guardrails".to_owned()))
    );
    assert_eq!(
        validate(&only("files.read", OperateLevel::Read)),
        Err(InvalidRights::Scope("files.read".to_owned())),
        "files are their own group"
    );
}

#[test]
fn actions_only_for_external_agents_are_offered() {
    let catalog = vec![action("chat.message.send", "chat.write", "write", false)];
    let rights = only("chat.write", OperateLevel::Write);
    assert_eq!(
        names(offers(&rights, &catalog, true)),
        ["chat.message.send"]
    );
}

#[test]
fn a_new_agent_may_read_everything_it_can_reach_and_nothing_more() {
    let rights = default_rights();
    assert_eq!(validate(&rights), Ok(()));
    assert_eq!(rights.operate.len(), OPERATE_SCOPES.len());
    assert!(rights
        .operate
        .values()
        .all(|level| *level == OperateLevel::Read));
}

#[test]
fn an_unknown_tool_is_refused_like_a_forbidden_one() {
    let catalog = catalog();
    assert!(check(&default_rights(), &catalog, "no_such_tool", true).is_none());
}
