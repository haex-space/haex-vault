//! Spec 021 T095: the setup snippets of both ways (FR-005); neither carries a credential.

use uuid::Uuid;

use super::commands::{http_snippet, snippet, SnippetClient};

#[test]
fn the_http_snippets_name_haexs_loopback_url() {
    let url = "http://127.0.0.1:47011/mcp";
    assert_eq!(
        http_snippet(SnippetClient::ClaudeCode, 47011),
        format!("claude mcp add --transport http haex {url}")
    );
    assert_eq!(
        http_snippet(SnippetClient::Codex, 47011),
        format!("codex mcp add haex --url {url}")
    );
    let other: serde_json::Value =
        serde_json::from_str(&http_snippet(SnippetClient::Other, 47011)).expect("json");
    assert_eq!(other["mcpServers"]["haex"]["type"], "http");
    assert_eq!(other["mcpServers"]["haex"]["url"], url);
}

#[test]
fn the_program_snippets_start_the_helper_for_this_vault() {
    let device = Uuid::from_u128(7);
    let claude = snippet(SnippetClient::ClaudeCode, "/opt/holzi", device);
    assert!(claude.starts_with("claude mcp add haex -- \"/opt/holzi\" --mcp-agent claude-code"));
    assert!(claude.ends_with(&format!("--vault {device}")));
}
