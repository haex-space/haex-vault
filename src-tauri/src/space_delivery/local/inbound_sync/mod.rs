//! Inbound-SyncPush validation, authorisation and origin attribution.
//!
//! The leader receives a batch of column-level CRDT changes from a peer,
//! already authenticated by UCAN signature. This module is the **single
//! choke point** that decides whether such a batch may be applied to the
//! space DB. Every inbound CRDT push MUST flow through
//! [`authorize_inbound_sync_push`] — handlers should never call the
//! lower-level checks individually, otherwise the security model decays
//! into "everyone enforces their own subset".
//!
//! The pipeline (in this exact order — each step's success is a
//! precondition for the next):
//!
//! 1. **Capability gate.** The minimum capability is determined from the
//!    set of tables touched. Pushes that touch only the
//!    [membership-system tables][`crate::crdt::scanner::MEMBERSHIP_SYSTEM_TABLES`]
//!    require `Read`; any other table requires `Write`. This lets a
//!    read-only member publish their own membership / device / MLS
//!    KeyPackage rows while still blocking attempts to write user content
//!    like `haex_peer_shares`.
//! 2. **Membership gate.** The UCAN audience must still be an active
//!    (non-tombstoned) member of the space — admin removal is the
//!    revocation kill-switch.
//! 3. **Payload validation** (pure transform — see [`validate`]):
//!    - **Table whitelist.** Only rows for tables in
//!      [`crate::crdt::scanner::SPACE_SCOPED_CRDT_TABLES`] may cross the
//!      wire.
//!    - **`space_id` column scope.** Any change that writes the
//!      `space_id` column must set it to the request's `space_id`;
//!      anything else is a cross-space injection attempt.
//!    - **Origin attribution.** The client's claim about `authored_by_did`
//!      is *stripped* from the batch; the leader re-injects exactly one
//!      `authored_by_did` column-change per unique `(table, row)` with
//!      the value taken from the validated UCAN audience. A peer cannot
//!      forge authorship because the field is never read from the wire.
//! 4. **Per-row space scope** (see [`space_scope`]). The column-level
//!    `space_id` check in step 3 only fires when the payload actually
//!    writes the `space_id` column. For updates that target an existing
//!    row by PK alone, this gate reads the row's current `space_id` from
//!    the DB and rejects the batch if it does not match the request's
//!    `space_id`. Closes the multi-space-member attack surface where a
//!    member of spaces A and B mutates a B-row through
//!    `SyncPush { space_id: "A" }` by omitting the column.
//! 5. **Per-row ownership** (see [`ownership`]). For the membership-system
//!    tables that any member may write, every modified row must already
//!    belong to the caller (or be a brand-new row whose declared owner is
//!    the caller). This stops Mallory from overwriting Bob's membership
//!    with `identity_id = mallory` or hijacking Bob's device endpoint
//!    registration.

pub mod ownership;
pub mod space_scope;
pub mod validate;
mod util;

use crate::crdt::scanner::{is_membership_system_table, LocalColumnChange};
use crate::database::DbConnection;
use crate::ucan::{require_capability, CapabilityLevel, ValidatedUcan};

use super::ucan::is_active_space_member;

pub use ownership::enforce_row_ownership;
pub use space_scope::enforce_row_space_scope;
pub use validate::validate_and_attribute;

/// The outcome of validating and attributing an inbound SyncPush batch.
#[derive(Debug)]
pub enum InboundSyncPushOutcome {
    /// All checks passed; `changes` has been stripped of client-supplied
    /// `authored_by_did` entries and one authoritative attribution per
    /// unique row has been injected.
    Accepted { changes: Vec<LocalColumnChange> },
    /// The batch was rejected; the reason is suitable for logging and
    /// returning to the peer as an error response.
    Rejected { reason: String },
}

/// Decide which capability level a push needs given the tables it touches.
/// A push that touches only [membership-system tables] is allowed for any
/// member (Read suffices). Any other table requires Write.
///
/// [membership-system tables]: crate::crdt::scanner::MEMBERSHIP_SYSTEM_TABLES
fn required_capability_for(changes: &[LocalColumnChange]) -> CapabilityLevel {
    if changes.iter().all(|c| is_membership_system_table(&c.table_name)) {
        CapabilityLevel::Read
    } else {
        CapabilityLevel::Write
    }
}

/// **The single authorisation entry point** for inbound CRDT pushes from
/// space peers. Every code path that wants to apply remote
/// `LocalColumnChange`s to the local DB MUST call this function and act
/// only on `Accepted { changes }`.
///
/// Pipeline (each step's success is a precondition for the next):
///
/// 1. Capability gate (per change-set, see [`required_capability_for`])
/// 2. Active-membership gate ([`is_active_space_member`])
/// 3. Payload validation + origin attribution
///    ([`validate_and_attribute`])
/// 4. Per-row space scope ([`enforce_row_space_scope`])
/// 5. Per-row ownership for membership-system tables
///    ([`enforce_row_ownership`])
///
/// On success, the returned `Accepted` value carries the *sanitised*
/// change set: the client-supplied `authored_by_did` claims have been
/// replaced by leader-injected ones derived from the UCAN audience.
pub fn authorize_inbound_sync_push(
    db: &DbConnection,
    space_id: &str,
    peer_endpoint_id: &str,
    validated_ucan: &ValidatedUcan,
    raw_changes: Vec<LocalColumnChange>,
) -> InboundSyncPushOutcome {
    if raw_changes.is_empty() {
        return InboundSyncPushOutcome::Accepted { changes: vec![] };
    }

    // (1) Capability gate
    let required = required_capability_for(&raw_changes);
    if let Err(e) = require_capability(validated_ucan, space_id, required) {
        // Surface the table(s) that triggered the higher-than-Read capability
        // requirement so operators can see *why* a push was rejected. Without
        // this, the only signal is "need Write, have Read" — which leaves
        // unanswered whether the pusher meant to push user content (legit
        // Write attempt) or whether a ping-pong re-push smuggled
        // `haex_peer_shares` into a Read member's batch (the actual bug).
        let mut offending: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
        for change in &raw_changes {
            if !is_membership_system_table(&change.table_name) {
                offending.insert(change.table_name.as_str());
            }
        }
        eprintln!(
            "[InboundSync] Capability reject: space={space_id} \
             audience={} required={required:?} offending_tables={:?} batch_size={}",
            &validated_ucan.audience[..24.min(validated_ucan.audience.len())],
            offending,
            raw_changes.len(),
        );
        return InboundSyncPushOutcome::Rejected {
            reason: format!("Access denied: {e}"),
        };
    }

    // (2) Active-membership gate
    match is_active_space_member(db, space_id, &validated_ucan.audience) {
        Ok(true) => {}
        Ok(false) => {
            return InboundSyncPushOutcome::Rejected {
                reason: "Access denied: not an active member of this space".to_string(),
            };
        }
        Err(e) => {
            return InboundSyncPushOutcome::Rejected {
                reason: format!("Membership check failed: {e}"),
            };
        }
    }

    // (3) Payload validation + origin attribution
    let attributed = match validate_and_attribute(space_id, &validated_ucan.audience, raw_changes) {
        InboundSyncPushOutcome::Accepted { changes } => changes,
        rejected @ InboundSyncPushOutcome::Rejected { .. } => return rejected,
    };

    // (4) Per-row space scope — the existing row's space_id must match
    // request_space_id, and inserts must declare space_id. Without this
    // gate, a multi-space member could rewrite a foreign-space row by
    // omitting the space_id column from the change set.
    if let Err(reason) = enforce_row_space_scope(db, space_id, &attributed) {
        return InboundSyncPushOutcome::Rejected {
            reason: format!("Cross-space row violation: {reason}"),
        };
    }

    // (5) Per-row ownership for membership-system tables
    if let Err(reason) =
        enforce_row_ownership(db, peer_endpoint_id, &validated_ucan.audience, &attributed)
    {
        return InboundSyncPushOutcome::Rejected {
            reason: format!("Row ownership violation: {reason}"),
        };
    }

    InboundSyncPushOutcome::Accepted { changes: attributed }
}
