//! Tests for `watch_claude_auto_completion` — the background watcher that
//! fixes the deadlock where a Claude connect flow's token, once the browser
//! round-trip redeemed it on its own, sat unread forever because nothing
//! called the session's event channel again until a manual code submission
//! that (per research.md §5's now-disproven assumption) was never going to
//! come. Uses `ClaudeConnectSession::new_for_test` (a real session shape
//! around an injected channel) rather than a real PTY/`claude` process —
//! no real network or subprocess I/O.

use std::sync::Arc;

use tokio::sync::{mpsc, Notify};

use crate::adapters::cli_delegate::connect_claude::{ClaudeConnectSession, ReaderEvent};

use super::connect::{
    unverified_vendors, watch_claude_auto_completion, write_delegate_provider,
    ClaudeAutoCompleteOutcome, ConnectCliDelegateArgs, DelegateConnectProgress,
    DelegateConnectState,
};

#[test]
fn finishing_an_old_abacus_connect_does_not_cancel_a_new_one() {
    let state = DelegateConnectState::new();
    let (old_id, old_token) = state.begin_abacus();
    let (new_id, new_token) = state.begin_abacus();

    assert!(
        old_token.is_cancelled(),
        "a new connect cancels the old one"
    );
    state.finish_abacus(old_id);

    assert!(!new_token.is_cancelled());
    assert_eq!(
        state
            .abacus
            .lock()
            .unwrap()
            .as_ref()
            .map(|active| active.id),
        Some(new_id),
        "the newer connect remains cancellable"
    );
    state.cancel_abacus();
    assert!(new_token.is_cancelled());
}

#[test]
fn connect_debug_outputs_redact_email_and_user_code() {
    let args = ConnectCliDelegateArgs {
        vendor: "abacusai".into(),
        name: "Abacus AI".into(),
        email: Some("person@example.invalid".into()),
    };
    let progress = DelegateConnectProgress {
        vendor: "abacusai",
        status: "awaiting_browser",
        url: Some("https://signin.example.invalid/device".into()),
        code: Some("SECRET-CODE".into()),
        message: None,
    };

    let printed = format!("{args:?} {progress:?}");
    assert!(!printed.contains("person@example.invalid"));
    assert!(!printed.contains("SECRET-CODE"));
    assert!(printed.contains("[redacted]"));
}

#[tokio::test]
/// Completes the pending flow when the browser round-trip produces a token.
async fn completes_when_the_token_arrives_without_a_manual_submission() {
    let (tx, rx) = mpsc::unbounded_channel();
    let token_ready = Arc::new(Notify::new());
    let session = ClaudeConnectSession::new_for_test(rx, token_ready.clone());
    let session_id = session.id();
    let state = DelegateConnectState::new();
    *state.pending_claude.lock().await = Some(session);

    // Mirrors the reader thread's own order: push the event, then notify.
    tx.send(ReaderEvent::Token("sk-ant-oat01-test".into()))
        .unwrap();
    token_ready.notify_one();

    match watch_claude_auto_completion(&state, session_id, token_ready).await {
        ClaudeAutoCompleteOutcome::Completed(token) => {
            assert_eq!(token, b"sk-ant-oat01-test");
        }
        _ => panic!("expected the auto-completion to complete with the token"),
    }
    assert!(
        state.pending_claude.lock().await.is_none(),
        "the completed session must be taken out of the pending slot"
    );
}

#[tokio::test]
async fn drains_url_before_token_with_a_single_notification() {
    let (tx, rx) = mpsc::unbounded_channel();
    let token_ready = Arc::new(Notify::new());
    let session = ClaudeConnectSession::new_for_test(rx, token_ready.clone());
    let session_id = session.id();
    let state = DelegateConnectState::new();
    *state.pending_claude.lock().await = Some(session);

    tx.send(ReaderEvent::Url("https://example.test/oauth".into()))
        .unwrap();
    tx.send(ReaderEvent::Token("sk-ant-oat01-test".into()))
        .unwrap();
    token_ready.notify_one();

    let outcome = watch_claude_auto_completion(&state, session_id, token_ready).await;
    assert!(matches!(
        outcome,
        ClaudeAutoCompleteOutcome::Completed(token) if token == b"sk-ant-oat01-test"
    ));
}

#[tokio::test]
/// Leaves state unchanged when manual submission has already consumed the session.
async fn is_a_no_op_when_a_manual_submission_already_completed_the_flow() {
    let (_tx, rx) = mpsc::unbounded_channel::<ReaderEvent>();
    let token_ready = Arc::new(Notify::new());
    let session = ClaudeConnectSession::new_for_test(rx, token_ready.clone());
    let session_id = session.id();
    let state = DelegateConnectState::new();
    // Simulates `submit_cli_delegate_code` having already `guard.take()`n
    // the session and completed successfully, leaving `pending_claude`
    // empty — the manual path won this race.
    drop(session);

    token_ready.notify_one();
    let outcome = watch_claude_auto_completion(&state, session_id, token_ready).await;
    assert!(matches!(outcome, ClaudeAutoCompleteOutcome::Superseded));
}

#[tokio::test]
/// Leaves a newer pending attempt intact when an older watcher wakes up.
async fn is_a_no_op_when_a_later_connect_attempt_replaced_the_session() {
    let (_stale_tx, stale_rx) = mpsc::unbounded_channel::<ReaderEvent>();
    let stale_token_ready = Arc::new(Notify::new());
    let stale_session = ClaudeConnectSession::new_for_test(stale_rx, stale_token_ready.clone());
    let stale_id = stale_session.id();
    drop(stale_session);

    let (_tx, rx) = mpsc::unbounded_channel::<ReaderEvent>();
    let newer_session = ClaudeConnectSession::new_for_test(rx, Arc::new(Notify::new()));
    let state = DelegateConnectState::new();
    *state.pending_claude.lock().await = Some(newer_session);

    stale_token_ready.notify_one();
    let outcome = watch_claude_auto_completion(&state, stale_id, stale_token_ready).await;
    assert!(matches!(outcome, ClaudeAutoCompleteOutcome::Superseded));
    assert!(
        state.pending_claude.lock().await.is_some(),
        "the newer, still-pending session must be left alone"
    );
}

#[tokio::test]
/// Reports failure and clears the session when the flow ends without a token.
async fn reports_failure_when_the_flow_ends_without_a_token() {
    let (tx, rx) = mpsc::unbounded_channel();
    let token_ready = Arc::new(Notify::new());
    let session = ClaudeConnectSession::new_for_test(rx, token_ready.clone());
    let session_id = session.id();
    let state = DelegateConnectState::new();
    *state.pending_claude.lock().await = Some(session);

    tx.send(ReaderEvent::Eof).unwrap();
    token_ready.notify_one();

    let outcome = watch_claude_auto_completion(&state, session_id, token_ready).await;
    assert!(matches!(outcome, ClaudeAutoCompleteOutcome::Failed(_)));
    assert!(state.pending_claude.lock().await.is_none());
}

#[tokio::test]
async fn a_connected_delegate_row_stores_no_address_and_a_reconnect_keeps_it_so() {
    use super::providers_tests::open_vault;
    use crate::storage::providers::{self as providers_store, ProviderKind};
    use crate::storage::query;
    use crate::vault_gate::VaultGate;

    let dir = tempfile::tempdir().expect("tmp dir");
    let db = open_vault(dir.path());
    let gate = VaultGate::new()
        .vault_db(Arc::clone(&db))
        .expect("vault gate");

    let first = write_delegate_provider(&gate, "codex", "Codex".into(), b"one".to_vec())
        .await
        .expect("connected");
    assert_eq!(first.kind, ProviderKind::CliDelegate);
    assert_eq!(first.base_url, None);

    let mut legacy = first.clone();
    legacy.base_url = Some("/usr/local/bin/codex".into());
    db.write(move |tx| {
        providers_store::update_provider(tx, &legacy)?;
        Ok(())
    })
    .expect("seed a legacy row with a program path");

    let again = write_delegate_provider(&gate, "codex", "Codex".into(), b"two".to_vec())
        .await
        .expect("reconnected");
    assert_eq!(again.id, first.id, "a reconnect updates the row in place");
    assert_eq!(again.base_url, None);

    let rows = query::read(&db, |r| {
        Ok(providers_store::list_providers(r).expect("list"))
    })
    .expect("read");
    assert_eq!(rows.len(), 1);
}

// The watcher confirms the version the flow ran when the token arrives on its own, and only then
// (spec 094 FR-022).

fn runtime_with_a_version(
    dir: &tempfile::TempDir,
) -> (
    crate::cli_tools::runtime::DelegateRuntime,
    std::path::PathBuf,
) {
    let version_dir = crate::instances::vault_paths::VaultPaths::new(dir.path())
        .cli_tool_dir("stub", "claude")
        .join("1.0.0");
    std::fs::create_dir_all(&version_dir).unwrap();
    (
        crate::cli_tools::runtime::DelegateRuntime::for_stub_version("claude", dir.path(), "1.0.0"),
        version_dir,
    )
}

#[tokio::test]
async fn a_token_that_arrives_on_its_own_confirms_the_version_the_flow_ran() {
    let dir = tempfile::tempdir().unwrap();
    let (runtime, version_dir) = runtime_with_a_version(&dir);
    let (tx, rx) = mpsc::unbounded_channel();
    let token_ready = Arc::new(Notify::new());
    let session = ClaudeConnectSession::new_for_test_with(rx, token_ready.clone(), Some(runtime));
    let session_id = session.id();
    let state = DelegateConnectState::new();
    *state.pending_claude.lock().await = Some(session);
    tx.send(ReaderEvent::Token("sk-ant-oat01-test".into()))
        .unwrap();
    token_ready.notify_one();

    let outcome = watch_claude_auto_completion(&state, session_id, token_ready).await;

    assert!(matches!(outcome, ClaudeAutoCompleteOutcome::Completed(_)));
    assert!(version_dir.join(".confirmed").is_file());
}

#[tokio::test]
async fn a_flow_that_ends_on_its_own_without_a_token_confirms_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (runtime, version_dir) = runtime_with_a_version(&dir);
    let (tx, rx) = mpsc::unbounded_channel();
    let token_ready = Arc::new(Notify::new());
    let session = ClaudeConnectSession::new_for_test_with(rx, token_ready.clone(), Some(runtime));
    let session_id = session.id();
    let state = DelegateConnectState::new();
    *state.pending_claude.lock().await = Some(session);
    tx.send(ReaderEvent::Eof).unwrap();
    token_ready.notify_one();

    let outcome = watch_claude_auto_completion(&state, session_id, token_ready).await;

    assert!(matches!(outcome, ClaudeAutoCompleteOutcome::Failed(_)));
    assert!(!version_dir.join(".confirmed").exists());
}

// The provider row of a vendor whose credential is in the password manager (spec 096).

const SMALLER: &str = "00000000-0000-4000-8000-000000000001";
const LARGER: &str = "ffffffff-ffff-4fff-8fff-ffffffffffff";

fn delegate_row(id: &str, vendor: &str, name: &str) -> crate::storage::providers::Provider {
    use crate::storage::providers::{Provider, ProviderCapability, ProviderKind};
    Provider {
        id: uuid::Uuid::parse_str(id).expect("uuid"),
        kind: ProviderKind::CliDelegate,
        adapter: Some(vendor.to_owned()),
        name: name.to_owned(),
        base_url: None,
        credentials: Some(Vec::new()),
        created_at: 0,
        capability: ProviderCapability::Chat,
    }
}

fn seed(db: &Arc<haex_crdt::Database>, rows: Vec<crate::storage::providers::Provider>) {
    use crate::storage::providers as providers_store;
    db.write(move |tx| {
        for row in &rows {
            providers_store::insert_provider(tx, row)?;
        }
        Ok(())
    })
    .expect("seed");
}

fn rows_of(db: &Arc<haex_crdt::Database>) -> Vec<crate::storage::providers::Provider> {
    use crate::storage::providers as providers_store;
    crate::storage::query::read(
        db,
        |r| Ok(providers_store::list_providers(r).expect("list")),
    )
    .expect("read")
}

#[tokio::test]
async fn two_abacus_rows_from_a_sync_reduce_to_the_row_with_the_smallest_id() {
    use super::providers_tests::open_vault;
    use crate::vault_gate::VaultGate;

    let dir = tempfile::tempdir().expect("tmp dir");
    let db = open_vault(dir.path());
    let gate = VaultGate::new()
        .vault_db(Arc::clone(&db))
        .expect("vault gate");
    seed(
        &db,
        vec![
            delegate_row(LARGER, "abacusai", "Abacus B"),
            delegate_row(SMALLER, "abacusai", "Abacus A"),
        ],
    );

    let survivor = write_delegate_provider(&gate, "abacusai", "Abacus AI".into(), Vec::new())
        .await
        .expect("connected");

    assert_eq!(survivor.id.to_string(), SMALLER);
    let rows = rows_of(&db);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id.to_string(), SMALLER);
}

#[tokio::test]
async fn a_connect_of_abacus_makes_one_row_and_a_second_connect_reuses_it() {
    use super::providers_tests::open_vault;
    use crate::vault_gate::VaultGate;

    let dir = tempfile::tempdir().expect("tmp dir");
    let db = open_vault(dir.path());
    let gate = VaultGate::new()
        .vault_db(Arc::clone(&db))
        .expect("vault gate");

    let first = write_delegate_provider(&gate, "abacusai", "Abacus AI".into(), Vec::new())
        .await
        .expect("first");
    let again = write_delegate_provider(&gate, "abacusai", "Abacus AI".into(), Vec::new())
        .await
        .expect("again");

    assert_eq!(first.id, again.id);
    assert_eq!(rows_of(&db).len(), 1);
}

#[tokio::test]
async fn the_rows_of_claude_and_codex_are_not_reduced() {
    use super::providers_tests::open_vault;
    use crate::vault_gate::VaultGate;

    let dir = tempfile::tempdir().expect("tmp dir");
    let db = open_vault(dir.path());
    let gate = VaultGate::new()
        .vault_db(Arc::clone(&db))
        .expect("vault gate");
    seed(
        &db,
        vec![
            delegate_row(SMALLER, "codex", "Codex"),
            delegate_row(LARGER, "codex", "Codex"),
        ],
    );

    write_delegate_provider(&gate, "abacusai", "Abacus AI".into(), Vec::new())
        .await
        .expect("abacus");
    write_delegate_provider(&gate, "codex", "Codex".into(), b"auth".to_vec())
        .await
        .expect("codex");

    let rows = rows_of(&db);
    assert_eq!(
        rows.iter()
            .filter(|row| row.adapter.as_deref() == Some("codex"))
            .count(),
        2,
        "the codex rows are left as they were"
    );
}

#[test]
fn abacus_is_listed_as_unverified_until_its_approvals_are_shown_to_reach_the_bridge() {
    assert_eq!(unverified_vendors(false), ["abacusai"]);
    assert!(unverified_vendors(true).is_empty());
}
