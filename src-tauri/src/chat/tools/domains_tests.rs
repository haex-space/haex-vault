use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use super::action_bridge::ActionBridge;
use super::action_tool::ActionTool;
use super::domains::{action_domain, domain_of, domains, topic_word, AgentDomain};
use super::{RiskClass, Tool, ToolOrigin, ToolRegistry, ToolResult};
use crate::chat::eval::{embedded_domains, embedded_tools};

/// An extension's tool as far as the domains look at it.
pub(super) struct Extension {
    pub name: &'static str,
    pub title: &'static str,
    pub risk: RiskClass,
    pub example: Option<&'static str>,
}

#[async_trait]
impl Tool for Extension {
    fn name(&self) -> &str {
        self.name
    }
    fn description(&self) -> &str {
        "d"
    }
    fn source(&self) -> &'static str {
        "haextension"
    }
    fn input_schema(&self) -> Value {
        json!({ "type": "object" })
    }
    fn risk_class(&self) -> RiskClass {
        self.risk
    }
    fn origin(&self) -> Option<ToolOrigin> {
        Some(ToolOrigin {
            extension_id: "6f1c1f6e-0000-4000-8000-000000000001".to_owned(),
            extension_name: "haex-mail".to_owned(),
            tool_title_de: String::new(),
            tool_title_en: self.title.to_owned(),
            dev: false,
            example_en: self.example.map(str::to_owned),
        })
    }
    async fn execute(&self, _: Value, _: CancellationToken) -> ToolResult {
        ToolResult::ok("{}")
    }
}

/// The registry a chat holds: holzi's actions with their domains, and `extensions`.
pub(super) fn registry_with(extensions: Vec<Extension>) -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    for def in embedded_tools() {
        registry.register(Arc::new(ActionTool::new(def, ActionBridge::default())));
    }
    for tool in extensions {
        registry.register(Arc::new(tool));
    }
    registry.set_domains(embedded_domains());
    registry
}

pub(super) fn mail() -> Vec<Extension> {
    vec![
        Extension {
            name: "x_mail_find_messages",
            title: "Search mail",
            risk: RiskClass::Safe,
            example: None,
        },
        Extension {
            name: "x_mail_delete_messages",
            title: "Delete mail for good",
            risk: RiskClass::Risky,
            example: None,
        },
    ]
}

#[test]
fn the_domains_are_read_from_the_snapshot() {
    let keys: Vec<String> = embedded_domains().into_iter().map(|d| d.key).collect();
    for key in [
        "wm",
        "browser",
        "contacts",
        "files",
        "passwords",
        "settings",
    ] {
        assert!(keys.contains(&key.to_owned()), "{key}");
    }
    let passwords = embedded_domains()
        .into_iter()
        .find(|d| d.key == "passwords")
        .unwrap();
    assert_eq!(passwords.also_needs, ["wm_app_open"]);
}

#[test]
fn every_action_falls_into_the_declared_domain_of_its_id_prefix() {
    let registry = registry_with(Vec::new());
    let declared: Vec<String> = embedded_domains().into_iter().map(|d| d.key).collect();
    for tool in registry.iter() {
        let Some(def) = tool.action_definition() else {
            continue;
        };
        let domain = domain_of(tool.as_ref()).unwrap();
        assert_eq!(domain, action_domain(&def.action_id));
        assert!(declared.contains(&domain), "{}", def.action_id);
    }
}

#[test]
fn the_search_ask_user_and_the_command_tool_belong_to_no_domain() {
    let registry = registry_with(Vec::new());
    for name in ["find_actions", "ask_user"] {
        if let Some(tool) = registry.get(name) {
            assert_eq!(domain_of(tool.as_ref()), None, "{name}");
        }
    }
    let cli = super::cli::CliTool::default();
    assert_eq!(domain_of(&cli), None);
}

#[test]
fn an_installed_extension_is_one_domain_named_by_what_its_tools_are_about() {
    let installed = domains(&registry_with(mail()));
    let domain = installed
        .iter()
        .find(|d| d.key == "x_mail")
        .expect("x_mail");
    assert_eq!(domain.name, "mail");
    assert_eq!(
        domain.description,
        "haex-mail: search mail, delete mail for good"
    );
    assert_eq!(domain.also_needs, ["wm_app_open"]);
    // The declared domains come first, in their order.
    assert_eq!(installed[0].key, "wm");
}

#[test]
fn the_extension_s_name_does_not_name_its_domain() {
    // A mail client called Thunderbird is still the mail.
    let installed = domains(&registry_with(vec![
        Extension {
            name: "x_thunderbird_find",
            title: "Search and filter mails",
            risk: RiskClass::Safe,
            example: None,
        },
        Extension {
            name: "x_thunderbird_read",
            title: "Read a mail",
            risk: RiskClass::Safe,
            example: None,
        },
    ]));
    let domain = installed.iter().find(|d| d.key == "x_thunderbird").unwrap();
    assert_eq!(domain.name, "mail");
}

#[test]
fn the_topic_is_the_word_most_titles_share() {
    let titles = |list: &[&str]| -> Vec<String> { list.iter().map(|t| t.to_string()).collect() };
    // haex-mail's titles: "mail" in fourteen of fifteen, mostly singular.
    let haex_mail = titles(&[
        "List mail folders and count unread mail",
        "Read a mail",
        "Mark mails as read or flagged",
        "Move mails to a folder",
        "Create a mail draft",
        "List mail accounts",
        "Search and filter mails",
        "Send a draft",
        "Delete mails for good",
    ]);
    assert_eq!(topic_word(&haex_mail).as_deref(), Some("mail"));
    assert_eq!(
        topic_word(&titles(&["Create a note", "List notes", "Search notes"])).as_deref(),
        Some("notes")
    );
    // Verbs and joining words never count; nothing in two titles names nothing.
    assert_eq!(topic_word(&titles(&["Find all", "Find the rest"])), None);
    assert_eq!(topic_word(&titles(&["Play music"])), None);
    assert_eq!(topic_word(&[]), None);
}

#[test]
fn an_extension_without_a_shared_word_is_named_by_its_prefix() {
    let installed = domains(&registry_with(vec![Extension {
        name: "x_radio_play",
        title: "Play a station",
        risk: RiskClass::Safe,
        example: None,
    }]));
    assert_eq!(
        installed.iter().find(|d| d.key == "x_radio").unwrap().name,
        "radio"
    );
}

#[test]
fn an_extension_that_is_not_installed_has_no_domain() {
    let installed = domains(&registry_with(Vec::new()));
    assert!(installed.iter().all(|d| !d.key.starts_with("x_")));
}

#[test]
fn a_declared_domain_without_tools_is_not_installed() {
    let mut registry = registry_with(Vec::new());
    let mut declared = embedded_domains();
    declared.push(AgentDomain {
        key: "calendar".to_owned(),
        name: "calendar".to_owned(),
        titles: Default::default(),
        description: "appointments".to_owned(),
        also_needs: Vec::new(),
    });
    registry.set_domains(declared);
    assert!(domains(&registry).iter().all(|d| d.key != "calendar"));
}

#[test]
fn an_extension_prefix_a_declared_domain_uses_keeps_its_key_as_name() {
    let installed = domains(&registry_with(vec![Extension {
        name: "x_files_list",
        title: "List",
        risk: RiskClass::Safe,
        example: None,
    }]));
    let domain = installed.iter().find(|d| d.key == "x_files").unwrap();
    assert_eq!(domain.name, "x_files");
}

#[test]
fn an_extension_domain_shows_the_first_two_example_requests_of_its_tools() {
    let tools = vec![
        Extension {
            name: "x_mail_list_mailboxes",
            title: "List mail folders",
            risk: RiskClass::Safe,
            example: None,
        },
        Extension {
            name: "x_mail_get_message",
            title: "Read a mail",
            risk: RiskClass::Safe,
            example: Some("What does the mail from Anna say?"),
        },
        Extension {
            name: "x_mail_find_messages",
            title: "Search mail",
            risk: RiskClass::Safe,
            example: Some("Show me the mails from Anna"),
        },
        Extension {
            name: "x_mail_create_draft",
            title: "Create a mail draft",
            risk: RiskClass::Change,
            example: Some("Write to Bernd"),
        },
    ];
    let installed = domains(&registry_with(tools));
    let domain = installed.iter().find(|d| d.key == "x_mail").unwrap();
    assert_eq!(
        domain.description,
        "haex-mail: list mail folders, read a mail, search mail, create a mail draft \
         (e.g. \"What does the mail from Anna say?\", \"Show me the mails from Anna\")"
    );
}
