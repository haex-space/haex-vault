//! Tests for what an agent profile lets the local agent use (spec 083 US5, T055, SC-006).

use std::collections::BTreeSet;
use std::sync::Arc;

use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::{permits, permitted_summaries, refusal, retain_permitted, Missing};
use crate::adapters::types::ToolSpec;
use crate::chat::tools::action_tool::{AgentActionDef, ACTION_SOURCE};
use crate::chat::tools::extension_offer::EXTENSION_SOURCE;
use crate::chat::tools::{RiskClass, Tool, ToolOrigin, ToolRegistry, ToolResult};
use crate::mcp_server::grants::{
    default_rights, BrowserRights, OperateLevel, PasswordLevel, PasswordRights, Rights,
};

struct Fake {
    name: String,
    source: &'static str,
    def: Option<AgentActionDef>,
    origin: Option<ToolOrigin>,
}

#[async_trait::async_trait]
impl Tool for Fake {
    fn name(&self) -> &str {
        &self.name
    }
    fn description(&self) -> &str {
        "a test tool"
    }
    fn source(&self) -> &'static str {
        self.source
    }
    fn input_schema(&self) -> Value {
        json!({ "type": "object" })
    }
    fn risk_class(&self) -> RiskClass {
        RiskClass::Safe
    }
    fn action_definition(&self) -> Option<&AgentActionDef> {
        self.def.as_ref()
    }
    fn origin(&self) -> Option<ToolOrigin> {
        self.origin.clone()
    }
    fn extension_summary(&self) -> Option<crate::chat::tools::extension_offer::ExtensionSummary> {
        self.origin.as_ref().map(
            |origin| crate::chat::tools::extension_offer::ExtensionSummary {
                extension_id: origin.extension_id.clone(),
                title: origin.extension_name.clone(),
                description: Some("mail".to_owned()),
                tool_title: String::new(),
            },
        )
    }
    async fn execute(&self, _: Value, _: CancellationToken) -> ToolResult {
        ToolResult::ok("")
    }
}

fn action(id: &str, scope: &str, effect: &str) -> Arc<dyn Tool> {
    let def: AgentActionDef = serde_json::from_value(json!({
        "toolName": id.replace('.', "_"),
        "actionId": id,
        "description": "d",
        "inputSchema": { "type": "object" },
        "effect": effect,
        "scope": scope,
        "runner": "native",
    }))
    .expect("definition");
    Arc::new(Fake {
        name: def.tool_name.clone(),
        source: ACTION_SOURCE,
        def: Some(def),
        origin: None,
    })
}

fn extension(id: Uuid) -> Arc<dyn Tool> {
    Arc::new(Fake {
        name: "x_mail_count".to_owned(),
        source: EXTENSION_SOURCE,
        def: None,
        origin: Some(ToolOrigin {
            extension_id: id.to_string(),
            extension_name: "Mail".to_owned(),
            tool_title_de: String::new(),
            tool_title_en: String::new(),
            dev: false,
            example_en: None,
        }),
    })
}

fn command_tool() -> Arc<dyn Tool> {
    Arc::new(Fake {
        name: "run_command".to_owned(),
        source: "cli",
        def: None,
        origin: None,
    })
}

#[test]
fn a_model_in_standard_reads_but_has_no_browser_commands_passwords_or_extensions() {
    let standard = default_rights();
    assert!(permits(
        &standard,
        action("wm.state.get", "wm.read", "read").as_ref()
    )
    .is_ok());
    assert_eq!(
        permits(
            &standard,
            action("chat.send", "chat.write", "write").as_ref()
        ),
        Err(Missing::TopicWrite {
            topic: "chat.write".to_owned()
        })
    );
    assert_eq!(
        permits(
            &standard,
            action("browser.open", "browser", "write").as_ref()
        ),
        Err(Missing::Browser)
    );
    assert_eq!(
        permits(
            &standard,
            action("passwords.items.search", "passwords.read", "read").as_ref()
        ),
        Err(Missing::Passwords)
    );
    assert_eq!(
        permits(&standard, command_tool().as_ref()),
        Err(Missing::Commands)
    );
    assert_eq!(
        permits(&standard, extension(Uuid::from_u128(1)).as_ref()),
        Err(Missing::Extension {
            name: "Mail".to_owned()
        })
    );
    // Files and contacts keep the gate their executors apply.
    assert!(permits(
        &standard,
        action("files.read", "files.read", "read").as_ref()
    )
    .is_ok());
    assert!(permits(
        &standard,
        action("contacts.search", "contacts.read", "read").as_ref()
    )
    .is_ok());
}

#[test]
fn a_profile_that_gives_more_opens_exactly_that() {
    let mail = Uuid::from_u128(1);
    let rights = Rights {
        operate: [("chat.write".to_owned(), OperateLevel::Write)]
            .into_iter()
            .collect(),
        browser: Some(BrowserRights::default()),
        passwords: Some(PasswordRights {
            level: PasswordLevel::Read,
            tags: None,
            default_tag: None,
        }),
        extensions: BTreeSet::from([mail]),
        commands: true,
        ..Rights::default()
    };
    assert!(permits(&rights, action("chat.send", "chat.write", "write").as_ref()).is_ok());
    assert!(permits(&rights, action("browser.open", "browser", "write").as_ref()).is_ok());
    assert!(permits(
        &rights,
        action("passwords.items.search", "passwords.read", "read").as_ref()
    )
    .is_ok());
    assert!(permits(&rights, command_tool().as_ref()).is_ok());
    assert!(permits(&rights, extension(mail).as_ref()).is_ok());
    assert!(permits(&rights, extension(Uuid::from_u128(2)).as_ref()).is_err());
    // A topic the profile does not list gives nothing, and says so without naming a level.
    assert_eq!(
        permits(
            &rights,
            action("settings.get", "settings.read", "read").as_ref()
        ),
        Err(Missing::Topic {
            topic: "settings.read".to_owned()
        })
    );
    let all = Rights {
        all_extensions: true,
        ..Rights::default()
    };
    assert!(permits(&all, extension(Uuid::from_u128(77)).as_ref()).is_ok());
}

#[test]
fn guardrails_are_never_permitted_and_search_and_asking_always_are() {
    let everything = Rights {
        operate: [("chat.write".to_owned(), OperateLevel::Write)]
            .into_iter()
            .collect(),
        ..Rights::default()
    };
    assert!(permits(
        &everything,
        action("settings.guardrails.set", "guardrails", "write").as_ref()
    )
    .is_err());
    let ask = Fake {
        name: "ask_user".to_owned(),
        source: ACTION_SOURCE,
        def: None,
        origin: None,
    };
    assert!(permits(&Rights::default(), &ask).is_ok());
}

#[test]
fn the_offer_loses_what_the_profile_does_not_give_and_the_refusal_names_the_next_step() {
    let mut registry = ToolRegistry::new();
    registry.register(action("wm.state.get", "wm.read", "read"));
    registry.register(action("browser.open", "browser", "write"));
    registry.register(command_tool());
    let mut tools: Vec<ToolSpec> = [
        "wm_state_get",
        "browser_open",
        "run_command",
        "unknown_tool",
    ]
    .iter()
    .map(|name| ToolSpec {
        name: (*name).to_owned(),
        description: String::new(),
        input_schema: json!({ "type": "object" }),
    })
    .collect();
    let first = retain_permitted(&default_rights(), &registry, &mut tools);
    let names: Vec<&str> = tools.iter().map(|tool| tool.name.as_str()).collect();
    assert_eq!(names, ["wm_state_get", "unknown_tool"]);
    assert_eq!(first, Some(Missing::Browser));
    let answer = refusal(&Missing::Browser);
    assert!(answer.starts_with("not_allowed_by_profile"), "{answer}");
    assert!(!answer.to_lowercase().contains("only read"));
}

#[test]
fn the_prompt_names_only_the_extensions_the_profile_gives() {
    let (mail, other) = (Uuid::new_v4(), Uuid::new_v4());
    let mut registry = ToolRegistry::new();
    registry.register(extension(mail));
    registry.register(Arc::new(Fake {
        name: "x_other_count".to_owned(),
        source: EXTENSION_SOURCE,
        def: None,
        origin: Some(ToolOrigin {
            extension_id: other.to_string(),
            extension_name: "Other".to_owned(),
            tool_title_de: String::new(),
            tool_title_en: String::new(),
            dev: false,
            example_en: None,
        }),
    }));
    // "Standard" gives no extension: none is named.
    assert!(permitted_summaries(&default_rights(), &registry).is_empty());

    let mut rights = default_rights();
    rights.extensions = BTreeSet::from([mail]);
    let named: Vec<String> = permitted_summaries(&rights, &registry)
        .into_iter()
        .map(|summary| summary.title)
        .collect();
    assert_eq!(named, ["Mail"]);

    rights.all_extensions = true;
    assert_eq!(permitted_summaries(&rights, &registry).len(), 2);
}
