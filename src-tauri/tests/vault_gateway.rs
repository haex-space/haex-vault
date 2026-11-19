//! The invoke-handler wrapper over Tauri's real IPC path (mock runtime): everything passes while
//! the gate is `Idle` or `Active`, and once it is `Closing` only the app-scoped allow-list passes
//! and every other command, including one added later, is denied at once (spec 013, FR-001, R3).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use haex_lib::vault_gate::VaultGate;
use tauri::http::HeaderMap;
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{get_ipc_response, mock_builder, mock_context, noop_assets, INVOKE_KEY};
use tauri::webview::InvokeRequest;

/// Bodies that must never run once the gate is closing.
static VAULT_BODY_RUNS: AtomicUsize = AtomicUsize::new(0);
static NEW_BODY_RUNS: AtomicUsize = AtomicUsize::new(0);
/// The counters are process-wide, so the tests take turns.
static TURN: Mutex<()> = Mutex::new(());

/// A command that touches the vault (any name outside the allow-list).
#[tauri::command]
async fn list_threads() -> &'static str {
    VAULT_BODY_RUNS.fetch_add(1, Ordering::SeqCst);
    "vault data"
}

/// The name is on the allow-list of app-scoped commands.
#[tauri::command]
async fn get_hardware_info() -> &'static str {
    "hardware"
}

/// Not in the allow-list and unknown to the gate: a command added later.
#[tauri::command]
async fn brand_new_command() -> &'static str {
    NEW_BODY_RUNS.fetch_add(1, Ordering::SeqCst);
    "new"
}

fn build_webview(gate: &VaultGate) -> tauri::WebviewWindow<tauri::test::MockRuntime> {
    let app = mock_builder()
        .manage(gate.clone())
        .invoke_handler(gate.wrap(tauri::generate_handler![
            list_threads,
            get_hardware_info,
            brand_new_command
        ]))
        .build(mock_context(noop_assets()))
        .expect("mock app");
    // Keep the app alive for the whole test process.
    let app: &'static _ = Box::leak(Box::new(app));
    tauri::WebviewWindowBuilder::new(app, "main", Default::default())
        .build()
        .expect("mock webview")
}

fn invoke(
    webview: &tauri::WebviewWindow<tauri::test::MockRuntime>,
    cmd: &str,
) -> Result<serde_json::Value, serde_json::Value> {
    get_ipc_response(
        webview,
        InvokeRequest {
            cmd: cmd.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: "tauri://localhost".parse().expect("url"),
            body: InvokeBody::default(),
            headers: HeaderMap::new(),
            invoke_key: INVOKE_KEY.to_string(),
        },
    )
    .map(|body| body.deserialize::<serde_json::Value>().expect("json body"))
}

fn vault_closed() -> serde_json::Value {
    serde_json::json!({ "kind": "VaultClosed" })
}

#[test]
fn every_command_passes_while_the_gate_is_idle_or_active() {
    let _turn = TURN.lock().unwrap_or_else(|e| e.into_inner());
    let gate = VaultGate::new();
    let webview = build_webview(&gate);

    for phase in ["idle", "active"] {
        if phase == "active" {
            gate.begin_session().expect("Idle to Active");
        }
        assert_eq!(
            invoke(&webview, "list_threads"),
            Ok("vault data".into()),
            "{phase}"
        );
        assert_eq!(
            invoke(&webview, "get_hardware_info"),
            Ok("hardware".into()),
            "{phase}"
        );
        assert_eq!(
            invoke(&webview, "brand_new_command"),
            Ok("new".into()),
            "{phase}"
        );
    }
}

#[test]
fn a_closing_gate_rejects_a_vault_command_at_once_and_never_runs_its_body() {
    let _turn = TURN.lock().unwrap_or_else(|e| e.into_inner());
    let gate = VaultGate::new();
    let webview = build_webview(&gate);
    gate.begin_session().expect("Idle to Active");
    let runs_before = VAULT_BODY_RUNS.load(Ordering::SeqCst);

    gate.request_close();

    assert_eq!(invoke(&webview, "list_threads"), Err(vault_closed()));
    assert_eq!(
        VAULT_BODY_RUNS.load(Ordering::SeqCst),
        runs_before,
        "the command body must not run after the close"
    );
}

#[test]
fn a_closing_gate_still_passes_an_allow_listed_command() {
    let _turn = TURN.lock().unwrap_or_else(|e| e.into_inner());
    let gate = VaultGate::new();
    let webview = build_webview(&gate);
    gate.begin_session().expect("Idle to Active");
    gate.request_close();

    assert_eq!(invoke(&webview, "get_hardware_info"), Ok("hardware".into()));
}

#[test]
fn a_command_added_later_is_denied_by_default_once_closing() {
    let _turn = TURN.lock().unwrap_or_else(|e| e.into_inner());
    let gate = VaultGate::new();
    let webview = build_webview(&gate);
    gate.request_close();
    let runs_before = NEW_BODY_RUNS.load(Ordering::SeqCst);

    assert_eq!(invoke(&webview, "brand_new_command"), Err(vault_closed()));
    assert_eq!(NEW_BODY_RUNS.load(Ordering::SeqCst), runs_before);
}
