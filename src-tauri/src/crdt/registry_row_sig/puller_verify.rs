//! Puller-side verify-on-apply for `haex_shared_space_sync` registry rows
//! (Task B.4).
//!
//! For every incoming registry-row change (INSERT or UPDATE) the puller
//! must, before applying it:
//!   1. Reconstruct the canonical [`RegistryRowSigPayload`] from the
//!      change's column values.
//!   2. Verify `row_sig` against the Ed25519 public key derived from
//!      `authored_by_did`.
//!   3. On UPDATE, additionally reject a change that claims a different
//!      `authored_by_did` than the row's existing value — `authored_by_did`
//!      is immutable post-creation (mirrors the B.3 local-write guard,
//!      enforced here at the peer boundary; see
//!      `database::core::execute::sign_registry_row_self`).
//!
//! Extension-manifest verification (does the claimed `extension_public_key`
//! / `extension_name` actually belong to an installed, compatible
//! extension?) is explicitly OUT OF SCOPE — that is Task E.3, wired into
//! this pipeline by Task F.1. This module only proves authorship + identity
//! integrity of the registry row itself.
//!
//! Not yet wired into the real pull-apply pipeline
//! (`crdt::commands::apply::apply_remote_changes_to_db_scoped`) — that is
//! Task B.5. This module is deliberately standalone and pure (no DB
//! access): the caller is responsible for reconstructing
//! [`IncomingRegistryChange`] from the batch of `RemoteColumnChange`s for
//! one row (merged with the existing persisted values for columns the batch
//! does not touch) and for loading [`PersistedRegistryRow`] when the change
//! is an UPDATE.

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};

use crate::crdt::column_sig::register_lookup::canonicalize_row_pks;
use crate::ucan::verify::public_key_from_did;

use super::payload::RegistryRowSigPayload;
use super::verify::{verify_registry_row, VerifyRegistryRowSigError};

/// A `haex_shared_space_sync` registry row as it will exist after applying
/// one incoming change — the same 12-field + `row_sig` shape
/// `sign_registry_row_self` (Task B.3) reads back after a local write.
/// Building this from the puller's per-column change batch (merging touched
/// columns with the row's persisted state) is Task B.5's job; this struct is
/// this module's input contract, not a wire/IPC type.
#[derive(Debug, Clone)]
pub struct IncomingRegistryChange {
    pub id: String,
    pub space_id: String,
    pub table_name: String,
    pub row_pks: String,
    pub extension_public_key: Option<String>,
    pub extension_name: Option<String>,
    pub category: Option<String>,
    pub r#type: Option<String>,
    pub category_label: Option<String>,
    pub type_label: Option<String>,
    pub authored_by_did: String,
    pub created_at: String,
    /// Base64-encoded Ed25519 signature, or `""` for a pre-migration-0014
    /// row (the DB default). Always rejected here — see
    /// [`RegistryVerifyError::RowSigMissingOrEmpty`] — the graceful skip
    /// B.3 applies for local pre-migration fixtures does not extend to
    /// changes arriving from a peer.
    pub row_sig: String,
}

/// The locally persisted row's immutable authorship anchor, fetched by the
/// caller for an UPDATE. Absent (`None` at the call site) for an INSERT —
/// there is no existing row to compare against.
#[derive(Debug, Clone)]
pub struct PersistedRegistryRow {
    pub authored_by_did: String,
}

/// Reasons an incoming registry-row change is dropped.
///
/// Extension-manifest findings (`ManifestTableNotShareable`,
/// `ExtensionVersionIncompatible`, `ExtensionNotInstalled`) are Task E.3's
/// concern and deliberately absent here.
#[derive(Debug, PartialEq, Eq)]
pub enum RegistryVerifyError {
    /// `row_sig` failed Ed25519 verification against the reconstructed
    /// canonical payload (wrong key, tampered field, or malformed signature
    /// bytes/encoding).
    SignatureInvalid(VerifyRegistryRowSigError),
    /// An UPDATE claims a different `authored_by_did` than the row's
    /// existing value — authorship is immutable post-creation.
    AuthoredByDidImmutable { existing: String, incoming: String },
    /// `authored_by_did` is not a well-formed `did:key:...` string, so no
    /// Ed25519 public key could be derived from it.
    UnknownAuthorDid(String),
    /// `row_sig` is empty — either a pre-migration-0014 row or a hostile
    /// peer's forgery/replay attempt.
    RowSigMissingOrEmpty,
}

/// Verify one incoming registry-row change (Task B.4).
///
/// `existing_row` is `Some` for an UPDATE (the row already exists locally)
/// and `None` for an INSERT. Checks run in this order:
///
///   1. **Immutability** — only when `existing_row` is `Some`. Runs before
///      the signature check: a forged `authored_by_did` on an UPDATE can
///      still carry the row's ORIGINAL, validly-signed `row_sig` (an
///      attacker who does not also hold the space's signing key cannot
///      re-sign), so immutability must catch the identity swap directly
///      rather than rely on the signature check to incidentally fail too.
///   2. **`row_sig` presence** (Concern 1).
///   3. **`authored_by_did` → Ed25519 public key** resolution.
///   4. **`row_sig` base64 decoding.**
///   5. **`row_pks` canonicalization** (Concern 3 — mirrors the exact
///      normalization B.3 applies before signing, via the same
///      [`canonicalize_row_pks`] helper, so a peer's differently-key-ordered
///      JSON does not spuriously fail verification) followed by Ed25519
///      signature verification of the reconstructed payload.
pub fn verify_incoming_registry_change(
    change: &IncomingRegistryChange,
    existing_row: Option<&PersistedRegistryRow>,
) -> Result<(), RegistryVerifyError> {
    if let Some(existing) = existing_row {
        if existing.authored_by_did != change.authored_by_did {
            return Err(RegistryVerifyError::AuthoredByDidImmutable {
                existing: existing.authored_by_did.clone(),
                incoming: change.authored_by_did.clone(),
            });
        }
    }

    if change.row_sig.is_empty() {
        return Err(RegistryVerifyError::RowSigMissingOrEmpty);
    }

    let pk = public_key_from_did(&change.authored_by_did)
        .map_err(|_| RegistryVerifyError::UnknownAuthorDid(change.authored_by_did.clone()))?;

    let sig_bytes = BASE64.decode(&change.row_sig).map_err(|_| {
        RegistryVerifyError::SignatureInvalid(VerifyRegistryRowSigError::MalformedSignatureBytes)
    })?;

    // Concern 3: canonicalize before rebuilding the payload — a peer's wire
    // encoding may not match the sorted-key form the signer used. A
    // `row_pks` blob that does not even parse as JSON cannot have been part
    // of any payload a legitimate signer produced, so it folds into the
    // same signature-invalid outcome callers already handle (design
    // decision mirrored from the malformed-base64 case: callers just log).
    let canonical_row_pks = canonicalize_row_pks(&change.row_pks).map_err(|_| {
        RegistryVerifyError::SignatureInvalid(VerifyRegistryRowSigError::InvalidSignature)
    })?;

    let payload = RegistryRowSigPayload {
        id: &change.id,
        space_id: &change.space_id,
        table_name: &change.table_name,
        row_pks: &canonical_row_pks,
        extension_public_key: change.extension_public_key.as_deref(),
        extension_name: change.extension_name.as_deref(),
        category: change.category.as_deref(),
        r#type: change.r#type.as_deref(),
        category_label: change.category_label.as_deref(),
        type_label: change.type_label.as_deref(),
        authored_by_did: &change.authored_by_did,
        created_at: &change.created_at,
    };

    verify_registry_row(&payload, &sig_bytes, &pk).map_err(RegistryVerifyError::SignatureInvalid)
}
