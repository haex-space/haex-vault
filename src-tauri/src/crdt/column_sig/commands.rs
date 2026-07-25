//! Tauri command for batch column-signature verification.
//!
//! The TS sync-apply path (`src/stores/sync/orchestrator/pull/apply.ts`)
//! calls this command in Runde 7 (Task H1) to verify a batch of pulled
//! column changes in one IPC round-trip. Rust owns the verifier — TS
//! never re-implements the signature check. Batching amortises the IPC
//! cost (a single pull page can carry hundreds of column changes and
//! each one runs a full Ed25519 verify against the domain-separated
//! preimage from `super::preimage`).
//!
//! Mirrors the Phase-2 pattern from
//! [`crate::ucan::commands::verify_ucan_chain_batch`]:
//! - `#[serde(rename_all = "camelCase")]` on every wire struct.
//! - Pure [`verify_column_sig_batch_inner`] + thin `#[tauri::command]`
//!   wrapper so the mapping logic is testable without a Tauri State.
//! - Row-scoped rejection (`{ verified, rejected }`) instead of a
//!   batch abort — one bad change never poisons the rest of the batch.
//! - Composite `row_key`
//!   `${tableName}|${rowPks}|${columnName}|${hlcTimestamp}` echoed back
//!   so the TS caller can correlate outcomes to its own map without
//!   maintaining a positional index.

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use serde::{Deserialize, Serialize};

use super::verify::{verify_column_sig, VerifyColumnSigError};
use crate::crdt::commands::apply::ColumnSig;

/// One column change plus the sig record from the wire. Field vocabulary
/// mirrors `RemoteColumnChange` / `ColumnSig` from
/// [`crate::crdt::commands::apply::types`], with two adjustments for the
/// server-sync path:
///   - `value_bytes` is base64-STANDARD-encoded rather than a `JsonValue`,
///     because the TS pull-path already has the canonical bytes on hand
///     (they came from the DB via `push.ts`) — no re-canonicalisation
///     round-trip.
///   - `sig` is mandatory (not `Option`), because a change without a
///     sig can never verify and would only clutter the batch. TS
///     filters those out before invoking.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnSigChange {
    pub table_name: String,
    pub row_pks: String,
    pub column_name: String,
    pub hlc_timestamp: String,
    /// base64-STANDARD-encoded canonical value bytes; must match the
    /// preimage byte-for-byte for the sig to verify.
    pub value_bytes: String,
    pub sig: ColumnSig,
}

/// Batch input. `expected_space_id` binds every change in the batch to
/// one space — a sig valid in space A does not verify against space B
/// because `space_id` is in the preimage (ADR 0002 §4b).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyColumnSigBatchInput {
    pub changes: Vec<ColumnSigChange>,
    pub expected_space_id: String,
}

/// Per-change rejection with the composite `row_key` + a stable variant-
/// name reason. Reason strings are the vocabulary a TS caller pattern-
/// matches on; keep them in sync with [`verify_error_variant_name`].
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RejectedRecord {
    pub row_key: String,
    pub reason: String,
}

/// Batch output. `verified` lists composite `row_key`s that passed;
/// `rejected` lists ones that failed with a reason. Order within each
/// list matches the input iteration order.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VerifyColumnSigBatchOutput {
    pub verified: Vec<String>,
    pub rejected: Vec<RejectedRecord>,
}

fn compose_row_key(c: &ColumnSigChange) -> String {
    format!(
        "{}|{}|{}|{}",
        c.table_name, c.row_pks, c.column_name, c.hlc_timestamp
    )
}

/// Stable variant-name for a [`VerifyColumnSigError`]. Written as an
/// explicit match so any new variant added to the enum forces a compile-
/// time miss here — the same guarantee the Phase-2 UCAN verifier gives
/// via `ucan_verify_error_variant_name`. TS `apply.ts` will pattern-
/// match on these strings; silent drift is a real bug.
fn verify_error_variant_name(e: &VerifyColumnSigError) -> &'static str {
    match e {
        VerifyColumnSigError::MalformedDid(_) => "MalformedDid",
        VerifyColumnSigError::InvalidSignature => "InvalidSignature",
        VerifyColumnSigError::MalformedSignatureBytes => "MalformedSignatureBytes",
        VerifyColumnSigError::ValueBytesTooLarge { .. } => "ValueBytesTooLarge",
    }
}

/// Pure batch-verify. Extracted from [`verify_column_sig_batch`] so
/// tests can exercise the full mapping without a Tauri command handle.
/// This function never fails at the batch level — a bad base64 payload
/// or a bad sig is a per-change `Rejected` entry with a specific reason.
pub(crate) fn verify_column_sig_batch_inner(
    input: VerifyColumnSigBatchInput,
) -> VerifyColumnSigBatchOutput {
    let VerifyColumnSigBatchInput {
        changes,
        expected_space_id,
    } = input;

    let mut verified = Vec::new();
    let mut rejected = Vec::new();

    for change in changes {
        let row_key = compose_row_key(&change);

        let value_bytes = match BASE64.decode(change.value_bytes.as_bytes()) {
            Ok(v) => v,
            Err(_) => {
                rejected.push(RejectedRecord {
                    row_key,
                    reason: "MalformedValueBytes".to_string(),
                });
                continue;
            }
        };

        let sig_bytes = match BASE64.decode(change.sig.sig.as_bytes()) {
            Ok(v) => v,
            Err(_) => {
                rejected.push(RejectedRecord {
                    row_key,
                    reason: "MalformedSignatureBytes".to_string(),
                });
                continue;
            }
        };

        match verify_column_sig(
            expected_space_id.as_bytes(),
            change.table_name.as_bytes(),
            change.row_pks.as_bytes(),
            change.column_name.as_bytes(),
            change.hlc_timestamp.as_bytes(),
            &change.sig.author_did,
            &value_bytes,
            &sig_bytes,
        ) {
            Ok(()) => verified.push(row_key),
            Err(e) => rejected.push(RejectedRecord {
                row_key,
                reason: verify_error_variant_name(&e).to_string(),
            }),
        }
    }

    VerifyColumnSigBatchOutput { verified, rejected }
}

/// Batch-verify column signatures for the TS sync-apply path.
///
/// Never errors on verification failure — bad changes populate
/// `rejected` with a stable reason string. Pure crypto, no DB access,
/// so no infrastructure failure mode exists today.
#[tauri::command]
pub fn verify_column_sig_batch(input: VerifyColumnSigBatchInput) -> VerifyColumnSigBatchOutput {
    verify_column_sig_batch_inner(input)
}
