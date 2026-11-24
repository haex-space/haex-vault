//! Tests for what of an Abo-Agent's own tools counts as foreign content (spec 097 T065, T073).

use serde_json::json;

use super::own_tools::{claude_line_is_foreign, codex_item_is_foreign};

fn claude_assistant(names: &[&str], parent: Option<&str>) -> String {
    let content: Vec<_> = names
        .iter()
        .map(|name| json!({ "type": "tool_use", "id": format!("toolu_{name}"), "name": name, "input": {} }))
        .collect();
    json!({
        "type": "assistant",
        "parent_tool_use_id": parent,
        "message": { "content": content },
    })
    .to_string()
}

#[test]
fn claude_web_tools_bash_and_unknown_names_are_foreign() {
    for name in [
        "WebFetch",
        "WebSearch",
        "Bash",
        "SomethingNew",
        "mcp__other__read",
    ] {
        assert!(
            claude_line_is_foreign(&claude_assistant(&[name], None)),
            "{name}"
        );
    }
    assert!(claude_line_is_foreign(&claude_assistant(
        &["Read", "WebFetch"],
        None
    )));
}

#[test]
fn claude_tools_on_the_run_folder_and_haex_tools_are_own() {
    for name in [
        "Read",
        "Glob",
        "Grep",
        "Edit",
        "Write",
        "TodoWrite",
        "Task",
        "Agent",
        "mcp__haex__memory_add",
        "mcp__haex__browser_search",
    ] {
        assert!(
            !claude_line_is_foreign(&claude_assistant(&[name], None)),
            "{name}"
        );
    }
}

#[test]
fn a_tool_of_a_sub_agent_is_read_too() {
    assert!(claude_line_is_foreign(&claude_assistant(
        &["WebFetch"],
        Some("toolu_Task")
    )));
}

#[test]
fn claude_lines_without_a_call_are_not_foreign() {
    let result = json!({
        "type": "user",
        "message": { "content": [{ "type": "tool_result", "tool_use_id": "toolu_WebFetch", "content": "tool_use" }] },
    })
    .to_string();
    for line in [
        result.as_str(),
        r#"{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"tool_use"}}}"#,
        "not json \"tool_use\"",
        "",
    ] {
        assert!(!claude_line_is_foreign(line), "{line}");
    }
}

fn started(item: serde_json::Value) -> serde_json::Value {
    json!({ "item": item, "threadId": "t", "turnId": "u" })
}

#[test]
fn codex_web_search_commands_other_servers_and_unknown_items_are_foreign() {
    // The shapes recorded by the spike (codex 0.169.0).
    let search = started(
        json!({ "type": "webSearch", "id": "exec-1", "query": "", "action": null, "results": null }),
    );
    let command = started(json!({
        "type": "commandExecution", "id": "exec-2", "command": "/bin/bash -lc 'uname -s'",
        "status": "inProgress", "aggregatedOutput": null, "exitCode": null,
    }));
    let apps = started(
        json!({ "type": "mcpToolCall", "id": "c", "server": "codex_apps", "status": "inProgress" }),
    );
    for params in [
        search,
        command,
        apps,
        started(json!({ "type": "dynamicToolCall", "id": "d" })),
        started(json!({ "type": "functionCallOutput", "id": "f" })),
        started(json!({ "type": "somethingNew", "id": "n" })),
        started(json!({ "id": "no type" })),
    ] {
        assert!(codex_item_is_foreign("item/started", &params), "{params}");
    }
}

#[test]
fn codex_own_items_and_haex_tools_are_not_foreign() {
    for kind in [
        "userMessage",
        "agentMessage",
        "reasoning",
        "plan",
        "fileChange",
        "imageView",
        "contextCompaction",
        "subAgentActivity",
    ] {
        let params = started(json!({ "type": kind, "id": "x" }));
        assert!(!codex_item_is_foreign("item/started", &params), "{kind}");
    }
    let haex = started(json!({ "type": "mcpToolCall", "id": "h", "server": "haex" }));
    assert!(!codex_item_is_foreign("item/started", &haex));
}

#[test]
fn only_the_start_and_end_of_an_item_are_judged() {
    let search = started(json!({ "type": "webSearch", "id": "exec-1" }));
    assert!(codex_item_is_foreign("item/completed", &search));
    for method in [
        "item/agentMessage/delta",
        "turn/completed",
        "thread/started",
    ] {
        assert!(!codex_item_is_foreign(method, &search), "{method}");
    }
}
