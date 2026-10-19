//! `close_instance`: closing the vault ends the app process, and nothing can refuse it
//! (spec 013 US1, FR-001 to FR-009, contracts/tauri-commands.md).
//!
//! **Phase 1** ([`begin_close`]) is synchronous and cannot fail: the gate shuts, the cancellation
//! token fires, the turn and the preload are cancelled, the page is replaced by the closing
//! spinner and the list-changed event goes out. The command returns as soon as it is done.
//! **Phase 2** ([`finish_close`]) runs in the background: it lets go of the session, drains the
//! tracked work within the ladder's limits, drops the database, and asks for the end of the
//! process. Two deadlines back it up, both ending the process from a plain thread: one armed with
//! phase 1, for a phase 2 that never gets to run, and one armed right after the end request, for
//! an event loop that does not end the process (SC-002).

use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Manager};

use crate::chat::commands::abort_turn;
use crate::chat::session::ChatState;
use crate::error::Result;
use crate::state::AppState;
use crate::vault_gate::{
    close_policy, hard_end_after, on_plain_thread, CloseEffects, ClosePolicy, DrainOutcome,
    VaultGate, COOPERATIVE_WINDOW, HARD_END_GRACE, TOTAL_LIMIT,
};
use crate::voice::VoiceState;

use super::close_effects::AppCloseEffects;

/// The deadlines of one close.
#[derive(Debug, Clone, Copy)]
pub struct CloseTimings {
    /// How long tracked work gets to stop by itself.
    pub cooperative: Duration,
    /// The most the drain waits in total, from the close request.
    pub total: Duration,
    /// How long the normal end of the process gets before a plain thread forces it.
    pub grace: Duration,
}

impl CloseTimings {
    /// The deadlines of a real close (research R5).
    pub const PRODUCTION: Self = Self {
        cooperative: COOPERATIVE_WINDOW,
        total: TOTAL_LIMIT,
        grace: HARD_END_GRACE,
    };
}

/// Everything a close touches.
pub struct CloseContext<'a> {
    gate: &'a VaultGate,
    app_state: &'a AppState,
    chat: &'a ChatState,
    voice: &'a VoiceState,
    effects: Arc<dyn CloseEffects>,
    policy: ClosePolicy,
    timings: CloseTimings,
}

impl<'a> CloseContext<'a> {
    pub fn new(
        gate: &'a VaultGate,
        app_state: &'a AppState,
        chat: &'a ChatState,
        voice: &'a VoiceState,
        effects: Arc<dyn CloseEffects>,
        policy: ClosePolicy,
        timings: CloseTimings,
    ) -> Self {
        Self {
            gate,
            app_state,
            chat,
            voice,
            effects,
            policy,
            timings,
        }
    }

    /// Ends the process by force after `grace`, at most once per process however often this is
    /// armed.
    fn arm_forced_end(&self, grace: Duration) {
        let gate = self.gate.clone();
        let effects = Arc::clone(&self.effects);
        let policy = self.policy;
        hard_end_after(grace, move || {
            if gate.claim_forced_end() {
                effects.force_end(policy);
            }
        });
    }
}

/// Phase 1. Returns whether this call started the close; a repeated call changes nothing and
/// repeats no effect (FR-002).
pub fn begin_close(ctx: &CloseContext<'_>) -> bool {
    // Shuts the gate and fires the token in one step; from here every new request is refused.
    if !ctx.gate.request_close() {
        return false;
    }
    ctx.arm_forced_end(ctx.timings.total + ctx.timings.grace);
    let name = ctx.app_state.active_name().unwrap_or_else(|error| {
        log::warn!("could not read the active vault name: {error}");
        None
    });
    if let Err(error) = abort_turn(ctx.chat) {
        log::warn!("could not cancel the running turn: {error}");
    }
    ctx.voice.cancel_for_close();
    // A model job reads the grants between its steps; it must not hold the vault open.
    ctx.app_state.extensions().model_jobs().cancel_all();
    // A copied password does not outlive the vault (spec 034, FR-006).
    ctx.app_state.clipboard().clear_now();
    ctx.chat.cancel_preload();
    ctx.effects.show_closing_page();
    ctx.effects.announce_closed(name);
    true
}

/// Phase 2. Returns how the drain ended, for logs and tests; the process ends either way.
pub async fn finish_close(ctx: &CloseContext<'_>) -> DrainOutcome {
    // The loaded model goes first: a delegate adapter holds a `VaultDb`, and the drain cannot
    // finish while one is alive.
    ctx.chat.reset_for_close();
    let outcome = ctx
        .gate
        .drain_with(ctx.timings.cooperative, ctx.timings.total)
        .await;
    // Whatever cooperated is gone; let go of the state's own reference too, so the database
    // closes and SQLCipher wipes its key before the process ends.
    match ctx.app_state.take() {
        Ok(handle) => drop(handle),
        Err(error) => log::warn!("could not release the vault: {error}"),
    }
    // Releases the microphone and frees the cached model. Nothing here holds vault data and the
    // process is ending, so it is bounded by the time the forced end would wait anyway.
    let _ = tokio::time::timeout(ctx.timings.grace, ctx.voice.reset_for_close()).await;
    ctx.effects.request_end(ctx.policy);
    ctx.arm_forced_end(ctx.timings.grace);
    outcome
}

/// Starts the close of this app process: phase 1 now, phase 2 in the background. Returns whether
/// this call started it.
pub fn start_close(app: &AppHandle, policy: ClosePolicy) -> bool {
    let gate = app.state::<VaultGate>();
    let effects: Arc<dyn CloseEffects> =
        Arc::new(AppCloseEffects::new(app.clone(), gate.children()));
    let started = {
        let app_state = app.state::<AppState>();
        let chat = app.state::<ChatState>();
        let voice = app.state::<VoiceState>();
        begin_close(&CloseContext::new(
            &gate,
            &app_state,
            &chat,
            &voice,
            Arc::clone(&effects),
            policy,
            CloseTimings::PRODUCTION,
        ))
    };
    if started {
        let app = app.clone();
        // Not tracked: this is the task that waits for the tracker to empty.
        tauri::async_runtime::spawn(async move {
            let gate = app.state::<VaultGate>();
            let app_state = app.state::<AppState>();
            let chat = app.state::<ChatState>();
            let voice = app.state::<VoiceState>();
            let outcome = finish_close(&CloseContext::new(
                &gate,
                &app_state,
                &chat,
                &voice,
                effects,
                policy,
                CloseTimings::PRODUCTION,
            ))
            .await;
            log::info!("vault close drained: {outcome:?}");
        });
    }
    started
}

/// The user leaves by closing the window or quitting from the system menu (FR-007). Both take the
/// same close as the lock control, with the exit policy, so a running reply is cancelled and the
/// process ends within the same limits. Returns whether the request was taken over, in which case
/// the caller keeps the window and the app alive until the close has ended the process; `false`
/// once a close is already running, so the process can then end.
pub fn take_over_exit(app: &AppHandle) -> bool {
    if app.state::<VaultGate>().is_closing() {
        return false;
    }
    let handle = app.clone();
    // On a plain thread, not the async runtime: a stuck runtime must not be able to keep the user
    // from leaving.
    on_plain_thread("vault-close", Duration::ZERO, move || {
        start_close(&handle, ClosePolicy::Exit);
    });
    true
}

/// Closes the vault and ends the app process. Idempotent, never refused, and it returns as soon
/// as phase 1 is done; a second call reports success and changes nothing.
#[tauri::command]
pub async fn close_instance(app: AppHandle) -> Result<()> {
    start_close(&app, close_policy());
    Ok(())
}
