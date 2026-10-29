use serde_json::json;

use super::grants::{
    browser_from_rows, browser_rows, check, default_rights, offers, validate, BrowserRights,
    InvalidRights, OperateLevel, Rights, SiteGrant, SiteLevel, OPERATE_SCOPES,
};
use crate::agent_browser::sites::Site;
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
        browser: None,
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
