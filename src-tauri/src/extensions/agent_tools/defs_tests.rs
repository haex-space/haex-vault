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
    let id = Uuid::new_v4();
    ExtensionSource {
        id,
        public_key: format!("{}{}", id.simple(), id.simple()),
        name: name.to_owned(),
        installed_at: Some(0),
        title: format!("{name} title"),
        description: None,
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

/// Model-facing names (research R8): `x_<slug>_<tool>`, valid for every provider. Of two
/// extensions whose names make the same slug, the one installed first keeps the bare slug; the
/// later one carries the first characters of its public key.
#[test]
fn names_are_valid_and_distinct_between_extensions() {
    let long = "a".repeat(32);
    let mut first = source(
        "my-tools",
        vec![tool(&long, ToolEffect::Read)],
        vec![row("read", &long, "granted")],
    );
    first.installed_at = Some(1);
    let mut second = source(
        "mytools",
        vec![tool(&long, ToolEffect::Read)],
        vec![row("read", &long, "granted")],
    );
    second.installed_at = Some(2);
    second.public_key = "C0FFEE".repeat(10) + "abcd";
    let other = source(
        "very-long-extension-name-for-tools",
        vec![tool("count", ToolEffect::Read)],
        vec![row("read", "count", "granted")],
    );
    // Read in the opposite order: who keeps the bare slug follows the installation, not the order.
    let defs = tool_defs(vec![second, first, other]);
    for def in &defs {
        assert!(is_valid_tool_name(&def.model_name), "{}", def.model_name);
    }
    assert_eq!(defs[0].model_name, format!("x_mytoolsc0ffee_{long}"));
    assert_eq!(defs[1].model_name, format!("x_mytools_{long}"));
    assert_eq!(defs[2].model_name, "x_verylongextensionnam_count");
}

/// Uppercase letters of an extension's name are lowercased, not dropped (#842).
#[test]
fn an_extension_name_with_uppercase_letters_keeps_every_letter() {
    let defs = tool_defs(vec![
        source(
            "Thunderbird",
            vec![tool("find", ToolEffect::Read)],
            vec![row("read", "find", "granted")],
        ),
        source(
            "MyNotes",
            vec![tool("list", ToolEffect::Read)],
            vec![row("read", "list", "granted")],
        ),
    ]);
    assert_eq!(names(&defs), ["x_thunderbird_find", "x_mynotes_list"]);
}

/// Installing a second extension with the same slug leaves the names of the first one alone, so
/// an agent that knows them keeps calling them; the suffix comes from the key, not the id, and so
/// survives a new extension id.
#[test]
fn a_later_extension_of_the_same_slug_does_not_rename_the_first() {
    let mut mail = source(
        "haex-mail",
        vec![tool("list_mailboxes", ToolEffect::Read)],
        vec![row("read", "list_mailboxes", "granted")],
    );
    mail.installed_at = Some(10);
    let alone = names(&tool_defs(vec![mail.clone()])).join(",");
    assert_eq!(alone, "x_haexmail_list_mailboxes");
    assert_eq!(tool_defs(vec![mail.clone()])[0].namesake_key, None);

    let mut fork = source(
        "haexmail",
        vec![tool("list_mailboxes", ToolEffect::Read)],
        vec![row("read", "list_mailboxes", "granted")],
    );
    fork.installed_at = Some(20);
    fork.public_key = "9f8e7d".repeat(10) + "0000";
    let both = tool_defs(vec![fork.clone(), mail.clone()]);
    // Both name the start of their key, the one with the bare slug too.
    assert_eq!(both[0].namesake_key.as_deref(), Some("9f8e7d"));
    assert_eq!(both[1].namesake_key.as_deref(), Some(&mail.public_key[..6]));
    assert_eq!(
        names(&both),
        [
            "x_haexmail9f8e7d_list_mailboxes",
            "x_haexmail_list_mailboxes"
        ]
    );

    fork.id = Uuid::new_v4();
    assert_eq!(
        names(&tool_defs(vec![mail, fork])),
        [
            "x_haexmail_list_mailboxes",
            "x_haexmail9f8e7d_list_mailboxes"
        ]
    );
}

/// An installed extension that is switched off or not ready on this device offers nothing but
/// still holds its name: a later extension of the same slug keeps its key suffix meanwhile, so a
/// call meant for the first one never reaches the other.
#[test]
fn an_extension_that_offers_nothing_here_keeps_its_name() {
    let mut mail = source("haex-mail", vec![], vec![]);
    mail.installed_at = Some(10);
    let mut fork = source(
        "haexmail",
        vec![tool("list_mailboxes", ToolEffect::Read)],
        vec![row("read", "list_mailboxes", "granted")],
    );
    fork.installed_at = Some(20);
    fork.public_key = "9f8e7d".repeat(10) + "0000";
    let defs = tool_defs(vec![mail, fork]);
    assert_eq!(names(&defs), ["x_haexmail9f8e7d_list_mailboxes"]);
    // The switched-off one is nothing the model could mix up, so no namesake is named.
    assert_eq!(defs[0].namesake_key, None);
}

/// A development version of an installed extension shares its slug and its key: the installed
/// one keeps the bare slug, the development version takes the key suffix, and a second
/// development version with the same key falls back to its id.
#[test]
fn development_versions_yield_to_installed_ones_and_never_collide() {
    let mail = |installed_at| {
        let mut s = source(
            "haex-mail",
            vec![tool("list_mailboxes", ToolEffect::Read)],
            vec![row("read", "list_mailboxes", "granted")],
        );
        s.public_key = "ab12cd".repeat(10) + "0000";
        s.installed_at = installed_at;
        s.dev = installed_at.is_none();
        s
    };
    let (dev, installed, dev2) = (mail(None), mail(Some(5)), mail(None));
    let ids = (dev.id, dev2.id);
    let defs = tool_defs(vec![dev, installed, dev2]);
    let by_id = |id| {
        defs.iter()
            .find(|d| d.extension_id == id)
            .map(|d| d.model_name.clone())
            .unwrap()
    };
    assert_eq!(defs[1].model_name, "x_haexmail_list_mailboxes");
    let dev_names = [by_id(ids.0), by_id(ids.1)];
    assert!(dev_names.contains(&"x_haexmailab12cd_list_mailboxes".to_owned()));
    assert_ne!(dev_names[0], dev_names[1]);
    for name in &dev_names {
        assert!(is_valid_tool_name(name), "{name}");
    }
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
