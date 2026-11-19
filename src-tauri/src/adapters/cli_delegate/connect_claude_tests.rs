//! Unit tests for `connect_claude.rs`'s pure OSC-8/token extraction — the
//! byte shapes here are the real ones captured live from `claude
//! setup-token` under a pty (research.md §5), not invented fixtures.

use std::sync::Arc;

use super::connect_claude::{extract_oauth_token, extract_osc8_url};

/// The exact OSC 8 hyperlink shape observed live: `\x1b]8;id=...;<url>\x07`
/// opening the link, the visible (word-wrapped) text, then `\x1b]8;;\x07`
/// closing it — repeated on every terminal redraw with the same URL but
/// differently-wrapped visible text each time.
fn osc8_hyperlink(id: &str, url: &str, visible: &str) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(format!("\x1b]8;id={id};{url}\x07").as_bytes());
    buf.extend_from_slice(visible.as_bytes());
    buf.extend_from_slice(b"\x1b]8;;\x07");
    buf
}

#[test]
fn extracts_the_whole_url_from_one_osc8_hyperlink() {
    let url = "https://claude.com/cai/oauth/authorize?code=true&client_id=abc&state=xyz";
    let buf = osc8_hyperlink("16b8je5", url, "https://claude.com/cai/oauth/authorize?co…");
    assert_eq!(extract_osc8_url(&buf).as_deref(), Some(url));
}

#[test]
fn ignores_the_wrapped_visible_text_and_reads_the_payload_instead() {
    // The visible text is truncated/word-wrapped differently across redraws,
    // but the OSC 8 payload always carries the full URL — this is exactly
    // why extraction reads the payload, not rendered text.
    let url = "https://claude.com/cai/oauth/authorize?code=true&client_id=9d1c250a-e61b-44d9-88ed-5944d1962f5e";
    let mut buf = Vec::new();
    buf.extend_from_slice(&osc8_hyperlink(
        "1",
        url,
        "https://claude.com/cai/oauth/authorize?co",
    ));
    buf.extend_from_slice(b"\r\n");
    buf.extend_from_slice(&osc8_hyperlink(
        "1",
        url,
        "de=true&client_id=9d1c250a-e61b-44d9-88",
    ));
    assert_eq!(extract_osc8_url(&buf).as_deref(), Some(url));
}

#[test]
fn no_osc8_sequence_yields_none() {
    let buf = b"Opening browser to sign in\xe2\x80\xa6\r\n".to_vec();
    assert_eq!(extract_osc8_url(&buf), None);
}

#[test]
fn unterminated_osc8_sequence_yields_none() {
    // No terminating BEL/ST byte at all.
    let buf = b"\x1b]8;id=1;https://example.com/incomplete".to_vec();
    assert_eq!(extract_osc8_url(&buf), None);
}

#[test]
fn extracts_a_bare_oauth_token() {
    let text =
        b"Here is your long-lived token:\r\n\r\nsk-ant-oat01-abcDEF123_-xyz\r\n\r\nCopy it now.";
    assert_eq!(
        extract_oauth_token(text).as_deref(),
        Some("sk-ant-oat01-abcDEF123_-xyz")
    );
}

#[test]
fn strips_ansi_before_matching_the_token() {
    let mut text = Vec::new();
    text.extend_from_slice(b"\x1b[32msk-ant-oat01-abcDEF\x1b[39m123\r\n");
    assert_eq!(
        extract_oauth_token(&text).as_deref(),
        Some("sk-ant-oat01-abcDEF123")
    );
}

#[test]
fn bare_prefix_with_no_suffix_is_not_a_token() {
    let text = b"sk-ant-oat is just a prefix mentioned in prose, not a real token here.";
    assert_eq!(extract_oauth_token(text), None);
}

#[test]
fn no_token_in_output_yields_none() {
    let text = b"Paste code here if prompted >";
    assert_eq!(extract_oauth_token(text), None);
}

// A connect flow confirms the version it ran only when the token is obtained (spec 094 FR-022).

use tokio::sync::{Notify, mpsc};

use super::connect_claude::{ClaudeConnectSession, ReaderEvent, submit_claude_code};
use crate::cli_tools::runtime::DelegateRuntime;
use crate::instances::vault_paths::VaultPaths;

/// A runtime that holds the claim on version `1.0.0` of a stub Claude, and that version's folder.
fn runtime_with_a_version(dir: &tempfile::TempDir) -> (DelegateRuntime, std::path::PathBuf) {
    let version_dir = VaultPaths::new(dir.path())
        .cli_tool_dir("stub", "claude")
        .join("1.0.0");
    std::fs::create_dir_all(&version_dir).unwrap();
    (
        DelegateRuntime::for_stub_version("claude", dir.path(), "1.0.0"),
        version_dir,
    )
}

#[tokio::test]
async fn a_submitted_code_that_yields_the_token_confirms_the_version() {
    let dir = tempfile::tempdir().unwrap();
    let (runtime, version_dir) = runtime_with_a_version(&dir);
    let (tx, rx) = mpsc::unbounded_channel();
    let mut session =
        ClaudeConnectSession::new_for_test_with(rx, Arc::new(Notify::new()), Some(runtime));
    tx.send(ReaderEvent::Token("sk-ant-oat01-test".into()))
        .unwrap();

    let token = submit_claude_code(&mut session, "the-code").await.unwrap();

    assert_eq!(token, b"sk-ant-oat01-test");
    assert!(version_dir.join(".confirmed").is_file());
}

#[tokio::test]
async fn a_flow_that_ends_without_a_token_confirms_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (runtime, version_dir) = runtime_with_a_version(&dir);
    let (tx, rx) = mpsc::unbounded_channel();
    let mut session =
        ClaudeConnectSession::new_for_test_with(rx, Arc::new(Notify::new()), Some(runtime));
    tx.send(ReaderEvent::Eof).unwrap();

    assert!(submit_claude_code(&mut session, "the-code").await.is_err());

    assert!(!version_dir.join(".confirmed").exists());
}
