//! Tauri command for batch UCAN chain verification.
//!
//! The TS sync-apply path (`src/stores/sync/orchestrator/pull/apply.ts`)
//! calls this command to verify a batch of pulled rows in one IPC
//! round-trip. Rust is the single source of verification truth — TS never
//! re-implements the chain walk. Batching amortises the IPC cost, which
//! matters because a single pull page can carry hundreds of rows and each
//! one needs the full pipeline (signature, audience, capability floor,
//! prf-chain walk to a self-signed root, self-certifying `space_id`
//! binding).
//!
//! The command reads `max_ucan_chain_depth` once via
//! [`crate::ucan::read_max_ucan_chain_depth`] and reuses it for every
//! request in the batch. Reads bypass `select_with_crdt` — the depth cap
//! is device-local security config, not CRDT-synced state.

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::database::core::with_connection;
use crate::ucan::capability_set::Cap;
use crate::ucan::config::read_max_ucan_chain_depth;
use crate::ucan::verify::{validate_token, UcanVerifyError};
use crate::AppState;

/// One item in the batch: a token plus the parameters the verifier needs
/// to bind it to a target space + operation. `row_id` and `table_name`
/// are echoed back in the response so the TS caller can correlate each
/// outcome with the row it came from without maintaining an index map.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyChainRequest {
    /// The signed UCAN leaf JWT (base64url-encoded `header.payload.sig`).
    pub token: String,
    /// The self-certifying `space_id` the token is claiming authority for.
    pub expected_space_id: String,
    /// The recipient DID the token must be addressed to.
    pub expected_audience: String,
    /// Minimum capability required for this row's operation (write, read, …).
    pub capability_needed: Cap,
    /// Opaque row identifier, echoed back in the response.
    pub row_id: String,
    /// Table the row came from, echoed back in the response. Included so
    /// callers debugging a rejected row can reconstruct which table's
    /// apply-path emitted it without an extra join.
    pub table_name: String,
}

/// Per-request outcome. Ok carries the resolved chain-root DID (the
/// Space-Root DID that `expected_space_id` binds to); Rejected carries
/// the stable variant-name string for the error — matched to the fixture
/// vocabulary in `chain_tests::variant_name`.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VerifyOutcome {
    Ok {
        #[serde(rename = "rootDid")]
        root_did: String,
    },
    Rejected {
        reason: String,
    },
}

/// Wrapper pairing the outcome with `row_id`/`table_name` echoed from the
/// request. Order in the response matches the order of the input `Vec`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyChainResult {
    pub row_id: String,
    pub table_name: String,
    pub outcome: VerifyOutcome,
}

/// Batch-verify UCAN chains for the TS sync-apply path.
///
/// Returns one [`VerifyChainResult`] per input, in the same order. The
/// command never returns `Err` for verification failures — a bad token
/// is a per-row `Rejected` outcome, not a batch-level error. `Err(String)`
/// is reserved for infrastructure faults (DB unavailable in a way that
/// prevents any verification from running).
///
/// Reading `max_ucan_chain_depth` also fails soft: if the DB read errors,
/// we fall back to [`crate::ucan::MAX_UCAN_CHAIN_DEPTH_DEFAULT`] rather
/// than rejecting the whole batch. This mirrors the fail-open-on-config-
/// read stance in `handle_stream` / `require_valid_ucan`.
#[tauri::command]
pub async fn verify_ucan_chain_batch(
    state: State<'_, AppState>,
    requests: Vec<VerifyChainRequest>,
) -> Result<Vec<VerifyChainResult>, String> {
    let max_depth = with_connection(&state.db, |conn| Ok(read_max_ucan_chain_depth(conn)))
        .unwrap_or(crate::ucan::MAX_UCAN_CHAIN_DEPTH_DEFAULT) as usize;

    Ok(verify_chain_batch_inner(requests, max_depth))
}

/// Pure batch-verify function. Extracted from [`verify_ucan_chain_batch`]
/// so tests can exercise the full request → outcome mapping without a
/// Tauri `State` handle. The Tauri command layer is a thin adapter that
/// only handles DB access + depth-read + IPC serialisation.
pub(crate) fn verify_chain_batch_inner(
    requests: Vec<VerifyChainRequest>,
    max_chain_depth: usize,
) -> Vec<VerifyChainResult> {
    requests
        .into_iter()
        .map(|req| {
            let outcome = match validate_token(
                &req.token,
                &req.expected_space_id,
                &req.expected_audience,
                req.capability_needed,
                max_chain_depth,
            ) {
                Ok(v) => VerifyOutcome::Ok {
                    root_did: v.root_did,
                },
                Err(e) => VerifyOutcome::Rejected {
                    reason: ucan_verify_error_variant_name(&e).to_string(),
                },
            };
            VerifyChainResult {
                row_id: req.row_id,
                table_name: req.table_name,
                outcome,
            }
        })
        .collect()
}

/// Stable variant-name for a [`UcanVerifyError`], written out explicitly
/// (not via `Debug` parsing or serde) so any new variant added to the
/// enum forces a compile-time miss here. The same vocabulary is used by
/// the fixture at `tests/fixtures/ucan_chain_vectors.json` and by the
/// mirror `variant_name` in `verify/chain_tests.rs` — TS `apply.ts` must
/// pattern-match on these strings, so silent drift between the two
/// locations would be a real bug.
pub(crate) fn ucan_verify_error_variant_name(e: &UcanVerifyError) -> &'static str {
    match e {
        UcanVerifyError::MalformedToken(_) => "MalformedToken",
        UcanVerifyError::Signature => "Signature",
        UcanVerifyError::Expired => "Expired",
        UcanVerifyError::AudienceMismatch { .. } => "AudienceMismatch",
        UcanVerifyError::EmptyExpectedAudience => "EmptyExpectedAudience",
        UcanVerifyError::MissingCapability { .. } => "MissingCapability",
        UcanVerifyError::InsufficientCapability { .. } => "InsufficientCapability",
        UcanVerifyError::UnknownCapability(_) => "UnknownCapability",
        UcanVerifyError::ChainTooDeep(_) => "ChainTooDeep",
        UcanVerifyError::ChainBroken => "ChainBroken",
        UcanVerifyError::DelegationMissing { .. } => "DelegationMissing",
        UcanVerifyError::DelegationNotDelegatable { .. } => "DelegationNotDelegatable",
        UcanVerifyError::RowCapAttenuation { .. } => "RowCapAttenuation",
        UcanVerifyError::RootNotSelfSigned => "RootNotSelfSigned",
        UcanVerifyError::RootBindingMismatch => "RootBindingMismatch",
        UcanVerifyError::RootBindingMalformed => "RootBindingMalformed",
        UcanVerifyError::WrongSpace => "WrongSpace",
    }
}

#[cfg(test)]
mod tests;
