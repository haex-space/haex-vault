//! Unit tests for `connect_claude.rs`'s pure OSC-8/token extraction — the
//! byte shapes here are the real ones captured live from `claude
//! setup-token` under a pty (research.md §5), not invented fixtures.

use std::sync::Arc;

use super::connect_claude::{extract_oauth_token, extract_osc8_url, sign_in_rejected};

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

#[test]
fn the_programs_sign_in_error_is_recognised_through_redraws() {
    // As the program prints it: wrapped with carriage returns, in colour, and followed by its prompt.
    let text = b"\x1b[31m\rOAuth error: That code wasn't accepted. It may be incomplete, expired, already \rused, or from an earlier sign-in attempt.\x1b[39m\r\n Press Enter to retry. ";
    assert!(sign_in_rejected(text));
}

#[test]
fn the_normal_sign_in_screen_is_not_an_error() {
    let text =
        b"Browser didn't open? Use the url below to sign in\r\n\r\nPaste code here if prompted >";
    assert!(!sign_in_rejected(text));
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

#[tokio::test]
async fn a_code_the_program_does_not_accept_ends_the_wait_at_once_and_confirms_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (runtime, version_dir) = runtime_with_a_version(&dir);
    let (tx, rx) = mpsc::unbounded_channel();
    let mut session =
        ClaudeConnectSession::new_for_test_with(rx, Arc::new(Notify::new()), Some(runtime));
    tx.send(ReaderEvent::Rejected).unwrap();

    // Without the event the wait would last the whole token timeout, in silence.
    let outcome = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        submit_claude_code(&mut session, "the-code"),
    )
    .await
    .expect("a rejected code does not wait for the timeout");

    assert!(matches!(
        outcome,
        Err(crate::adapters::AdapterError::InvalidCredentials)
    ));
    assert!(!version_dir.join(".confirmed").exists());
}

/// A writer that remembers each write with the time it happened.
type RecordedWrites = Arc<std::sync::Mutex<Vec<(std::time::Instant, Vec<u8>)>>>;

struct Recorder(RecordedWrites);

impl std::io::Write for Recorder {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .unwrap()
            .push((std::time::Instant::now(), buf.to_vec()));
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn the_code_and_the_enter_reach_the_program_as_two_writes_with_a_gap() {
    // A real code (about 90 characters) that arrives in one piece with its line end is taken for
    // pasted text, and the program never starts the sign-in (measured with claude 2.1.311).
    let (tx, rx) = mpsc::unbounded_channel();
    let mut session = ClaudeConnectSession::new_for_test(rx, Arc::new(Notify::new()));
    let written = Arc::new(std::sync::Mutex::new(Vec::new()));
    session.set_writer_for_test(Box::new(Recorder(written.clone())));
    tx.send(ReaderEvent::Token("sk-ant-oat01-test".into()))
        .unwrap();
    let code = format!("{}#{}", "a".repeat(48), "b".repeat(43));

    submit_claude_code(&mut session, &format!("  {code}  "))
        .await
        .unwrap();

    let writes = written.lock().unwrap();
    assert_eq!(writes.len(), 2, "the code, then the Enter");
    assert_eq!(
        writes[0].1,
        code.as_bytes(),
        "the code, trimmed, without a line end"
    );
    assert_eq!(writes[1].1, b"\r");
    assert!(
        writes[1].0 - writes[0].0 >= super::connect_claude::ENTER_DELAY,
        "the Enter follows after a pause"
    );
}
