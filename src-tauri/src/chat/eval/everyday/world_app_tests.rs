use serde_json::{json, Value};

use super::embedded_world;
use crate::chat::eval::embedded_tools;
use crate::chat::tools::action_tool::ActionEffect;

fn content(answer: &super::world::Answer) -> Value {
    serde_json::from_str(&answer.content).unwrap()
}

/// An input that names what the world holds, so that a tool with a required id or path finds it.
fn known_input(tool: &str) -> Value {
    match tool {
        "chat_messages_list" => json!({ "threadId": "conv-urlaub" }),
        "wm_tab_history" => json!({ "tabId": "tab-chat" }),
        "files_stat" | "files_read" | "files_list" | "files_search" => json!({
            "source": "device", "path": "/Dokumente/Mietvertrag.pdf", "query": "mietvertrag",
        }),
        "files_shares_of" => json!({ "path": "/Dokumente/Mietvertrag.pdf" }),
        "files_shares_ofContact" => json!({ "contactId": "c-anna" }),
        "contacts_get" => json!({ "id": "c-anna" }),
        "syncRules_status" => json!({ "id": "rule-1" }),
        _ => json!({ "tab": "t1" }),
    }
}

#[test]
fn every_reading_tool_answers_from_the_world_not_with_a_bare_success() {
    let world = embedded_world();
    for tool in embedded_tools()
        .into_iter()
        .filter(|t| t.effect == ActionEffect::Read)
    {
        let answer = world.answer(&tool.tool_name, &known_input(&tool.tool_name));
        assert_ne!(
            answer.content,
            json!({ "ok": true }).to_string(),
            "{} is answered with a bare success",
            tool.tool_name
        );
    }
}

#[test]
fn the_conversations_are_the_user_s_own_with_holzi() {
    let world = embedded_world();
    let answer = world.answer("chat_conversations_list", &json!({}));
    assert!(!answer.is_error);
    let titles: Vec<String> = content(&answer)["conversations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["title"].as_str().unwrap().to_owned())
        .collect();
    assert!(!titles.is_empty());
    // Conversations with contacts look absent to an agent without spec 072's grant (FR-070).
    for contact in &world.contacts {
        let first = contact.name.split_whitespace().next().unwrap();
        assert!(
            titles.iter().all(|t| !t.contains(first)),
            "{first} in {titles:?}"
        );
    }
}

#[test]
fn a_conversation_s_messages_carry_no_text() {
    let world = embedded_world();
    let answer = world.answer("chat_messages_list", &json!({ "threadId": "conv-urlaub" }));
    assert!(!answer.is_error);
    let messages = content(&answer)["messages"].as_array().unwrap().clone();
    assert!(!messages.is_empty());
    for message in messages {
        let mut keys: Vec<&str> = message
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(keys, ["createdAt", "id", "role", "status"]);
    }
}

#[test]
fn an_unknown_id_or_path_is_an_error() {
    let world = embedded_world();
    let unknown = [
        ("chat_messages_list", json!({ "threadId": "conv-anna" })),
        (
            "files_stat",
            json!({ "source": "device", "path": "/Dokumente/Anna.pdf" }),
        ),
        (
            "files_read",
            json!({ "source": "device", "path": "/Dokumente/Anna.pdf" }),
        ),
        ("syncRules_status", json!({ "id": "rule-1" })),
        ("wm_tab_history", json!({ "tabId": "tab-mail" })),
        ("browser_page_read", json!({ "tab": "t1" })),
    ];
    for (tool, input) in unknown {
        assert!(world.answer(tool, &input).is_error, "{tool} {input}");
    }
}

#[test]
fn a_known_file_s_details_match_the_search() {
    let world = embedded_world();
    let hit = content(&world.answer("files_search", &json!({ "query": "mietvertrag" })))["hits"][0]
        .clone();
    let stat = world.answer(
        "files_stat",
        &json!({ "source": "device", "path": "/Dokumente/Mietvertrag.pdf" }),
    );
    assert!(!stat.is_error);
    assert_eq!(content(&stat), hit);
    let folder = world.answer(
        "files_stat",
        &json!({ "source": "device", "path": "/Dokumente" }),
    );
    assert_eq!(content(&folder)["kind"], "dir");
    let root = world.answer("files_stat", &json!({ "source": "device", "path": "/" }));
    assert_eq!(content(&root)["kind"], "dir");
}

/// `files_sources` lists this device only: a storage holds nothing, even where the device has the
/// path.
#[test]
fn a_storage_the_world_does_not_have_is_an_error() {
    let world = embedded_world();
    for tool in ["files_stat", "files_read"] {
        let answer = world.answer(
            tool,
            &json!({ "source": "storage:missing", "path": "/Dokumente/Mietvertrag.pdf" }),
        );
        assert!(answer.is_error, "{tool}");
        assert!(
            content(&answer).to_string().contains("no storage"),
            "{tool}"
        );
    }
}

#[test]
fn holzi_s_areas_without_data_answer_empty() {
    let world = embedded_world();
    let empty = [
        ("syncRules_list", "rules"),
        ("files_shares_list", "shares"),
        ("files_shares_ofContact", "shares"),
        ("browser_tabs_list", "tabs"),
        ("settings_models_checkUpdates", "updates"),
    ];
    for (tool, key) in empty {
        let answer = world.answer(tool, &known_input(tool));
        assert!(!answer.is_error, "{tool}");
        assert_eq!(content(&answer)[key], json!([]), "{tool}");
    }
    let share = world.answer("files_shares_of", &known_input("files_shares_of"));
    assert_eq!(content(&share), json!({}));
}

#[test]
fn the_window_the_models_and_this_device_are_there() {
    let world = embedded_world();
    let state = content(&world.answer("wm_state_get", &json!({})));
    assert_eq!(state["workspaces"][0]["windows"][0]["app"], "system.chat");
    let models = content(&world.answer("settings_models_list", &json!({})));
    assert!(!models["models"].as_array().unwrap().is_empty());
    let devices = content(&world.answer("settings_devices_list", &json!({})));
    assert_eq!(devices["devices"][0]["name"], world.settings["deviceName"]);
    assert_eq!(devices["devices"][0]["thisDevice"], true);
    let sources = content(&world.answer("files_sources", &json!({})));
    assert_eq!(sources["device"]["name"], world.settings["deviceName"]);
}

#[test]
fn changing_tools_still_answer_a_plain_success() {
    let world = embedded_world();
    for tool in [
        "chat_conversation_open",
        "settings_general_setLanguage",
        "files_show",
    ] {
        let answer = world.answer(tool, &json!({}));
        assert!(!answer.is_error, "{tool}");
        assert_eq!(answer.content, json!({ "ok": true }).to_string(), "{tool}");
    }
}

#[test]
fn an_app_opens_by_its_id_its_title_or_a_name_that_holds_it_and_an_unknown_one_is_an_error() {
    let world = embedded_world();
    for app in [
        "system.passwords",
        "Passwörter",
        "passwords",
        "Einstellungen",
        "haex-mail",
        "extension.6f1c2a8e-3b7d-4e19-9a52-0c4d8b1e7f30",
        "haex-chat",
    ] {
        let answer = world.answer("wm_app_open", &json!({ "appId": app }));
        assert!(!answer.is_error, "{app}");
    }
    for app in ["office", "Word", ""] {
        let answer = world.answer("wm_app_open", &json!({ "appId": app }));
        assert!(answer.is_error, "{app}");
    }
}

#[test]
fn the_file_search_finds_every_word_in_any_order() {
    let world = embedded_world();
    let hits = content(&world.answer("files_search", &json!({ "query": "Anna Geburtstag" })))
        ["hits"]
        .clone();
    assert_eq!(hits[0]["name"], "IMG_1001.jpg");
    let none =
        content(&world.answer("files_search", &json!({ "query": "Anna Urlaub" })))["hits"].clone();
    assert_eq!(none, json!([]));
}
