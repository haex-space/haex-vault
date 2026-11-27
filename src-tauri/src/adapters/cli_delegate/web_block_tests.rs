use serde_json::{json, Value};

use super::{
    abacus_own_web_use, claude_args, codex_args, confirm_claude_init, confirm_codex,
    ABACUS_DENY_RULES, CLAUDE_TOOLS, CODEX_DISABLED_FEATURES,
};

/// The tools of the init line of claude 2.1.311 with `--tools` set (research R1).
fn claude_init(tools: &[&str], servers: Value) -> Value {
    json!({ "type": "system", "subtype": "init", "tools": tools, "mcp_servers": servers })
}

/// `experimentalFeature/list` data as codex 0.169.0 answers it, with the five features off.
fn codex_features(enabled: &[&str]) -> Vec<Value> {
    let mut features: Vec<Value> = CODEX_DISABLED_FEATURES
        .iter()
        .map(|name| json!({ "name": name, "stage": "stable", "enabled": enabled.contains(name) }))
        .collect();
    features.push(json!({ "name": "computer_use", "stage": "stable", "enabled": true }));
    features
}

fn codex_config(web_search: Value) -> Value {
    json!({ "config": { "web_search": web_search, "features": {} } })
}

#[test]
fn claude_gets_an_allow_list_without_its_web_tools_and_no_chrome() {
    assert_eq!(claude_args(), ["--tools", CLAUDE_TOOLS, "--no-chrome"]);
    let tools: Vec<&str> = CLAUDE_TOOLS.split(',').collect();
    for expected in [
        "Task",
        "Bash",
        "PowerShell",
        "Read",
        "Write",
        "Edit",
        "Glob",
        "Grep",
        "NotebookEdit",
        "TodoWrite",
        "TaskCreate",
        "TaskUpdate",
        "TaskGet",
        "TaskList",
        "TaskStop",
        "Skill",
        "REPL",
        "JavaScript",
        "AskUserQuestion",
        "ToolSearch",
        "SendUserMessage",
        "CronCreate",
        "CronDelete",
        "CronList",
        "DesignSync",
        "EnterWorktree",
        "ExitWorktree",
        "ListAgents",
        "Monitor",
        "ReportFindings",
        "ScheduleWakeup",
        "SendMessage",
        "Workflow",
    ] {
        assert!(
            tools.contains(&expected),
            "{expected} missing from the allow-list"
        );
    }
    assert!(!tools.contains(&"WebFetch"));
    assert!(!tools.contains(&"WebSearch"));
    assert!(tools
        .iter()
        .all(|tool| !tool.is_empty() && !tool.contains(char::is_whitespace)));
}

#[test]
fn codex_gets_web_search_disabled_and_five_browser_features_off_but_keeps_computer_use() {
    let args = codex_args();
    assert_eq!(args[..2], ["-c", "web_search=\"disabled\""]);
    let disabled: Vec<&str> = args[2..]
        .chunks(2)
        .map(|pair| {
            assert_eq!(pair[0], "--disable");
            pair[1].as_str()
        })
        .collect();
    assert_eq!(disabled, CODEX_DISABLED_FEATURES);
    assert!(!args.iter().any(|arg| arg == "computer_use"));
}

#[test]
fn abacus_denies_its_bundled_browser_in_its_own_rule_syntax() {
    assert_eq!(
        ABACUS_DENY_RULES,
        [
            "Bash(agent-browser *)",
            "Bash(agent-browser)",
            "Bash(npx agent-browser *)"
        ]
    );
}

#[test]
fn a_claude_init_without_web_tools_and_chrome_is_accepted() {
    let init = claude_init(
        &["Task", "Bash", "Read", "mcp__haex__browser_open"],
        json!([{ "name": "haex", "status": "connected" }]),
    );
    assert_eq!(confirm_claude_init(&init), Ok(()));
}

#[test]
fn a_claude_init_with_a_web_tool_is_refused_with_its_name() {
    for tool in ["WebFetch", "WebSearch"] {
        let init = claude_init(&["Task", tool], json!([]));
        let reason = confirm_claude_init(&init).expect_err("refused");
        assert!(reason.contains(tool), "{reason}");
        assert!(
            reason.contains("claude started without its web block"),
            "{reason}"
        );
    }
}

#[test]
fn a_claude_init_with_claude_in_chrome_is_refused() {
    let connected = claude_init(
        &["Task"],
        json!([{ "name": "claude-in-chrome", "status": "connected" }]),
    );
    assert!(confirm_claude_init(&connected)
        .expect_err("refused")
        .contains("claude-in-chrome is connected"));
    let tool = claude_init(&["Task", "mcp__claude-in-chrome__navigate"], json!([]));
    assert!(confirm_claude_init(&tool)
        .expect_err("refused")
        .contains("mcp__claude-in-chrome__navigate"));
}

#[test]
fn a_claude_init_without_a_tool_list_is_refused() {
    let init = json!({ "type": "system", "subtype": "init" });
    assert!(confirm_claude_init(&init)
        .expect_err("refused")
        .contains("no tool list"));
}

#[test]
fn codex_with_the_block_is_accepted() {
    assert_eq!(
        confirm_codex(&codex_config(json!("disabled")), &codex_features(&[])),
        Ok(())
    );
}

#[test]
fn codex_with_web_search_on_or_unreported_is_refused() {
    for value in [json!("live"), json!("cached"), Value::Null] {
        let reason =
            confirm_codex(&codex_config(value.clone()), &codex_features(&[])).expect_err("refused");
        assert!(reason.contains("web_search"), "{value}: {reason}");
    }
    let reason =
        confirm_codex(&json!({ "error": "nope" }), &codex_features(&[])).expect_err("refused");
    assert!(reason.contains("not reported"), "{reason}");
}

#[test]
fn codex_with_a_browser_feature_on_or_missing_is_refused() {
    let reason = confirm_codex(
        &codex_config(json!("disabled")),
        &codex_features(&["browser_use"]),
    )
    .expect_err("refused");
    assert!(reason.contains("browser_use is enabled"), "{reason}");

    let mut features = codex_features(&[]);
    features.retain(|feature| feature["name"] != "in_app_browser");
    let reason = confirm_codex(&codex_config(json!("disabled")), &features).expect_err("refused");
    assert!(
        reason.contains("in_app_browser is not reported"),
        "{reason}"
    );
}

#[test]
fn abacus_server_side_web_use_is_recognised_from_its_events() {
    // Recorded from the 2.10.23 runs of research R4.
    assert!(abacus_own_web_use(&json!({
        "type": "web_search_results", "query": "Rust programming language", "resultType": "web",
        "results": []
    })));
    assert!(abacus_own_web_use(&json!({
        "type": "collapsible", "content": "Looked at https://example.org", "isSpinny": true,
        "title": "Web Access"
    })));
    assert!(!abacus_own_web_use(&json!({
        "type": "collapsible", "content": "Thinking", "title": "Reasoning"
    })));
    assert!(!abacus_own_web_use(
        &json!({ "type": "text_delta", "content": "Web Access" })
    ));
}
