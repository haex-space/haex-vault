//! `connect_cli_delegate`/`submit_cli_delegate_code` Tauri commands (spec
//! 007-cli-delegate US2) — acquires a `cli_delegate` credential without ever
//! touching host-level `claude`/`codex` login state (research.md §3/§5).
//!
//! The two vendors have different acquisition shapes (research.md §5):
//! `codex login --device-auth` is plain-text and completes end-to-end inside
//! one command; `claude setup-token` needs a real pty and pauses after
//! showing the OAuth URL. The current Anthropic OAuth page never shows a
//! code to paste back (only research.md §5's never-verified assumption that
//! it would), so completion is primarily automatic: `connect_cli_delegate`
//! spawns a background watcher (`watch_claude_auto_completion`) that
//! detects the token once the browser round-trip finishes on its own.
//! `submit_cli_delegate_code` still exists as a fallback for a code shown by
//! some other `claude` CLI version/build.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::{Mutex as AsyncMutex, Notify};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::adapters::cli_delegate::connect_claude::{
    start_claude_connect, submit_claude_code, ClaudeConnectSession, TerminalOutcome,
};
use crate::adapters::cli_delegate::connect_codex::run_device_auth;
use crate::adapters::cli_delegate::DelegateVendor;
use crate::error::{HaexError, Result};
use crate::state::AppState;
use crate::state_utils::active_database;
use crate::storage::providers::{self as storage, Provider, ProviderCapability, ProviderKind};
use crate::vault_gate::{VaultDb, VaultGate};

use super::{map_adapter_error, ProviderPayload};

pub const DELEGATE_CONNECT_PROGRESS_EVENT: &str = "delegate-connect-progress";

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct DelegateConnectProgress {
    vendor: &'static str,
    status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

fn emit_progress(app: &AppHandle, progress: DelegateConnectProgress) {
    if let Err(error) = app.emit(DELEGATE_CONNECT_PROGRESS_EVENT, progress) {
        log::warn!("emit {DELEGATE_CONNECT_PROGRESS_EVENT} failed: {error}");
    }
}

/// Holds a paused Claude connect flow between `connect_cli_delegate`,
/// `submit_cli_delegate_code`, and the background `watch_claude_auto_completion`
/// task (contracts/tauri-commands.md). Only one flow can be pending at a
/// time; starting a new `connect_cli_delegate(claude)` call implicitly
/// abandons any previous unsubmitted flow — its PTY child is killed by
/// `ClaudeConnectSession`'s `Drop`, and the old watcher notices via its
/// session id no longer matching (`watch_claude_auto_completion`).
///
/// `pending_claude` is `pub(super)` (not private) so `connect_tests` can
/// seed and inspect it directly with a `ClaudeConnectSession` test double,
/// without a real PTY/`claude` process.
#[derive(Default)]
pub struct DelegateConnectState {
    pub(super) pending_claude: AsyncMutex<Option<ClaudeConnectSession>>,
    /// The sign-in of Abacus that is waiting for the person (spec 096): ended by
    /// `cancel_cli_delegate_connect`, by a newer sign-in, or when it ends by itself.
    abacus: std::sync::Mutex<Option<AbacusCancellation>>,
}

struct AbacusCancellation {
    id: Uuid,
    token: CancellationToken,
}

impl DelegateConnectState {
    pub fn new() -> Self {
        Self::default()
    }

    /// The token of a new Abacus sign-in; one that was still waiting is ended.
    fn begin_abacus(&self) -> (Uuid, CancellationToken) {
        let id = Uuid::new_v4();
        let token = CancellationToken::new();
        let mut slot = self.abacus.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(previous) = slot.replace(AbacusCancellation {
            id,
            token: token.clone(),
        }) {
            previous.token.cancel();
        }
        (id, token)
    }

    /// Clears a completed sign-in only if it still owns the slot. A newer connect may have
    /// replaced it while this command was returning from the vault gate.
    fn finish_abacus(&self, id: Uuid) {
        let mut slot = self.abacus.lock().unwrap_or_else(|p| p.into_inner());
        if slot.as_ref().is_some_and(|active| active.id == id) {
            slot.take();
        }
    }

    /// Ends the sign-in of Abacus that is waiting, if any. Idempotent.
    fn cancel_abacus(&self) {
        let mut slot = self.abacus.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(active) = slot.take() {
            active.token.cancel();
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectCliDelegateArgs {
    pub vendor: String,
    pub name: String,
    /// The e-mail address of the Abacus account; required for `abacusai`, unused otherwise.
    #[serde(default)]
    pub email: Option<String>,
}

/// `rename_all = "snake_case"` (not `camelCase`) so the `status` tag value
/// matches `delegate-connect-progress`'s own snake_case status vocabulary
/// (`awaiting_code`, `awaiting_browser`, `success`) — `AwaitingCode` would
/// otherwise serialize its tag as `"awaitingCode"`, inconsistent with the
/// event and with contracts/tauri-commands.md.
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum ConnectCliDelegateResult {
    AwaitingCode { vendor: &'static str },
    Connected { provider: ProviderPayload },
}

/// Starts the connect flow (contracts/tauri-commands.md). For `codex` this
/// runs the whole flow and returns `Connected`; for `claude` it returns as
/// soon as the OAuth URL is known, and the flow is completed by a later
/// `submit_cli_delegate_code` call.
#[tauri::command]
pub async fn connect_cli_delegate(
    app: AppHandle,
    state: State<'_, AppState>,
    gate: State<'_, VaultGate>,
    connect_state: State<'_, DelegateConnectState>,
    args: ConnectCliDelegateArgs,
) -> Result<ConnectCliDelegateResult> {
    let vendor = DelegateVendor::parse(&args.vendor).ok_or_else(|| HaexError::InvalidInput {
        reason: format!("unknown cli_delegate vendor \"{}\"", args.vendor),
    })?;

    match vendor {
        DelegateVendor::Claude => {
            // The vault's own installation; without one the flow ends here with the install action
            // (spec 094 FR-004), never with a copy from the host.
            let runtime = delegate_runtime(&app, &state, "claude")?;
            runtime
                .record
                .append("connect_started", &[("vendor", "claude")]);
            let (session, url) = match gate
                .run(start_claude_connect(&runtime, gate.children()))
                .await
            {
                Ok(Ok(result)) => result,
                Ok(Err(error)) => {
                    runtime
                        .record
                        .append("connect_failed", &[("vendor", "claude")]);
                    return Err(map_adapter_error(error));
                }
                Err(error) => {
                    runtime
                        .record
                        .append("connect_failed", &[("vendor", "claude")]);
                    return Err(error);
                }
            };
            let session_id = session.id();
            let token_ready = session.token_ready();
            *connect_state.pending_claude.lock().await = Some(session);
            emit_progress(
                &app,
                DelegateConnectProgress {
                    vendor: "claude",
                    status: "awaiting_code",
                    url: Some(url),
                    code: None,
                    message: None,
                },
            );
            spawn_claude_auto_completion(app.clone(), args.name, session_id, token_ready);
            Ok(ConnectCliDelegateResult::AwaitingCode { vendor: "claude" })
        }
        DelegateVendor::Codex => {
            let runtime = delegate_runtime(&app, &state, "codex")?;
            runtime
                .record
                .append("connect_started", &[("vendor", "codex")]);
            let app_for_prompt = app.clone();
            let credentials = match gate
                .run(run_device_auth(&runtime, &gate.children(), move |prompt| {
                    emit_progress(
                        &app_for_prompt,
                        DelegateConnectProgress {
                            vendor: "codex",
                            status: "awaiting_browser",
                            url: Some(prompt.url.clone()),
                            code: Some(prompt.code.clone()),
                            message: None,
                        },
                    );
                }))
                .await
            {
                Ok(Ok(credentials)) => credentials,
                Ok(Err(error)) => {
                    runtime
                        .record
                        .append("connect_failed", &[("vendor", "codex")]);
                    return Err(map_adapter_error(error));
                }
                Err(error) => {
                    runtime
                        .record
                        .append("connect_failed", &[("vendor", "codex")]);
                    return Err(error);
                }
            };

            let provider =
                match upsert_delegate_provider(&app, &state, "codex", args.name, credentials).await
                {
                    Ok(provider) => provider,
                    Err(error) => {
                        runtime
                            .record
                            .append("connect_failed", &[("vendor", "codex")]);
                        return Err(error);
                    }
                };
            runtime
                .record
                .append("connect_finished", &[("vendor", "codex")]);
            emit_progress(
                &app,
                DelegateConnectProgress {
                    vendor: "codex",
                    status: "success",
                    url: None,
                    code: None,
                    message: None,
                },
            );
            Ok(ConnectCliDelegateResult::Connected {
                provider: provider.into(),
            })
        }
        DelegateVendor::Abacus => {
            connect_abacus_command(&app, &state, &gate, &connect_state, args).await
        }
    }
}

/// The Abacus branch of [`connect_cli_delegate`]: the device sign-in, run to its end (spec 096).
async fn connect_abacus_command(
    app: &AppHandle,
    state: &State<'_, AppState>,
    gate: &State<'_, VaultGate>,
    connect_state: &State<'_, DelegateConnectState>,
    args: ConnectCliDelegateArgs,
) -> Result<ConnectCliDelegateResult> {
    let email = args
        .email
        .as_deref()
        .map(str::trim)
        .filter(|email| !email.is_empty())
        .ok_or_else(|| HaexError::InvalidInput {
            reason: "abacusai needs the e-mail address of the account".into(),
        })?;
    let email = super::abacus_signin::Secret::new(email.to_owned());
    // The tool must be in this vault first: without it the flow ends here with the install action
    // (spec 094 FR-004), before any request goes out.
    let runtime = delegate_runtime(app, state, "abacusai")?;
    let db = active_database(state)?;
    // An e2e run points the sign-in at a server on this device (debug builds only).
    let client = match super::abacus_stand_in::active() {
        Some(base) => super::abacus_signin::Client::with_base(&base),
        None => super::abacus_signin::Client::new(),
    }
    .map_err(|e| super::connect_abacus::failure(&e))?;
    let (connect_id, cancel) = connect_state.begin_abacus();
    let app_for_prompt = app.clone();
    let result = gate
        .run(super::connect_abacus::connect(
            &client,
            &db,
            &runtime.record,
            args.name,
            &email,
            &cancel,
            |prompt| {
                emit_progress(
                    &app_for_prompt,
                    DelegateConnectProgress {
                        vendor: "abacusai",
                        status: "awaiting_browser",
                        url: Some(prompt.uri.clone()),
                        code: Some(prompt.code.expose().to_owned()),
                        message: None,
                    },
                );
            },
        ))
        .await;
    connect_state.finish_abacus(connect_id);
    let provider = match result {
        Ok(Ok(provider)) => provider,
        Ok(Err(error)) => {
            emit_progress(
                app,
                DelegateConnectProgress {
                    vendor: "abacusai",
                    status: "error",
                    url: None,
                    code: None,
                    message: connect_failure_code(&error),
                },
            );
            return Err(error);
        }
        Err(error) => return Err(error),
    };
    // Best effort, as for the other vendors: the model list is the account's default.
    let tools = crate::cli_tools::tools_of_open_vault(app, state).ok();
    if super::do_refresh_with(&db, &provider, tools).await.is_err() {
        log::warn!("cli_delegate model refresh after connect failed");
    }
    emit_progress(
        app,
        DelegateConnectProgress {
            vendor: "abacusai",
            status: "success",
            url: None,
            code: None,
            message: None,
        },
    );
    Ok(ConnectCliDelegateResult::Connected {
        provider: provider.into(),
    })
}

/// The short code the progress event of a failed sign-in carries: what the dialog words.
fn connect_failure_code(error: &HaexError) -> Option<String> {
    let HaexError::InvalidInput { reason } = error else {
        return None;
    };
    let code = reason.strip_prefix("delegate.")?.split(':').next()?;
    Some(code.to_owned())
}

/// Stops the sign-in of `vendor` that is waiting for the person (the dialog was closed). Idempotent;
/// the flows of Claude Code and Codex end with their processes and need nothing.
#[tauri::command]
pub async fn cancel_cli_delegate_connect(
    connect_state: State<'_, DelegateConnectState>,
    vendor: String,
) -> Result<()> {
    if DelegateVendor::parse(&vendor) == Some(DelegateVendor::Abacus) {
        connect_state.cancel_abacus();
    }
    Ok(())
}

/// Appends a connect event to the record of the open vault (spec 094 FR-019). Best effort: a
/// vault that is gone or a record that cannot be written changes nothing.
fn record_connect(app: &AppHandle, event: &'static str, vendor: &'static str) {
    let state = app.state::<AppState>();
    if let Ok(tools) = crate::cli_tools::tools_of_open_vault(app, &state) {
        tools.record().append(event, &[("vendor", vendor)]);
    }
}

/// The runtime of `vendor` in the open vault, or the reason there is none: "not installed in this
/// vault" and "needs reinstall" reach the frontend as `backend unavailable: ...` (spec 094 FR-013).
fn delegate_runtime(
    app: &AppHandle,
    state: &State<'_, AppState>,
    vendor: &str,
) -> Result<crate::cli_tools::runtime::DelegateRuntime> {
    crate::cli_tools::tools_of_open_vault(app, state)?
        .runtime(vendor)
        .map_err(|error| {
            map_adapter_error(crate::adapters::AdapterError::Unavailable {
                reason: error.reason(),
            })
        })
}

/// Outcome of [`watch_claude_auto_completion`].
pub(super) enum ClaudeAutoCompleteOutcome {
    /// The token arrived; `Vec<u8>` is the credential blob to persist.
    Completed(Vec<u8>),
    /// The flow ended (PTY EOF or a reader error) before ever producing a
    /// token — the underlying process is gone, so no code submission could
    /// complete it either.
    Failed(String),
    /// Another caller already claimed the session (a manual
    /// `submit_cli_delegate_code` call finished first), or a later
    /// `connect_cli_delegate(claude)` call replaced it — nothing to do.
    Superseded,
}

/// Waits for a Claude connect session to reach a terminal state without a
/// manual `submit_cli_delegate_code` call — the fix for the deadlock where
/// nothing ever read the session's event channel again once
/// `connect_cli_delegate` returned `AwaitingCode`, so a token produced by
/// the browser round-trip completing on its own sat unread forever.
///
/// Never awaits `events.recv()` directly while holding `connect_state`'s
/// mutex — that would block a concurrent manual `submit_cli_delegate_code`
/// call from ever acquiring the same lock to submit a pasted code,
/// recreating the deadlock this exists to fix, just on the other caller.
/// Instead it waits on `token_ready` (no lock held) and only takes the
/// lock for a brief non-blocking check. `submit_cli_delegate_code` already
/// holds the lock for its whole attempt, so whichever side gets there
/// first "wins": this watcher simply finds `pending_claude` empty (or
/// holding a session from a later, superseding connect attempt) and
/// reports `Superseded`.
pub(super) async fn watch_claude_auto_completion(
    connect_state: &DelegateConnectState,
    session_id: Uuid,
    token_ready: Arc<Notify>,
) -> ClaudeAutoCompleteOutcome {
    loop {
        token_ready.notified().await;
        let mut guard = connect_state.pending_claude.lock().await;
        let Some(session) = guard.as_mut() else {
            return ClaudeAutoCompleteOutcome::Superseded;
        };
        if session.id() != session_id {
            return ClaudeAutoCompleteOutcome::Superseded;
        }
        match session.try_recv_terminal() {
            Some(TerminalOutcome::Token(token)) => {
                session.runtime().confirm_run().await;
                guard.take();
                return ClaudeAutoCompleteOutcome::Completed(token.into_bytes());
            }
            Some(TerminalOutcome::Ended) => {
                guard.take();
                return ClaudeAutoCompleteOutcome::Failed(
                    "claude setup-token ended before producing a token".into(),
                );
            }
            Some(TerminalOutcome::Errored(error)) => {
                guard.take();
                return ClaudeAutoCompleteOutcome::Failed(error);
            }
            None => continue,
        }
    }
}

/// Spawns [`watch_claude_auto_completion`] and, once it resolves, does
/// whatever `connect_cli_delegate`/`submit_cli_delegate_code` would have
/// done on success or failure: upsert the credential and emit the same
/// `success`/`error` `delegate-connect-progress` the frontend already
/// listens for. A `Superseded` outcome emits nothing — the flow that
/// superseded this one owns reporting its own result.
fn spawn_claude_auto_completion(
    app: AppHandle,
    name: String,
    session_id: Uuid,
    token_ready: Arc<Notify>,
) {
    // A closing gate refuses the watcher; the flow is over with the vault then.
    let gate = app.state::<VaultGate>().inner().clone();
    let _ = gate.spawn(async move {
        let connect_state = app.state::<DelegateConnectState>();
        let outcome = watch_claude_auto_completion(&connect_state, session_id, token_ready).await;
        match outcome {
            ClaudeAutoCompleteOutcome::Completed(token) => {
                let state = app.state::<AppState>();
                let progress =
                    match upsert_delegate_provider(&app, &state, "claude", name, token).await {
                        Ok(_provider) => DelegateConnectProgress {
                            vendor: "claude",
                            status: "success",
                            url: None,
                            code: None,
                            message: None,
                        },
                        Err(error) => DelegateConnectProgress {
                            vendor: "claude",
                            status: "error",
                            url: None,
                            code: None,
                            message: Some(super::format_haex_error(&error)),
                        },
                    };
                let event = if progress.status == "success" {
                    "connect_finished"
                } else {
                    "connect_failed"
                };
                record_connect(&app, event, "claude");
                emit_progress(&app, progress);
            }
            ClaudeAutoCompleteOutcome::Failed(message) => {
                record_connect(&app, "connect_failed", "claude");
                emit_progress(
                    &app,
                    DelegateConnectProgress {
                        vendor: "claude",
                        status: "error",
                        url: None,
                        code: None,
                        message: Some(message),
                    },
                );
            }
            ClaudeAutoCompleteOutcome::Superseded => {}
        }
    });
}

/// The code of the error a submitted sign-in code that was not accepted ends with; the settings
/// page shows it as its own message.
pub const CODE_REJECTED: &str = "cli_delegate.code_rejected";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitCliDelegateCodeArgs {
    pub code: String,
    pub name: String,
}

/// Completes a Claude connect flow started by `connect_cli_delegate`
/// (contracts/tauri-commands.md). A failure (e.g. a rejected code) ends the flow: the
/// underlying `claude setup-token` answers a bad code with "Press Enter to retry" and a new
/// sign-in, so the user connects again for a new link.
#[tauri::command]
pub async fn submit_cli_delegate_code(
    app: AppHandle,
    state: State<'_, AppState>,
    gate: State<'_, VaultGate>,
    connect_state: State<'_, DelegateConnectState>,
    args: SubmitCliDelegateCodeArgs,
) -> Result<ProviderPayload> {
    let mut guard = connect_state.pending_claude.lock().await;
    let mut session = guard.take().ok_or_else(|| HaexError::InvalidInput {
        reason: "no claude connect flow is currently awaiting a code".into(),
    })?;

    let token = match gate
        .run(submit_claude_code(&mut session, &args.code))
        .await?
    {
        Ok(token) => token,
        Err(error) => {
            // The flow ends with a failed submit: after a code that was not accepted the program
            // waits for Enter to start another sign-in, so this session cannot take the next
            // code. Dropping it stops the program; the user connects again for a new link.
            drop(session);
            return Err(match error {
                crate::adapters::AdapterError::InvalidCredentials => HaexError::InvalidInput {
                    reason: format!("{CODE_REJECTED}: the sign-in code was not accepted"),
                },
                other => map_adapter_error(other),
            });
        }
    };
    drop(guard);

    let provider = match upsert_delegate_provider(&app, &state, "claude", args.name, token).await {
        Ok(provider) => provider,
        Err(error) => {
            record_connect(&app, "connect_failed", "claude");
            return Err(error);
        }
    };
    record_connect(&app, "connect_finished", "claude");
    emit_progress(
        &app,
        DelegateConnectProgress {
            vendor: "claude",
            status: "success",
            url: None,
            code: None,
            message: None,
        },
    );
    Ok(provider.into())
}

/// Inserts or, if a `cli_delegate` row already exists for `vendor`, updates
/// its credentials in place (upsert semantics, contracts/tauri-commands.md —
/// FR-014 reconnect is the same command pair, not a separate route). Refresh
/// failures are logged, not surfaced — matches `add_provider`'s existing
/// "insert always succeeds, refresh is best-effort" behavior.
async fn upsert_delegate_provider(
    app: &AppHandle,
    state: &State<'_, AppState>,
    vendor: &str,
    name: String,
    credentials: Vec<u8>,
) -> Result<Provider> {
    let db = active_database(state)?;
    let provider = write_delegate_provider(&db, vendor, name, credentials).await?;

    // Fixed text only: the error may carry what the tool printed.
    let tools = crate::cli_tools::tools_of_open_vault(app, state).ok();
    if super::do_refresh_with(&db, &provider, tools).await.is_err() {
        log::warn!("cli_delegate model refresh after connect failed");
    }

    Ok(provider)
}

/// Writes the `cli_delegate` row of `vendor`: a new one, or the credentials of the existing one.
/// The row names no program and no address: the tool is the vault's own installation (spec 094).
pub(super) async fn write_delegate_provider(
    db: &VaultDb,
    vendor: &str,
    name: String,
    credentials: Vec<u8>,
) -> Result<Provider> {
    let vendor_owned = vendor.to_string();
    db.write(move |tx| {
        // A vendor whose credential is in the password manager has one row per vault: rows that
        // a sync of two devices left side by side are reduced to the one with the smallest id.
        let existing = if super::delegate_credential::lives_in_password_manager(&vendor_owned) {
            storage::reduce_cli_delegate_duplicates(tx, &vendor_owned)?
        } else {
            storage::find_cli_delegate_provider(tx, &vendor_owned)?
        };
        match existing {
            Some(id) => {
                let mut provider =
                    storage::get_provider(tx, id)?.ok_or_else(|| HaexError::CrdtInit {
                        reason: "cli_delegate provider vanished during reconnect".into(),
                    })?;
                provider.base_url = None;
                provider.credentials = Some(credentials);
                storage::update_provider(tx, &provider)?;
                Ok(Some(provider))
            }
            None => {
                let provider = Provider {
                    id: Uuid::new_v4(),
                    kind: ProviderKind::CliDelegate,
                    adapter: Some(vendor_owned.clone()),
                    name,
                    // The tool is the vault's own installation (spec 094): no binary name.
                    base_url: None,
                    credentials: Some(credentials),
                    created_at: super::now_ms(),
                    capability: ProviderCapability::Chat,
                };
                storage::insert_provider(tx, &provider)?;
                Ok(Some(provider))
            }
        }
    })
    .await?
    .ok_or_else(|| HaexError::CrdtInit {
        reason: "cli_delegate provider vanished immediately after being written".into(),
    })
}
