//! Test-only MLS Tauri commands, gated behind the `e2e-hooks` Cargo feature.
//!
//! Companion specs in haex-e2e-tests use these to build adversarial MLS
//! commits an honest client can never emit (a Remove from a member holding
//! no Invite capability, a captured UCAN replayed onto a different commit)
//! and to observe the receive-side gate's decision as structured data —
//! the production path only `eprintln!`s its rejection reason
//! (`space_delivery::local::sync_loop::mls`), which no spec can read.
//!
//! Scope discipline, mirroring `crdt::commands::apply::e2e_hooks`:
//!
//! - **Send side may be bypassed.** `test_mls_remove_member_unchecked`
//!   skips `authorization::authorize_local_removal` and the proof-
//!   attachment block. That is the whole point: it plays the attacker.
//! - **Receive side is NEVER bypassed.** `test_mls_process_commit_report`
//!   calls the same `MlsManager::decrypt` production uses, with the same
//!   gates in the same order. It only adds observation (epoch before/after,
//!   classified rejection reason) around it.
//!
//! NEVER compile into a binary shipped to end users.

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use serde::{Deserialize, Serialize};
use tauri::State;
use ts_rs::TS;

use crate::mls::manager::MlsManager;
use crate::AppState;

/// Which gate rejected the commit. Derived by matching the production
/// rejection strings, so a reworded message fails the spec loudly instead
/// of silently reclassifying an attack as "some other rejection".
#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TestCommitGateOutcome {
    /// Merged — the local epoch advanced.
    Accepted,
    /// Phase-1 (`authorization::authorize`): addee not a member, credential
    /// instability, unmodelled proposal type.
    RejectedPhase1 { reason: String },
    /// Phase-2 (`authorization::verify_pops`).
    RejectedPop { reason: String },
    /// The commit-bind signature did not verify against this commit
    /// (`commit_bind::verify_commit_bind_bytes`) — replay defence.
    RejectedCommitBind { reason: String },
    /// Phase-3 (`authorization::authorize_committer_capability`).
    RejectedCommitterCapability { reason: String },
    /// Anything else (openmls parse/process failure, missing group, …).
    RejectedOther { reason: String },
}

/// Result of feeding one commit into the real receive path.
#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct TestCommitGateReport {
    pub outcome: TestCommitGateOutcome,
    /// MLS epoch before / after. Equal on every rejection — the assertion
    /// that the gate really is fail-closed on the group state, not just on
    /// the returned error.
    pub epoch_before: u64,
    pub epoch_after: u64,
    /// Whether `resolve_presented_committer_capability` produced a
    /// capability at all, and its audience/level when it did. Lets a spec
    /// tell "UCAN failed to verify" apart from "UCAN verified but the gate
    /// still said no" — indistinguishable in the production error string.
    pub resolved_audience_did: Option<String>,
    pub resolved_level: Option<String>,
}

#[cfg(test)]
#[path = "e2e_hooks_tests.rs"]
mod tests;
