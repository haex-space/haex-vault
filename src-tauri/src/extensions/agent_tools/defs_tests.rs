use serde_json::json;
use uuid::Uuid;

use super::defs::{tool_defs, ExtensionSource, ToolRow};
use crate::chat::tools::action_tool::is_valid_tool_name;
use crate::extensions::bundle::{ManifestTool, ToolEffect};

fn tool(name: &str, effect: ToolEffect) -> ManifestTool {
    ManifestTool {
        name: name.to_owned(),
        title_de: format!("{name} de"),
        title_en: format!("{name} en"),
        description: format!("Does {name}"),
        input_schema: json!({ "type": "object" }),
        effect,
        examples_de: vec![],
        examples_en: vec![],
        supported: true,
    }
}

fn row(action: &str, target: &str, status: &str) -> ToolRow {
    ToolRow {
        action: action.to_owned(),
        target: target.to_owned(),
        status: status.to_owned(),
    }
}

fn source(name: &str, tools: Vec<ManifestTool>, rows: Vec<ToolRow>) -> ExtensionSource {
    ExtensionSource {
        id: Uuid::new_v4(),
        name: name.to_owned(),
        title: format!("{name} title"),
        tools,
        rows,
        dev: false,
    }
}

fn names(defs: &[super::defs::ExtensionToolDef]) -> Vec<&str> {
    defs.iter().map(|d| d.model_name.as_str()).collect()
}

/// The table "Wirkung je Werkzeug" of data-model.md.
#[test]
fn only_confirmed_tools_are_offered_and_ask_always_asks() {
    let mail = source(
        "haex-mail",
        vec![
            tool("granted_tool", ToolEffect::Read),
            tool("ask_tool", ToolEffect::Change),
            tool("denied_tool", ToolEffect::Read),
            tool("unconfirmed", ToolEffect::Read),
        ],
        vec![
            row("read", "granted_tool", "granted"),
            row("change", "ask_tool", "ask"),
            row("read", "denied_tool", "denied"),
        ],
    );
    let defs = tool_defs(vec![mail]);
    assert_eq!(
        names(&defs),
        ["x_haexmail_granted_tool", "x_haexmail_ask_tool"]
    );
    assert!(!defs[0].always_ask);
    assert!(defs[1].always_ask);
}

#[test]
fn the_switch_off_row_hides_every_tool_of_the_extension() {
    let mail = source(
        "haex-mail",
        vec![tool("count_unread", ToolEffect::Read)],
        vec![
            row("read", "count_unread", "granted"),
            row("*", "*", "denied"),
        ],
    );
    assert!(tool_defs(vec![mail]).is_empty());
}

/// A row confirms a tool only with the effect the manifest now declares: an update that changed
/// the effect asks again (research R4).
#[test]
fn a_row_for_another_effect_does_not_confirm_the_tool() {
    let mail = source(
        "haex-mail",
        vec![tool("archive", ToolEffect::Destructive)],
        vec![row("change", "archive", "granted")],
    );
    assert!(tool_defs(vec![mail]).is_empty());
}

#[test]
fn a_tool_outside_the_schema_subset_is_never_offered() {
    let mut unsupported = tool("strange", ToolEffect::Read);
    unsupported.supported = false;
    let mail = source(
        "haex-mail",
        vec![unsupported],
        vec![row("read", "strange", "granted")],
    );
    assert!(tool_defs(vec![mail]).is_empty());
}

/// Model-facing names (research R8): `x_<slug>_<tool>`, valid for every provider, and two
/// extensions whose names make the same slug keep apart by a part of their ids.
#[test]
fn names_are_valid_and_distinct_between_extensions() {
    let long = "a".repeat(32);
    let first = source(
        "my-tools",
        vec![tool(&long, ToolEffect::Read)],
        vec![row("read", &long, "granted")],
    );
    let second = source(
        "mytools",
        vec![tool(&long, ToolEffect::Read)],
        vec![row("read", &long, "granted")],
    );
    let other = source(
        "very-long-extension-name-for-tools",
        vec![tool("count", ToolEffect::Read)],
        vec![row("read", "count", "granted")],
    );
    let ids = (first.id, second.id);
    let defs = tool_defs(vec![first, second, other]);
    for def in &defs {
        assert!(is_valid_tool_name(&def.model_name), "{}", def.model_name);
    }
    assert_ne!(defs[0].model_name, defs[1].model_name);
    assert!(defs[0]
        .model_name
        .contains(&ids.0.simple().to_string()[..6]));
    assert!(defs[1]
        .model_name
        .contains(&ids.1.simple().to_string()[..6]));
    assert_eq!(defs[2].model_name, "x_verylongextensionnam_count");
}

#[test]
fn a_definition_keeps_what_the_call_and_the_display_need() {
    let mail = source(
        "haex-mail",
        vec![tool("count_unread", ToolEffect::Read)],
        vec![row("read", "count_unread", "granted")],
    );
    let id = mail.id;
    let def = tool_defs(vec![mail]).remove(0);
    assert_eq!(def.extension_id, id);
    assert_eq!(def.extension_title, "haex-mail title");
    assert_eq!(def.tool.name, "count_unread");
    assert!(!def.dev);
}
