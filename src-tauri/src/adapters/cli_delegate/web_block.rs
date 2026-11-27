//! The web block of the subscription delegates (spec 102): each vendor's own web tools and browser
//! are switched off by the program itself on every start, in every autonomy mode, so a delegate
//! reaches the web only through haex-vault's agent browser (research R1–R4,
//! `contracts/web-block.md`). Where the program reports its state at start, the confirmations
//! below judge that report; a start without the block ends before anything runs (FR-007).
//!
//! Computer use is not part of the block (operator decision 3). Abacus' web search and page fetch
//! run on its server, out of reach of its rules (research R4): they stay a named residual, and
//! [`abacus_own_web_use`] recognises them in its stream so the chat can show it (FR-013).

use serde_json::Value;

use super::DelegateVendor;

/// Claude Code's built-in tools a delegate may use: the 2.1.311 default set and the program's own
/// list of built-in names, without `WebFetch` and `WebSearch` (research R1). A tool a later version
/// adds is not here and stays off; a name the program does not know is dropped by it.
pub(super) const CLAUDE_TOOLS: &str =
    "Task,Bash,PowerShell,Read,Write,Edit,Glob,Grep,NotebookEdit,\
TodoWrite,TaskCreate,TaskUpdate,TaskGet,TaskList,TaskStop,Skill,REPL,JavaScript,AskUserQuestion,\
ToolSearch,SendUserMessage,CronCreate,CronDelete,CronList,DesignSync,EnterWorktree,ExitWorktree,\
ListAgents,Monitor,ReportFindings,ScheduleWakeup,SendMessage,Workflow";

/// Claude Code's own web tools, which the init line must not list.
const CLAUDE_WEB_TOOLS: [&str; 2] = ["WebFetch", "WebSearch"];

/// The MCP server Claude Code adds itself to drive the person's Chrome ("Claude in Chrome").
const CLAUDE_CHROME_SERVER: &str = "claude-in-chrome";

/// Codex' features that load web pages or drive a browser, all stable and on by default in 0.169.0
/// (research R2). `computer_use` is deliberately absent.
pub(super) const CODEX_DISABLED_FEATURES: [&str; 5] = [
    "browser_use",
    "browser_use_external",
    "browser_use_full_cdp_access",
    "browser_annotation_api",
    "in_app_browser",
];

/// Abacus' deny rules for its bundled browser, in the rule syntax its own description uses
/// (`Bash(<command> *)`), measured in both modes (research R4).
pub(super) const ABACUS_DENY_RULES: [&str; 3] = [
    "Bash(agent-browser *)",
    "Bash(agent-browser)",
    "Bash(npx *agent-browser*)",
];

/// The arguments every Claude Code start gets, after the catalog's `config_args`.
pub(super) fn claude_args() -> [&'static str; 3] {
    ["--tools", CLAUDE_TOOLS, "--no-chrome"]
}

/// The arguments every Codex start gets, after the catalog's `config_args` and before the
/// subcommand. An unknown feature name ends the program at start (research R2).
pub(super) fn codex_args() -> Vec<String> {
    let mut args = vec!["-c".to_owned(), "web_search=\"disabled\"".to_owned()];
    for feature in CODEX_DISABLED_FEATURES {
        args.push("--disable".to_owned());
        args.push(feature.to_owned());
    }
    args
}

/// Why a start ends: the vendor started without its web block (contracts/web-block.md).
pub(super) fn refusal(vendor: DelegateVendor, what: &str) -> String {
    format!(
        "{} started without its web block ({what}); nothing was run.",
        vendor.as_str()
    )
}

/// Judges Claude Code's `system`/`init` line: its tool list names no own web tool and no tool of
/// "Claude in Chrome", and that server is not among its MCP servers.
pub(super) fn confirm_claude_init(init: &Value) -> Result<(), String> {
    let what = |what: &str| refusal(DelegateVendor::Claude, what);
    let Some(tools) = init.get("tools").and_then(Value::as_array) else {
        return Err(what("no tool list in its init line"));
    };
    let chrome_prefix = format!("mcp__{CLAUDE_CHROME_SERVER}__");
    for tool in tools.iter().filter_map(Value::as_str) {
        if CLAUDE_WEB_TOOLS.contains(&tool) {
            return Err(what(&format!("{tool} is available")));
        }
        if tool.starts_with(&chrome_prefix) {
            return Err(what(&format!("{tool} is available")));
        }
    }
    let chrome = init
        .get("mcp_servers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .any(|server| server.get("name").and_then(Value::as_str) == Some(CLAUDE_CHROME_SERVER));
    if chrome {
        return Err(what(&format!("{CLAUDE_CHROME_SERVER} is connected")));
    }
    Ok(())
}

/// Judges Codex' answers to `config/read` and `experimentalFeature/list` (all pages joined):
/// `web_search` is `disabled` and every feature of [`CODEX_DISABLED_FEATURES`] is listed as off.
pub(super) fn confirm_codex(config_read: &Value, features: &[Value]) -> Result<(), String> {
    let what = |what: &str| refusal(DelegateVendor::Codex, what);
    let web_search = config_read
        .get("config")
        .and_then(|config| config.get("web_search"))
        .and_then(Value::as_str);
    if web_search != Some("disabled") {
        return Err(what(&format!(
            "web_search is {}",
            web_search.unwrap_or("not reported")
        )));
    }
    for name in CODEX_DISABLED_FEATURES {
        let feature = features
            .iter()
            .find(|feature| feature.get("name").and_then(Value::as_str) == Some(name));
        match feature
            .and_then(|feature| feature.get("enabled"))
            .and_then(Value::as_bool)
        {
            Some(false) => {}
            Some(true) => return Err(what(&format!("{name} is enabled"))),
            None => return Err(what(&format!("{name} is not reported"))),
        }
    }
    Ok(())
}

/// Whether a line of Abacus' stream reports one of its own server-side web tools: a web search
/// (`web_search_results`) or a page fetch (`collapsible` with the program's fixed title
/// "Web Access"), research R4.
pub(super) fn abacus_own_web_use(event: &Value) -> bool {
    match event.get("type").and_then(Value::as_str) {
        Some("web_search_results") => true,
        Some("collapsible") => event.get("title").and_then(Value::as_str) == Some("Web Access"),
        _ => false,
    }
}

#[cfg(test)]
#[path = "web_block_tests.rs"]
mod tests;
