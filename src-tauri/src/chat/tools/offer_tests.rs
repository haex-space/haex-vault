use serde_json::json;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

use super::action_bridge::ActionBridge;
use super::action_tool::ActionTool;
use super::action_tool::AgentActionDef;
use super::ask_user::AskUserTool;
use super::find_actions::FindActionsTool;
use super::offer::{core_offer, extend_offer, found_tools, search_actions, ExtensionScope};
use super::{Tool, ToolRegistry};
use crate::adapters::types::ToolSpec;

fn def(id: &str, description: &str, core: bool) -> AgentActionDef {
    serde_json::from_value(json!({
        "toolName": id.replace('.', "_"),
        "actionId": id,
        "description": description,
        "inputSchema": { "type": "object" },
        "effect": "read",
        "core": core,
        "titles": { "de": "", "en": "" },
    }))
    .expect("test action definition")
}

#[test]
fn search_ranks_word_hits_and_paginates_stably() {
    let defs = vec![
        def("settings.models.list", "List models", false),
        def("wm.apps.list", "List apps", true),
        def("settings.get", "Read all settings", true),
    ];

    let (page, next) = search_actions(&defs, Some("settings"), None, 2);
    assert_eq!(
        page.iter()
            .map(|action| action.action_id.as_str())
            .collect::<Vec<_>>(),
        ["settings.get", "settings.models.list"]
    );
    assert_eq!(next, None);

    let (page, next) = search_actions(&defs, None, None, 2);
    assert_eq!(page.len(), 2);
    assert_eq!(next.as_deref(), Some("2"));
    let (page, next) = search_actions(&defs, None, next.as_deref(), 2);
    assert_eq!(page.len(), 1);
    assert_eq!(next, None);
}

#[test]
fn extend_offer_replaces_non_core_actions_and_preserves_other_tools() {
    let core = def("wm.apps.list", "List apps", true);
    let first = def("settings.get", "Read settings", false);
    let second = def("settings.models.list", "List models", false);
    let defs = vec![core.clone(), first.clone(), second.clone()];
    let mut offer = vec![
        ToolSpec {
            name: "run_command".into(),
            description: "shell".into(),
            input_schema: json!({ "type": "object" }),
        },
        ToolSpec {
            name: core.tool_name.clone(),
            description: core.description.clone(),
            input_schema: core.input_schema.clone(),
        },
        ToolSpec {
            name: first.tool_name.clone(),
            description: first.description.clone(),
            input_schema: first.input_schema.clone(),
        },
    ];
    let found = vec![
        ToolSpec {
            name: second.tool_name.clone(),
            description: second.description.clone(),
            input_schema: second.input_schema.clone(),
        },
        ToolSpec {
            name: core.tool_name.clone(),
            description: "duplicate core".into(),
            input_schema: json!({}),
        },
    ];

    extend_offer(&mut offer, &found, &defs, &ExtensionScope::default());
    assert_eq!(
        offer
            .iter()
            .map(|tool| tool.name.as_str())
            .collect::<Vec<_>>(),
        ["run_command", "wm_apps_list", "settings_models_list",]
    );
}

#[test]
fn found_tools_only_accepts_the_search_result_shape() {
    let found = found_tools(
        &json!({
            "actions": [{
                "tool": "settings_get",
                "description": "Read settings",
                "inputSchema": { "type": "object" }
            }]
        })
        .to_string(),
    );
    assert_eq!(
        found.as_ref().expect("valid result")[0].name,
        "settings_get"
    );
    assert!(found_tools("not json").is_none());
    assert!(found_tools("{}").is_none());
}

#[test]
fn core_offer_keeps_non_core_actions_hidden_but_offers_the_search_tool() {
    let core = def("wm.apps.list", "List apps", true);
    let hidden = def("settings.get", "Read settings", false);
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(ActionTool::new(
        core.clone(),
        ActionBridge::default(),
    )));
    registry.register(Arc::new(ActionTool::new(hidden, ActionBridge::default())));
    registry.register(Arc::new(FindActionsTool::new(vec![core])));

    let names = core_offer(&registry)
        .into_iter()
        .map(|tool| tool.name)
        .collect::<Vec<_>>();
    assert_eq!(names, ["wm_apps_list", "find_actions"]);
}

#[tokio::test]
async fn find_actions_returns_bounded_search_results() {
    let defs = (0..7)
        .map(|index| def(&format!("settings.item{index}"), "Settings item", false))
        .collect();
    let tool = FindActionsTool::new(defs);
    let result = tool
        .execute(
            json!({ "query": "settings", "limit": 99 }),
            CancellationToken::new(),
        )
        .await;
    assert!(!result.is_error);
    let value: serde_json::Value = serde_json::from_str(&result.content).expect("JSON result");
    assert_eq!(value["actions"].as_array().expect("actions").len(), 5);
    assert_eq!(value["nextCursor"], "5");
}

#[test]
fn core_offer_always_offers_ask_user() {
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(ActionTool::new(
        def("settings.get", "Read settings", false),
        ActionBridge::default(),
    )));
    registry.register(Arc::new(AskUserTool));

    let names = core_offer(&registry)
        .into_iter()
        .map(|tool| tool.name)
        .collect::<Vec<_>>();
    assert_eq!(names, ["ask_user"]);
}

#[test]
fn search_matches_singular_and_plural_of_a_word() {
    let defs = vec![
        def(
            "wm.apps.list",
            "List the installed apps and extensions (Erweiterungen)",
            false,
        ),
        def("settings.get", "Read all settings", false),
    ];

    for query in ["extension", "extensions", "Erweiterung", "Erweiterungen"] {
        let (page, _) = search_actions(&defs, Some(query), None, 5);
        let ids = page
            .iter()
            .map(|action| action.action_id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(ids, ["wm.apps.list"], "query {query}");
    }

    // Below five letters a word only matches whole words.
    let (page, _) = search_actions(&defs, Some("set"), None, 5);
    assert!(page.is_empty());
}

#[test]
fn search_keeps_a_word_with_umlauts_whole() {
    let defs = vec![
        def("window.resize", "Größe ändern", false),
        def("entries.count", "Einträge zählen", false),
    ];
    let ids = |query| {
        search_actions(&defs, Some(query), None, 5)
            .0
            .iter()
            .map(|action| action.action_id.clone())
            .collect::<Vec<_>>()
    };

    assert!(ids("Grüße senden").is_empty());
    assert_eq!(ids("Einträge"), ["entries.count"]);
    // "Büch" is four letters but five bytes, so it does not match "Bücher" as a prefix.
    assert!(
        search_actions(&[def("books.list", "Bücher", false)], Some("Büch"), None, 5)
            .0
            .is_empty()
    );
}

#[tokio::test]
async fn find_actions_hint_says_a_miss_is_about_actions_not_data() {
    let tool = FindActionsTool::new(vec![def("settings.get", "Read all settings", false)]);
    let result = tool
        .execute(
            json!({ "query": "erweiterungen" }),
            CancellationToken::new(),
        )
        .await;
    let value: serde_json::Value = serde_json::from_str(&result.content).expect("JSON result");
    let hint = value["hint"].as_str().expect("hint");
    assert!(hint.contains("not about the user's data"), "{hint}");
    assert!(hint.contains("omit query"), "{hint}");
}

#[test]
fn search_finds_the_app_list_for_questions_about_extensions() {
    let defs = crate::chat::eval::embedded_tools();
    for query in [
        "erweiterungen",
        "Erweiterung",
        "installierte Erweiterungen",
        "Erweiterungen auflisten",
        "extensions",
        "installed extensions",
        "list extensions",
        "installed apps",
    ] {
        let (page, _) = search_actions(&defs, Some(query), None, 5);
        let ids = page
            .iter()
            .map(|action| action.action_id.as_str())
            .collect::<Vec<_>>();
        assert!(
            ids.contains(&"wm.apps.list"),
            "query {query:?} found {ids:?}"
        );
    }
}

/// An extension's tool as the offer sees it (spec 018): source `haextension` and its texts.
struct ExtensionLike {
    name: String,
    texts: Vec<String>,
}

#[async_trait::async_trait]
impl Tool for ExtensionLike {
    fn name(&self) -> &str {
        &self.name
    }
    fn description(&self) -> &str {
        "an extension's tool"
    }
    fn source(&self) -> &'static str {
        super::extension_offer::EXTENSION_SOURCE
    }
    fn input_schema(&self) -> serde_json::Value {
        json!({ "type": "object" })
    }
    fn risk_class(&self) -> super::RiskClass {
        super::RiskClass::Safe
    }
    fn match_entry(&self) -> Option<super::extension_offer::MatchEntry> {
        Some(super::extension_offer::MatchEntry {
            tool_name: self.name.clone(),
            extension_title: "Mail".to_owned(),
            texts: self.texts.clone(),
        })
    }
    async fn execute(&self, _: serde_json::Value, _: CancellationToken) -> super::ToolResult {
        super::ToolResult::ok("")
    }
}

/// Nine core actions, search, `ask_user` and five extension tools that all fit "ungelesene Mails".
fn registry_with_extension_tools() -> (ToolRegistry, Vec<AgentActionDef>) {
    let bridge = ActionBridge::default();
    let mut defs: Vec<AgentActionDef> = (0..9)
        .map(|i| def(&format!("core.action{i}"), "Core", true))
        .collect();
    defs.extend((0..8).map(|i| def(&format!("more.action{i}"), "More", false)));
    let mut registry = ToolRegistry::new();
    for def in &defs {
        registry.register(Arc::new(ActionTool::new(def.clone(), bridge.clone())));
    }
    registry.register(Arc::new(FindActionsTool::new(defs.clone())));
    registry.register(Arc::new(AskUserTool));
    for i in 0..5 {
        registry.register(Arc::new(ExtensionLike {
            name: format!("x_mail_tool{i}"),
            texts: vec![format!("Ungelesene Mails zählen {i}")],
        }));
    }
    (registry, defs)
}

fn offered(offer: &[ToolSpec]) -> Vec<&str> {
    offer.iter().map(|tool| tool.name.as_str()).collect()
}

/// Spec 018 R6: the first step offers the core and at most three extension tools that fit the
/// message, at most 14 tools; a message they do not fit adds none.
#[test]
fn the_first_offer_adds_only_extension_tools_that_fit_the_message() {
    use super::availability::ToolAvailability;
    use super::offer::first_offer;

    let (registry, _) = registry_with_extension_tools();
    let offer = first_offer(
        ToolAvailability::Offered,
        &registry,
        "Wie viele ungelesene Mails habe ich?",
    );
    let names = offered(&offer);
    assert_eq!(names.iter().filter(|n| n.starts_with("x_")).count(), 3);
    assert!(offer.len() <= 14, "{names:?}");

    let offer = first_offer(ToolAvailability::Offered, &registry, "Hallo!");
    assert!(offered(&offer).iter().all(|n| !n.starts_with("x_")));
    assert_eq!(core_offer(&registry).len(), offer.len());
}

/// Spec 018 FR-017: a model that cannot use tools and a delegate turn get no extension tools.
#[test]
fn no_extension_tools_for_a_model_without_tools_or_a_delegate() {
    use super::availability::ToolAvailability;
    use super::offer::first_offer;

    let (registry, _) = registry_with_extension_tools();
    for availability in [ToolAvailability::Unsupported, ToolAvailability::Delegate] {
        let offer = first_offer(
            availability,
            &registry,
            "Wie viele ungelesene Mails habe ich?",
        );
        assert!(offer.is_empty(), "{availability:?}");
    }
}

/// The matched tools stay for every step; after a search at most 19 tools are offered.
#[test]
fn matched_extension_tools_stay_after_a_search() {
    use super::availability::ToolAvailability;
    use super::offer::first_offer;

    let (registry, defs) = registry_with_extension_tools();
    let mut offer = first_offer(
        ToolAvailability::Offered,
        &registry,
        "Wie viele ungelesene Mails habe ich?",
    );
    let found: Vec<ToolSpec> = defs
        .iter()
        .filter(|def| !def.core)
        .map(|def| ToolSpec {
            name: def.tool_name.clone(),
            description: def.description.clone(),
            input_schema: def.input_schema.clone(),
        })
        .collect();
    let scope = ExtensionScope {
        names: (0..5).map(|i| format!("x_mail_tool{i}")).collect(),
        pinned: offer.iter().map(|tool| tool.name.clone()).collect(),
    };
    extend_offer(&mut offer, &found, &defs, &scope);
    let names = offered(&offer);
    assert_eq!(names.iter().filter(|n| n.starts_with("x_")).count(), 3);
    assert!(offer.len() <= 19, "{} tools: {names:?}", offer.len());
}

fn spec(name: &str) -> ToolSpec {
    ToolSpec {
        name: name.to_owned(),
        description: String::new(),
        input_schema: json!({ "type": "object" }),
    }
}

/// Spec 018 R7: a search may add extension tools; the next search replaces those, and the ones
/// the first step offered stay.
#[test]
fn a_search_adds_extension_tools_and_the_next_one_replaces_them() {
    let core = def("wm.apps.list", "List apps", true);
    let defs = vec![core.clone()];
    let scope = ExtensionScope {
        names: ["x_mail_count", "x_mail_search", "x_notes_find"]
            .map(str::to_owned)
            .into(),
        pinned: [core.tool_name.clone(), "x_mail_count".to_owned()].into(),
    };
    let mut offer = vec![spec(&core.tool_name), spec("x_mail_count")];

    extend_offer(
        &mut offer,
        &[spec("x_mail_search"), spec("x_unknown")],
        &defs,
        &scope,
    );
    assert_eq!(
        offered(&offer),
        [core.tool_name.as_str(), "x_mail_count", "x_mail_search"]
    );

    extend_offer(&mut offer, &[spec("x_notes_find")], &defs, &scope);
    assert_eq!(
        offered(&offer),
        [core.tool_name.as_str(), "x_mail_count", "x_notes_find"]
    );
}

/// Spec 018 R7: `find_actions` finds extension tools by their texts, examples included, with the
/// same result form as actions.
#[tokio::test]
async fn the_search_finds_extension_tools_by_their_examples() {
    let defs = vec![def("settings.get", "Read settings", false)];
    let registry = Arc::new(std::sync::Mutex::new(ToolRegistry::new()));
    registry.lock().unwrap().register(Arc::new(ExtensionLike {
        name: "x_mail_count_unread".to_owned(),
        texts: vec!["Wie viele ungelesene Mails habe ich?".to_owned()],
    }));
    let search = FindActionsTool::with_extensions(defs, &registry);

    let result = search
        .execute(
            json!({ "query": "ungelesene Mails" }),
            CancellationToken::new(),
        )
        .await;
    let found = found_tools(&result.content).expect("a search result");
    assert_eq!(offered(&found), ["x_mail_count_unread"]);
    assert_eq!(found[0].description, "an extension's tool");

    let result = search
        .execute(json!({ "query": "settings" }), CancellationToken::new())
        .await;
    assert_eq!(
        offered(&found_tools(&result.content).unwrap()),
        ["settings_get"]
    );
}
