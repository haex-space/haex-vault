use std::sync::Arc;

use async_trait::async_trait;
use serde_json::json;
use uuid::Uuid;

use super::grants::{
    browser_from_rows, browser_rows, check, check_extension_tool, default_rights, extension_offers,
    offers, validate, BrowserRights, DataRights, InvalidRights, OperateLevel, PasswordLevel,
    PasswordRights, Reach, Rights, SiteGrant, SiteLevel, OPERATE_SCOPES,
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
use crate::files::access::{AgentGrants, DeviceGrant, FolderGrant, Locality, StorageGrant};
use crate::files::permissions::AgentFileStatus;

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
            names(offers(
                &only(scope, OperateLevel::Read),
                &Reach::default(),
                &catalog
            )),
            [format!("{scope}.read")],
            "{scope}: reading offers only reading"
        );
        assert_eq!(
            names(offers(
                &only(scope, OperateLevel::Write),
                &Reach::default(),
                &catalog
            )),
            [
                format!("{scope}.read"),
                format!("{scope}.write"),
                format!("{scope}.destroy")
            ],
            "{scope}: changing offers read, write and destructive"
        );
    }
    assert!(
        offers(&Rights::default(), &Reach::default(), &catalog).is_empty(),
        "no rights, no tools"
    );
}

#[test]
fn a_write_level_offers_changing_and_destroying() {
    let catalog = catalog();
    let rights = only("settings.device", OperateLevel::Write);
    assert!(check(
        &rights,
        &Reach::default(),
        &catalog,
        "settings_device_write"
    )
    .is_some());
    assert!(check(
        &rights,
        &Reach::default(),
        &catalog,
        "settings_device_destroy"
    )
    .is_some());
    let reading = only("settings.device", OperateLevel::Read);
    assert!(check(
        &reading,
        &Reach::default(),
        &catalog,
        "settings_device_write"
    )
    .is_none());
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
    assert!(check(
        &rights,
        &Reach::default(),
        &catalog,
        "settings_agents_mcp_enable"
    )
    .is_none());
    assert!(!names(offers(&rights, &Reach::default(), &catalog))
        .contains(&"settings.agents.mcp.enable".to_owned()));
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
        names(offers(&rights, &Reach::default(), &catalog)),
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
    assert!(check(
        &default_rights(),
        &Reach::default(),
        &catalog,
        "no_such_tool"
    )
    .is_none());
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
    assert!(
        !names(offers(&off, &Reach::default(), &catalog)).contains(&"browser.page.read".to_owned())
    );
    assert!(check(&off, &Reach::default(), &catalog, "browser_page_read").is_none());

    let on = Rights {
        browser: Some(BrowserRights::default()),
        ..default_rights()
    };
    assert!(
        names(offers(&on, &Reach::default(), &catalog)).contains(&"browser.page.read".to_owned())
    );
    assert!(check(&on, &Reach::default(), &catalog, "browser_page_read").is_some());
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

fn native_action(id: &str, scope: &str, effect: &str) -> AgentActionDef {
    serde_json::from_value(json!({
        "toolName": id.replace('.', "_"),
        "actionId": id,
        "description": "d",
        "inputSchema": { "type": "object" },
        "effect": effect,
        "scope": scope,
        "runner": "native",
    }))
    .expect("definition")
}

/// US5, research R3: a data scope opens once any row covers its kind and action; an external agent
/// has no device grant without a row, and a denied `*` takes all contacts away.
#[test]
fn rows_of_agent_file_permissions_open_their_data_scopes() {
    assert_eq!(
        DataRights::of(&AgentGrants::default(), &[]),
        DataRights::default()
    );
    let device = AgentGrants {
        device: Some(DeviceGrant::Granted),
        ..AgentGrants::default()
    };
    let opened = DataRights::of(&device, &[]);
    assert!(opened.files_read && opened.files_write);
    let reading = AgentGrants::default().with_storage("s3", StorageGrant::Read);
    let opened = DataRights::of(&reading, &[]);
    assert!(opened.files_read && !opened.files_write);
    let denied = AgentGrants::default().with_storage("s3", StorageGrant::Denied);
    assert_eq!(DataRights::of(&denied, &[]), DataRights::default());
    let mut folder = AgentGrants::default();
    folder.folders.insert(
        "s3/f1".to_owned(),
        FolderGrant {
            grant: StorageGrant::ReadWrite,
            reach: Locality::Cloud,
        },
    );
    assert!(DataRights::of(&folder, &[]).files_write);

    let contacts = |rows: &[(&str, AgentFileStatus)]| {
        let rows: Vec<_> = rows
            .iter()
            .map(|(target, status)| ((*target).to_owned(), *status))
            .collect();
        DataRights::of(&AgentGrants::default(), &rows)
    };
    let reading = contacts(&[("*", AgentFileStatus::Read)]);
    assert!(reading.contacts_read && !reading.contacts_write);
    assert!(contacts(&[("group:g1", AgentFileStatus::ReadWrite)]).contacts_write);
    assert_eq!(
        contacts(&[
            ("*", AgentFileStatus::Denied),
            ("group:g1", AgentFileStatus::Read)
        ]),
        DataRights::default()
    );
}

/// FR-012, FR-020: file and contact tools need a row of the agent and a native executor that
/// serves external agents; a native action whose executor serves only the built-in agent is never
/// offered, whatever the rights.
#[test]
fn data_tools_need_a_row_and_an_executor_for_external_agents() {
    let catalog = vec![
        native_action("files.list", "files.read", "read"),
        native_action("files.delete", "files.write", "destructive"),
        native_action("contacts.get", "contacts.read", "read"),
        native_action("contacts.share", "contacts.read", "destructive"),
        action("files.show", "files.write", "read", true),
    ];
    let served = ["files.list", "files.delete", "contacts.get"];
    let reach = |data: DataRights| Reach {
        data,
        native: served.into_iter().collect(),
    };
    let rights = default_rights();
    assert!(offers(&rights, &reach(DataRights::default()), &catalog).is_empty());

    let everything = reach(DataRights {
        files_read: true,
        files_write: true,
        contacts_read: true,
        contacts_write: true,
    });
    assert_eq!(
        names(offers(&rights, &everything, &catalog)),
        ["files.list", "files.delete", "contacts.get"]
    );
    assert!(check(&rights, &everything, &catalog, "contacts_share").is_none());
    assert!(check(&rights, &everything, &catalog, "files_show").is_none());

    let reading = reach(DataRights {
        files_read: true,
        ..DataRights::default()
    });
    assert_eq!(names(offers(&rights, &reading, &catalog)), ["files.list"]);
    assert!(check(&rights, &reading, &catalog, "files_delete").is_none());
}

/// US5, FR-019: the group `passwords` opens the search and the single secret, nothing else; its
/// grant reaches the tags it names, or every entry without tags.
#[test]
fn the_password_group_opens_search_and_secret_within_its_tags() {
    let catalog = vec![
        action("passwords.items.search", "passwords.read", "read", true),
        action("passwords.secret.get", "passwords.secret", "read", false),
    ];
    assert!(offers(&default_rights(), &Reach::default(), &catalog).is_empty());
    let dev = PasswordRights {
        level: PasswordLevel::Read,
        tags: Some(vec!["dev".to_owned()]),
    };
    let rights = Rights {
        passwords: Some(dev.clone()),
        ..default_rights()
    };
    assert_eq!(
        names(offers(&rights, &Reach::default(), &catalog)),
        ["passwords.items.search", "passwords.secret.get"]
    );
    assert_eq!(
        dev.grants(),
        [crate::passwords::access::Grant::new(
            crate::passwords::access::GrantAction::Read,
            crate::passwords::access::Scope::tags(["dev"])
        )]
    );
    let every = PasswordRights {
        level: PasswordLevel::ReadWrite,
        tags: None,
    };
    assert_eq!(
        every.grants(),
        [crate::passwords::access::Grant::new(
            crate::passwords::access::GrantAction::ReadWrite,
            crate::passwords::access::Scope::All
        )]
    );
    let empty = Rights {
        passwords: Some(PasswordRights {
            level: PasswordLevel::Read,
            tags: Some(vec![" ".to_owned()]),
        }),
        ..default_rights()
    };
    assert_eq!(validate(&empty), Err(InvalidRights::EmptyTags));
}
