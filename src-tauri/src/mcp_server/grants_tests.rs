use std::sync::Arc;

use async_trait::async_trait;
use serde_json::json;
use uuid::Uuid;

use super::grants::{
    browser_from_rows, browser_rows, check, check_extension_tool, default_rights, extension_offers,
    offers, validate, BrowserRights, InvalidRights, OperateLevel, Rights, SiteGrant, SiteLevel,
    OPERATE_SCOPES,
};
use crate::agent_browser::sites::Site;
use crate::chat::tools::action_tool::AgentActionDef;
use crate::chat::tools::ask_user::AskUserTool;
use crate::chat::tools::Tool;
use crate::extensions::agent_tools::defs::ExtensionToolDef;
use crate::extensions::agent_tools::slot::McpLink;
use crate::extensions::agent_tools::tool::{ExtensionTool, Links};
use crate::extensions::bridge::frames::FrameSession;
use crate::extensions::bundle::{ManifestTool, ToolEffect};

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
        browser: None,
        ..Default::default()
    }
}

#[test]
fn each_scope_and_level_offers_exactly_its_effects() {
    let catalog = catalog();
    for scope in OPERATE_SCOPES {
        assert_eq!(
            names(offers(&only(scope, OperateLevel::Read), &catalog)),
            [format!("{scope}.read")],
            "{scope}: reading offers only reading"
        );
        assert_eq!(
            names(offers(&only(scope, OperateLevel::Write), &catalog)),
            [
                format!("{scope}.read"),
                format!("{scope}.write"),
                format!("{scope}.destroy")
            ],
            "{scope}: changing offers read, write and destructive"
        );
    }
    assert!(
        offers(&Rights::default(), &catalog).is_empty(),
        "no rights, no tools"
    );
}

#[test]
fn a_write_level_offers_changing_and_destroying() {
    let catalog = catalog();
    let rights = only("settings.device", OperateLevel::Write);
    assert!(check(&rights, &catalog, "settings_device_write").is_some());
    assert!(check(&rights, &catalog, "settings_device_destroy").is_some());
    let reading = only("settings.device", OperateLevel::Read);
    assert!(check(&reading, &catalog, "settings_device_write").is_none());
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
    assert!(check(&rights, &catalog, "settings_agents_mcp_enable").is_none());
    assert!(!names(offers(&rights, &catalog)).contains(&"settings.agents.mcp.enable".to_owned()));
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
    assert_eq!(names(offers(&rights, &catalog)), ["chat.message.send"]);
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
    assert!(check(&default_rights(), &catalog, "no_such_tool").is_none());
}

// ---------------------------------------------------------------------------
// The group `browser` (spec 065 T007)
// ---------------------------------------------------------------------------

fn browser(sites: &[(&str, SiteLevel)]) -> BrowserRights {
    BrowserRights {
        user_browser: false,
        sites: sites
            .iter()
            .map(|(site, level)| {
                (
                    Site::parse(site).expect("site"),
                    SiteGrant {
                        level: *level,
                        fill_remembered: false,
                    },
                )
            })
            .collect(),
    }
}

#[test]
fn the_browser_tools_need_the_group_switched_on() {
    let catalog = vec![
        action("browser.page.read", "browser", "read", true),
        action("settings.get", "settings.read", "read", true),
    ];
    let off = default_rights();
    assert!(!names(offers(&off, &catalog)).contains(&"browser.page.read".to_owned()));
    assert!(check(&off, &catalog, "browser_page_read").is_none());

    let on = Rights {
        browser: Some(BrowserRights::default()),
        ..default_rights()
    };
    assert!(names(offers(&on, &catalog)).contains(&"browser.page.read".to_owned()));
    assert!(check(&on, &catalog, "browser_page_read").is_some());
}

#[test]
fn a_plain_http_website_can_only_be_read() {
    let reading = Rights {
        browser: Some(browser(&[("http://intranet.example", SiteLevel::Read)])),
        ..default_rights()
    };
    assert_eq!(validate(&reading), Ok(()));
    let operating = Rights {
        browser: Some(browser(&[("http://intranet.example", SiteLevel::Operate)])),
        ..default_rights()
    };
    assert_eq!(
        validate(&operating),
        Err(InvalidRights::PlainSite(
            "http://intranet.example".to_owned()
        ))
    );
}

#[test]
fn the_window_cannot_send_a_website_that_is_none() {
    for bad in ["file:///etc", "javascript:x", "https://u:p@example.com"] {
        let sent = json!({ "operate": {}, "browser": { "sites": { bad: { "level": "read" } } } });
        assert!(
            serde_json::from_value::<Rights>(sent).is_err(),
            "{bad} must not reach the rights"
        );
    }
    let sent = json!({ "operate": {}, "browser": { "sites": { "https://EXAMPLE.com/x": { "level": "operate" } } } });
    let rights: Rights = serde_json::from_value(sent).expect("rights");
    let sites = rights.browser.expect("on").sites;
    assert!(sites.contains_key(&Site::parse("https://example.com").expect("site")));
}

#[test]
fn the_browser_group_round_trips_through_its_rows() {
    let mut rights = browser(&[
        ("https://shop.example.de", SiteLevel::Operate),
        ("http://intranet.example", SiteLevel::Read),
    ]);
    rights.user_browser = true;
    rights
        .sites
        .get_mut(&Site::parse("https://shop.example.de").expect("site"))
        .expect("grant")
        .fill_remembered = true;
    let rows = browser_rows(&rights);
    assert!(rows.contains(&(String::new(), "on".to_owned(), None)));
    assert_eq!(browser_from_rows(&rows), Some(rights));
    let without_switch: Vec<_> = rows
        .into_iter()
        .filter(|(key, _, _)| !key.is_empty())
        .collect();
    assert_eq!(
        browser_from_rows(&without_switch),
        None,
        "off without its switch"
    );
}

struct NoFrame;

#[async_trait]
impl Links for NoFrame {
    async fn open(&self, _: Uuid) -> Result<(Arc<FrameSession>, Arc<McpLink>), String> {
        Err("none".to_owned())
    }
}

fn extension_tool(extension_id: Uuid, model_name: &str) -> Arc<dyn Tool> {
    let def = ExtensionToolDef {
        extension_id,
        extension_title: "Mail".to_owned(),
        tool: ManifestTool {
            name: "count".to_owned(),
            title_de: "Zählen".to_owned(),
            title_en: "Count".to_owned(),
            description: "Counts".to_owned(),
            input_schema: json!({ "type": "object" }),
            effect: ToolEffect::Read,
            examples_de: vec![],
            examples_en: vec![],
            supported: true,
        },
        model_name: model_name.to_owned(),
        always_ask: false,
        dev: false,
    };
    Arc::new(ExtensionTool::new(def, Arc::new(NoFrame)))
}

/// US4: an agent sees and calls only the tools of the extensions granted to it; a new extension
/// is not granted, and holzi's own tools never come through this group.
#[test]
fn only_tools_of_granted_extensions_are_offered_and_callable() {
    let mail = Uuid::from_u128(1);
    let notes = Uuid::from_u128(2);
    let tools: Vec<Arc<dyn Tool>> = vec![
        extension_tool(mail, "x_mail_count"),
        extension_tool(notes, "x_notes_count"),
        Arc::new(AskUserTool),
    ];
    assert!(extension_offers(&default_rights(), &tools).is_empty());

    let rights = Rights {
        extensions: [mail].into_iter().collect(),
        ..default_rights()
    };
    let offered: Vec<&str> = extension_offers(&rights, &tools)
        .iter()
        .map(|tool| tool.name())
        .collect();
    assert_eq!(offered, ["x_mail_count"]);
    assert!(check_extension_tool(&rights, &tools, "x_mail_count").is_some());
    assert!(check_extension_tool(&rights, &tools, "x_notes_count").is_none());
    assert!(check_extension_tool(&rights, &tools, "ask_user").is_none());
    assert!(check_extension_tool(&rights, &tools, "x_mail_gone").is_none());
}
