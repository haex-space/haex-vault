use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use haex_crdt::{Database, DatabaseConfig, NoopSignatureProvider, SqlCipherKey};
use serde_json::Value;
use tokio::sync::{oneshot, Mutex as AsyncMutex};
use uuid::Uuid;

use super::approval_bridge::request_approval;
use super::autonomy::{ApprovalRequestPayload, AutonomyMode};
use super::{EventEmitter, PendingToolApprovals};
use crate::chat::tools::ApprovalDecision;
use crate::identity::{holzi_migration_source, installation_id_path, HolziBootstrap};
use crate::storage::chat_messages::{self as msg_store, MessageRole};
use crate::storage::preferences::{self, PrefScope};
use crate::storage::query;
use crate::vault_gate::VaultGate;

const PASSPHRASE: &str = "approval-bridge-posture-test";
static OPEN_VAULT: OnceLock<Mutex<()>> = OnceLock::new();

fn open_vault(dir: &Path) -> Database {
    // SQLCipher's PRAGMA setup is process-global in the CI build. Keep the
    // independent fixture opens in this module from racing during startup.
    let _open_guard = OPEN_VAULT
        .get_or_init(|| Mutex::new(()))
        .lock()
        .expect("vault-open lock");
    let installation_id = installation_id_path(dir);
    Database::open(DatabaseConfig {
        path: dir.join("vault.db"),
        key: SqlCipherKey::new(PASSPHRASE),
        create_if_missing: true,
        bootstrap: Arc::new(HolziBootstrap::new(installation_id).with_alias("test")),
        signature_provider: Arc::new(NoopSignatureProvider),
        migration_source: holzi_migration_source(),
        trigger_version: haex_crdt::DEFAULT_TRIGGER_VERSION,
        max_transaction_bytes: haex_crdt::MAX_CRDT_TRANSACTION_BYTES,
        max_value_bytes: haex_crdt::MAX_VALUE_BYTES,
    })
    .expect("vault open")
}

fn never_emitting() -> (EventEmitter, Arc<AtomicBool>) {
    let called = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&called);
    let emit: EventEmitter = Arc::new(move |_event: &str, _payload: Value| {
        flag.store(true, Ordering::SeqCst);
    });
    (emit, called)
}

#[tokio::test]
async fn manual_approval_uses_the_existing_pending_request_flow() {
    let pending: PendingToolApprovals = Arc::new(Mutex::new(HashMap::new()));
    let (event_sender, event_receiver) = oneshot::channel();
    let event_sender = Arc::new(AsyncMutex::new(Some(event_sender)));
    let emit: EventEmitter = {
        let event_sender = Arc::clone(&event_sender);
        Arc::new(move |_event: &str, payload: Value| {
            let event_sender = Arc::clone(&event_sender);
            tokio::spawn(async move {
                if let Some(sender) = event_sender.lock().await.take() {
                    let _ = sender.send(payload);
                }
            });
        })
    };

    let pending_for_request = Arc::clone(&pending);
    let emit_for_request = Arc::clone(&emit);
    let task = tokio::spawn(async move {
        request_approval(
            &pending_for_request,
            &emit_for_request,
            None,
            &[],
            Some(Uuid::nil()),
            AutonomyMode::Standard,
            Path::new("/tmp"),
            "Bash".to_string(),
            serde_json::json!({"command": "echo test"}),
            ApprovalRequestPayload::ClaudeToolCall {
                tool_name: "Bash".to_string(),
                input: serde_json::json!({"command": "echo test"}),
            },
        )
        .await
    });

    let payload = event_receiver
        .await
        .expect("approval event should be emitted");
    let request_id = payload["requestId"]
        .as_str()
        .and_then(|value| Uuid::parse_str(value).ok())
        .expect("approval event should contain a UUID");
    assert_eq!(payload["toolSource"], "cli");
    let sender = pending
        .lock()
        .expect("pending map lock")
        .remove(&request_id)
        .expect("approval sender should be registered");
    sender
        .send(ApprovalDecision::Allow)
        .expect("receiver is alive");
    assert_eq!(
        task.await.expect("approval task should finish"),
        ApprovalDecision::Allow
    );
}

/// tasks.md T031: `Decision::Allow` (Auto posture, a Safe tool) must resolve
/// without ever registering a pending approval or emitting
/// `tool-permission-request` — the live round trip this module exists for is
/// reserved for `Decision::Ask` only.
#[tokio::test]
async fn auto_mode_allow_on_a_safe_tool_skips_the_live_round_trip() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = VaultGate::new()
        .vault_db(Arc::new(open_vault(dir.path())))
        .expect("open gate");
    db.write(|tx| {
        preferences::insert_or_update(tx, PrefScope::Vault, "chat.permission_mode", "auto")?;
        Ok(())
    })
    .await
    .expect("set permission mode");

    let pending: PendingToolApprovals = Arc::new(Mutex::new(HashMap::new()));
    let (emit, emitted) = never_emitting();

    let decision = request_approval(
        &pending,
        &emit,
        Some(&db),
        &[],
        Some(Uuid::nil()),
        AutonomyMode::Standard,
        Path::new("/tmp"),
        "Read".to_string(), // in `is_risky_tool`'s safe list
        serde_json::json!({"path": "/tmp/x"}),
        ApprovalRequestPayload::ClaudeToolCall {
            tool_name: "Read".to_string(),
            input: serde_json::json!({"path": "/tmp/x"}),
        },
    )
    .await;

    assert_eq!(decision, ApprovalDecision::Allow);
    assert!(
        pending.lock().expect("pending lock").is_empty(),
        "Allow must never register a pending approval"
    );
    assert!(
        !emitted.load(Ordering::SeqCst),
        "Allow must never emit tool-permission-request"
    );
}

/// tasks.md T031/T032: `Decision::Deny` (Plan posture, a Risky tool) must
/// also resolve without a live round trip — this is the "posture blocks
/// without ever surfacing a prompt" guarantee (spec.md FR-007/SC-003)
/// distinct from a live `Ask` that a human then denies.
#[tokio::test]
async fn plan_mode_deny_on_a_risky_tool_skips_the_live_round_trip() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = VaultGate::new()
        .vault_db(Arc::new(open_vault(dir.path())))
        .expect("open gate");
    db.write(|tx| {
        preferences::insert_or_update(tx, PrefScope::Vault, "chat.permission_mode", "plan")?;
        Ok(())
    })
    .await
    .expect("set permission mode");

    let pending: PendingToolApprovals = Arc::new(Mutex::new(HashMap::new()));
    let (emit, emitted) = never_emitting();

    let decision = request_approval(
        &pending,
        &emit,
        Some(&db),
        &[],
        Some(Uuid::nil()),
        AutonomyMode::Standard,
        Path::new("/tmp"),
        "Bash".to_string(), // not in the safe list => Risky
        serde_json::json!({"command": "rm -rf /"}),
        ApprovalRequestPayload::ClaudeToolCall {
            tool_name: "Bash".to_string(),
            input: serde_json::json!({"command": "rm -rf /"}),
        },
    )
    .await;

    assert_eq!(decision, ApprovalDecision::Deny);
    assert!(
        pending.lock().expect("pending lock").is_empty(),
        "Deny must never register a pending approval"
    );
    assert!(
        !emitted.load(Ordering::SeqCst),
        "Deny must never emit tool-permission-request"
    );
}

/// spec FR-005/US2, plus the code review's transaction/redaction fix:
/// a `gated-permissive` call persists exactly one `tool_call`/`tool_result`
/// audit row pair, and the call row never carries the raw tool input (it
/// can contain command arguments, file contents, or credentials).
#[tokio::test]
async fn gated_permissive_allow_persists_an_audit_pair_without_raw_input() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = VaultGate::new()
        .vault_db(Arc::new(open_vault(dir.path())))
        .expect("open gate");
    // No deny rules configured — spec's documented "fully permissive" edge case (an empty set).

    let pending: PendingToolApprovals = Arc::new(Mutex::new(HashMap::new()));
    let (emit, emitted) = never_emitting();
    let thread_id = Uuid::new_v4();
    let secret_input =
        serde_json::json!({"command": "curl -H 'Authorization: Bearer sekret' https://internal"});

    let decision = request_approval(
        &pending,
        &emit,
        Some(&db),
        &[],
        Some(thread_id),
        AutonomyMode::GatedPermissive,
        Path::new("/tmp"),
        "Bash".to_string(),
        secret_input.clone(),
        ApprovalRequestPayload::ClaudeToolCall {
            tool_name: "Bash".to_string(),
            input: secret_input,
        },
    )
    .await;

    assert_eq!(decision, ApprovalDecision::Allow);
    assert!(
        !emitted.load(Ordering::SeqCst),
        "gated-permissive must never emit tool-permission-request"
    );

    let rows = query::read(&db, |r| msg_store::list_messages(r, thread_id))
        .expect("list persisted audit rows");
    assert_eq!(rows.len(), 2, "expected exactly one call/result row pair");
    let call_row = rows
        .iter()
        .find(|m| m.role == MessageRole::ToolCall)
        .expect("call row persisted");
    let result_row = rows
        .iter()
        .find(|m| m.role == MessageRole::ToolResult)
        .expect("result row persisted");
    assert!(
        !call_row
            .tool_input
            .as_deref()
            .unwrap_or_default()
            .contains("sekret"),
        "the secret itself must not leak into the audit row"
    );
    assert_eq!(call_row.tool_call_id, result_row.tool_call_id);
    assert_eq!(result_row.tool_is_error, Some(false));
}

/// If no thread is available to persist the audit record against, the
/// call must be denied rather than silently allowed unaudited (the same
/// fail-closed reasoning the transaction fix applies to a commit failure).
#[tokio::test]
async fn gated_permissive_denies_when_there_is_no_thread_to_audit_against() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = VaultGate::new()
        .vault_db(Arc::new(open_vault(dir.path())))
        .expect("open gate");
    let pending: PendingToolApprovals = Arc::new(Mutex::new(HashMap::new()));
    let (emit, _emitted) = never_emitting();

    let decision = request_approval(
        &pending,
        &emit,
        Some(&db),
        &[],
        None,
        AutonomyMode::GatedPermissive,
        Path::new("/tmp"),
        "Bash".to_string(),
        serde_json::json!({"command": "echo hi"}),
        ApprovalRequestPayload::ClaudeToolCall {
            tool_name: "Bash".to_string(),
            input: serde_json::json!({"command": "echo hi"}),
        },
    )
    .await;

    assert_eq!(decision, ApprovalDecision::Deny);
}
