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

/// Map a production rejection string onto the gate that produced it.
///
/// Substring matching is deliberate and deliberately narrow: it couples
/// the specs to the exact wording of each gate, so rewording a message
/// without updating this function fails the spec instead of silently
/// downgrading a real attack to `RejectedOther`.
pub(crate) fn classify_rejection(err: &str) -> TestCommitGateOutcome {
    let reason = err.to_string();
    if err.contains("commit-bind signature invalid")
        || err.contains("presented without a commit-bind signature")
    {
        TestCommitGateOutcome::RejectedCommitBind { reason }
    } else if err.contains("requires a committer capability proof")
        || err.contains("does not match the commit's committer")
        || err.contains("membership removal requires Invite-or-higher")
        || err.contains("has no resolvable committer DID")
    {
        TestCommitGateOutcome::RejectedCommitterCapability { reason }
    } else if err.contains("proof-of-possession") || err.contains("PoP") {
        TestCommitGateOutcome::RejectedPop { reason }
    } else if err.contains("is not a member of this space")
        || err.contains("credential-stability violation")
        || err.contains("unmodelled proposal type")
        || err.contains("addee credential is empty")
    {
        TestCommitGateOutcome::RejectedPhase1 { reason }
    } else {
        TestCommitGateOutcome::RejectedOther { reason }
    }
}

/// Feed one commit into the REAL receive path
/// (`MlsManager::decrypt` — same gates, same order as
/// `sync_loop::mls::fetch_and_process_mls_messages`) and report the
/// decision as structured data.
///
/// `committer_ucan` / `committer_commit_bind_sig` are supplied by the
/// caller exactly as the wire would carry them, so a spec can present a
/// forged, expired, replayed or absent proof. The UCAN still goes through
/// the production `resolve_presented_committer_capability` — nothing about
/// verification is bypassed here.
#[tauri::command]
pub async fn test_mls_process_commit_report(
    state: State<'_, AppState>,
    space_id: String,
    commit_b64: String,
    committer_ucan: Option<String>,
    committer_commit_bind_sig_b64: Option<String>,
) -> Result<TestCommitGateReport, String> {
    let commit = BASE64
        .decode(&commit_b64)
        .map_err(|e| format!("commit_b64 is not valid base64: {e}"))?;
    let bind_sig = match committer_commit_bind_sig_b64.as_deref() {
        Some(s) => Some(
            BASE64
                .decode(s)
                .map_err(|e| format!("bind sig is not valid base64: {e}"))?,
        ),
        None => None,
    };

    let db = crate::database::DbConnection(state.db.0.clone());
    let presented = crate::space_delivery::local::ucan::resolve_presented_committer_capability(
        &db,
        &space_id,
        committer_ucan.as_deref(),
    );
    let resolved_audience_did = presented.as_ref().map(|p| p.audience_did.clone());
    let resolved_level = presented.as_ref().map(|p| format!("{:?}", p.level));

    let manager = MlsManager::new(state.db.0.clone());
    let epoch_before = manager.current_epoch(&space_id)?;
    let outcome = match manager.decrypt(&space_id, &commit, presented, bind_sig.as_deref()) {
        Ok(_) => TestCommitGateOutcome::Accepted,
        Err(e) => classify_rejection(&e),
    };
    let epoch_after = manager.current_epoch(&space_id)?;

    Ok(TestCommitGateReport {
        outcome,
        epoch_before,
        epoch_after,
        resolved_audience_did,
        resolved_level,
    })
}

#[cfg(test)]
#[path = "e2e_hooks_tests.rs"]
mod tests;
