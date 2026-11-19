use std::path::Path;

use tempfile::TempDir;

use super::autonomy::AutonomyMode;
use super::claude::{build_command, McpArgs};
use super::haex_mcp::{
    claude_config, codex_config, with_instruction, write_claude_config, write_codex_config,
    ApproveBridge, HaexServer, CLAUDE_HAEX_TOOLS, DELEGATE_INSTRUCTION,
};
use crate::cli_tools::runtime::DelegateRuntime;

fn haex(slug: &str) -> HaexServer {
    HaexServer::new(
        "/opt/holzi/holzi".to_owned(),
        vec![
            "--mcp-agent".to_owned(),
            slug.to_owned(),
            "--vault".to_owned(),
            "0b7d3c62-9f2e-4c1a-8d55-2a4b6e1f0c99".to_owned(),
            "--delegate-token".to_owned(),
            "pass".to_owned(),
        ],
        (),
    )
}

fn args(cmd: &tokio::process::Command) -> Vec<String> {
    cmd.as_std()
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect()
}

fn claude_args(mcp_config: Option<&Path>, haex: bool, mode: AutonomyMode) -> Vec<String> {
    let tmp = TempDir::new().expect("tmp");
    let runtime = DelegateRuntime::for_stub("claude", tmp.path().join("claude"), tmp.path());
    let run = runtime.run_folder().expect("run folder");
    let mcp = mcp_config.map(|config| McpArgs { config, haex });
    args(&build_command(&runtime, "", mcp, None, &run, mode, None))
}

#[test]
fn a_registered_claude_delegate_gets_both_servers_outside_ungated() {
    let server = haex("claude-code-delegate");
    let approve = ApproveBridge {
        executable: Path::new("/opt/holzi/holzi"),
        socket: Path::new("/tmp/x/approval.sock"),
    };
    let config = claude_config(Some(approve), Some(&server)).expect("a config");
    let servers = &config["mcpServers"];
    assert_eq!(
        servers["haex-approve"]["args"][0],
        "--internal-cli-delegate-approval-bridge"
    );
    assert_eq!(servers["haex"]["command"], "/opt/holzi/holzi");
    assert_eq!(servers["haex"]["args"][1], "claude-code-delegate");
    assert_eq!(servers["haex"]["args"][4], "--delegate-token");
}

/// FR-024: "Ungated" has no approval bridge, but holzi's server is there all the same.
#[test]
fn ungated_keeps_haex_and_drops_only_the_approval_bridge() {
    let server = haex("claude-code-delegate");
    let config = claude_config(None, Some(&server)).expect("a config");
    assert!(config["mcpServers"].get("haex-approve").is_none());
    assert!(config["mcpServers"].get("haex").is_some());

    let path = Path::new("/tmp/x/mcp-config.json");
    let args = claude_args(Some(path), true, AutonomyMode::Ungated);
    let at = |flag: &str| args.iter().position(|arg| arg == flag);
    assert_eq!(
        at("--permission-mode").map(|i| args[i + 1].as_str()),
        Some("bypassPermissions")
    );
    assert_eq!(
        at("--mcp-config").map(|i| args[i + 1].as_str()),
        Some("/tmp/x/mcp-config.json")
    );
    assert_eq!(
        at("--allowedTools").map(|i| args[i + 1].as_str()),
        Some(CLAUDE_HAEX_TOOLS)
    );
    assert!(at("--permission-prompt-tool").is_none());
}

/// holzi asks for its own tools; Claude's approval bridge must not ask for them again.
#[test]
fn outside_ungated_haex_tools_skip_claudes_own_approval() {
    let path = Path::new("/tmp/x/mcp-config.json");
    let args = claude_args(Some(path), true, AutonomyMode::Standard);
    assert!(args.contains(&"--permission-prompt-tool".to_owned()));
    assert!(args.contains(&CLAUDE_HAEX_TOOLS.to_owned()));
}

#[test]
fn an_unregistered_delegate_gets_no_haex_server() {
    let approve = ApproveBridge {
        executable: Path::new("/opt/holzi/holzi"),
        socket: Path::new("/tmp/x/approval.sock"),
    };
    let config = claude_config(Some(approve), None).expect("the bridge");
    assert!(config["mcpServers"].get("haex").is_none());
    assert_eq!(claude_config(None, None), None);
    let args = claude_args(None, false, AutonomyMode::Ungated);
    assert!(!args.contains(&"--mcp-config".to_owned()));
    assert!(!args.contains(&"--allowedTools".to_owned()));
}

#[tokio::test]
async fn the_written_files_hold_haex_only_when_registered() {
    let dir = TempDir::new().expect("tmp");
    assert_eq!(
        write_claude_config(dir.path(), None, None)
            .await
            .expect("nothing to write"),
        None
    );
    write_codex_config(dir.path(), None).await.expect("no file");
    assert!(!dir.path().join("config.toml").exists());

    let server = haex("codex-delegate");
    write_codex_config(dir.path(), Some(&server))
        .await
        .expect("written");
    let written = std::fs::read_to_string(dir.path().join("config.toml")).expect("read");
    assert_eq!(written, codex_config(&server));
}

#[test]
fn codex_gets_haex_in_its_config_toml() {
    let config = codex_config(&haex("codex-delegate"));
    assert!(config.starts_with("[mcp_servers.haex]\n"));
    assert!(config.contains("command = \"/opt/holzi/holzi\"\n"));
    assert!(config.contains(
        "args = [\"--mcp-agent\", \"codex-delegate\", \"--vault\", \
         \"0b7d3c62-9f2e-4c1a-8d55-2a4b6e1f0c99\", \"--delegate-token\", \"pass\"]\n"
    ));
}

/// Codex asks before every call of an MCP tool unless the server is set to `approve`; without it
/// holzi's tools (the agent browser among them) were refused before holzi saw the call.
#[test]
fn codex_calls_the_tools_of_haex_without_its_own_question() {
    let config = codex_config(&haex("codex-delegate"));
    assert!(config.contains("default_tools_approval_mode = \"approve\"\n"));
    assert_eq!(
        config.matches("default_tools_approval_mode").count(),
        1,
        "set for holzi's server only"
    );
}

/// A Windows path or an AppImage path with odd characters stays one TOML string.
#[test]
fn codex_paths_are_escaped_as_toml_strings() {
    let server = HaexServer::new(
        r#"C:\Program Files\holzi "beta"\haex.exe"#.to_owned(),
        vec!["line\nbreak\u{7f}".to_owned()],
        (),
    );
    let config = codex_config(&server);
    assert!(config.contains(r#"command = "C:\\Program Files\\holzi \"beta\"\\haex.exe""#));
    assert!(config.contains(r#"args = ["line\u000Abreak\u007F"]"#));
}

#[test]
fn debug_output_never_shows_the_pass() {
    let shown = format!("{:?}", haex("claude-code-delegate"));
    assert!(!shown.contains("pass\""), "{shown}");
    assert!(!shown.contains("--delegate-token"), "{shown}");
}

#[test]
fn a_delegate_with_haex_reads_the_instruction_after_the_given_prompt() {
    let with = with_instruction(Some("Answer briefly."), true).expect("prompt");
    assert!(with.starts_with("Answer briefly.\n\n"));
    assert!(with.contains("browser_search"));
    assert!(with.contains("extension"));
    assert_eq!(
        with_instruction(None, true).as_deref(),
        Some(DELEGATE_INSTRUCTION)
    );
    assert_eq!(with_instruction(Some("x"), false).as_deref(), Some("x"));
    assert_eq!(with_instruction(None, false), None);
}
