//! Phase-1 authorization for incoming MLS commits.
//!
//! MLS (RFC 9420) provides cryptographic group agreement but no role model —
//! any group member can propose any commit and OpenMLS will happily merge it
//! as long as the crypto verifies. Without an application-policy layer that
//! means:
//!
//! - a legitimate member can add an arbitrary outsider (silent eavesdropper),
//! - a legitimate member can remove any other member (including the owner),
//! - a legitimate member can rotate their leaf into a different DID.
//!
//! This module runs between `MlsGroup::process_message` and
//! `merge_staged_commit` (see [`crate::mls::manager::MlsManager::decrypt`])
//! and rejects commits that violate the space's application-level rules.
//! Rejecting means the local group does not advance to the new epoch; the
//! caller falls back to epoch-gap recovery. This is a fail-closed check.
//!
//! Phase-1 policy:
//!
//! - **Addee membership**: every Add proposal's credential DID must already
//!   be a member of the space (`haex_space_members` ⋈ `haex_identities`).
//!   External-commit joiners are checked with the same rule.
//! - **Credential stability**: an Update (whether an inline proposal or a
//!   path-in-commit leaf rotation) must not change the DID at the target
//!   leaf.
//! - **Fail-closed on unmodelled proposals**: any proposal type this module
//!   does not model (PSK, ReInit, GroupContextExtensions, SelfRemove,
//!   Custom, …) rejects the whole commit.
//!
//! Phase-2 additionally verifies the proof-of-possession KeyPackage
//! extension on every Add proposal (see [`verify_pops`]).
//!
//! Phase-3 additionally requires the committer to hold
//! `CapabilityLevel::Invite`-or-higher before a membership-changing commit
//! (Add or Remove) may merge, with a self-leave exemption (see
//! [`authorize_committer_capability`]). This is the interim rule from §5.7
//! of the plan; a dedicated orthogonal `Cap::ManageMembers` is deferred to
//! the `CapabilitySet` migration (Phase 4).
//!
//! **Not yet closed:**
//!
//! - External-commit joiners are not PoP-verified (openmls 0.8.1 limitation,
//!   documented on [`verify_pops`]).
//! - The interim `Invite`-or-higher rule is a blunt hierarchical gate, not
//!   the orthogonal "may remove but not invite" / "may invite but not kick"
//!   split the plan's final design calls for.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use openmls::credentials::{Credential, CredentialType};
use openmls::framing::Sender;
use openmls::group::{MlsGroup, StagedCommit};
use openmls::messages::proposals::{Proposal, ProposalType};
use rusqlite::Connection;

use crate::ucan::CapabilityLevel;

/// One entry per Add proposal in the commit.
#[derive(Debug, Clone)]
pub(crate) struct AddFact {
    /// DID string extracted from the KeyPackage's BasicCredential.
    pub credential_did: String,
    /// The 32-byte Ed25519 MLS signature key on the KP's leaf. Feeds
    /// [`verify_pops`] together with `pop_bytes`. Zeroed when the facts
    /// are synthesized in unit tests that do not exercise PoP.
    #[allow(dead_code)]
    pub mls_sig_pub: [u8; 32],
    /// The 64-byte Ed25519 PoP signature extracted from the KP's leaf
    /// extension [`crate::mls::pop::HAEX_POP_EXTENSION_TYPE`]. `None` if
    /// the extension was absent or malformed — [`verify_pops`] treats
    /// that as reject-closed.
    #[allow(dead_code)]
    pub pop_bytes: Option<Vec<u8>>,
}

/// One entry per Remove proposal in the commit.
#[derive(Debug, Clone)]
pub(crate) struct RemoveFact {
    /// Leaf index the Remove targets.
    pub leaf_index: u32,
    /// DID at that leaf in the pre-commit tree; `None` if the leaf slot was
    /// already empty (defensive — a Remove on an empty slot is not something
    /// we expect). Feeds [`CommitFacts::self_removal`] via DID comparison
    /// against the committer — necessary because a rejoining external-commit
    /// joiner cleaning up their own stale leaf has no *pre-commit* leaf of
    /// their own to compare indices against (`Sender::NewMemberCommit` has
    /// no meaningful "own leaf" before the commit); comparing identities
    /// instead of leaf positions covers that case correctly.
    pub credential_did: Option<String>,
}

/// One entry per Update proposal (inline) or path-in-commit leaf rotation.
#[derive(Debug, Clone)]
pub(crate) struct UpdateFact {
    /// Leaf index the update rotates.
    pub leaf_index: u32,
    /// DID at that leaf before the commit is applied.
    pub old_did: Option<String>,
    /// DID the new leaf's credential carries.
    pub new_did: String,
}

/// Facts extracted from a `StagedCommit` for the policy layer to decide on.
#[derive(Debug, Clone, Default)]
pub(crate) struct CommitFacts {
    pub adds: Vec<AddFact>,
    pub removes: Vec<RemoveFact>,
    pub updates: Vec<UpdateFact>,
    /// For an external commit (`Sender::NewMemberCommit`), the joiner's DID
    /// from their own leaf credential. Checked with the same membership rule
    /// as an Add proposal — the joiner is essentially "adding themselves".
    pub external_joiner: Option<String>,
    /// Descriptions of any queued proposal whose type this module does not
    /// model. Non-empty means the commit is rejected fail-closed.
    pub unmodelled: Vec<&'static str>,
    /// True if the committer removed their own leaf (a leave). Consumed by
    /// [`authorize_committer_capability`] to grant the leave exemption —
    /// only when it's the ONLY membership change in the commit (see there).
    pub self_removal: bool,
    /// DID resolved from the committer's own credential (`Sender::Member`'s
    /// own leaf, or the joiner's credential for `Sender::NewMemberCommit`).
    /// `None` for the anomalous sender variants, which are already rejected
    /// via `unmodelled` before this would matter. Feeds the capability
    /// lookup in [`authorize_committer_capability`].
    pub committer_did: Option<String>,
}

/// Extract the DID string from a BasicCredential.
///
/// The vault sets `BasicCredential::new(did.as_bytes().to_vec())` in
/// [`crate::mls::manager::MlsManager::init_identity`], so
/// `serialized_content()` on such a credential is the raw DID bytes. Any
/// non-Basic credential (X.509, Custom, …) has a different serialisation
/// shape and returns `None`; callers treat that as "no DID" and the
/// addee/joiner check then fails fail-closed. Non-UTF8 Basic content also
/// returns `None`.
fn did_from_credential(cred: &Credential) -> Option<String> {
    if cred.credential_type() != CredentialType::Basic {
        return None;
    }
    String::from_utf8(cred.serialized_content().to_vec()).ok()
}

/// Inspect a staged commit against the current group state.
///
/// Must be called AFTER `process_message` (which produced `staged`) but
/// BEFORE `merge_staged_commit` — the pre-commit leaf → DID mapping we
/// snapshot here would otherwise reflect the new epoch.
pub(crate) fn inspect(
    sender: &Sender,
    committer_credential: &Credential,
    staged: &StagedCommit,
    group: &MlsGroup,
) -> CommitFacts {
    // Snapshot the current tree's leaf → DID map. Used for:
    //   - Remove target lookup (§ RemoveFact.credential_did)
    //   - Update old-DID lookup (§ UpdateFact.old_did)
    let leaf_to_did: HashMap<u32, String> = group
        .members()
        .filter_map(|m| did_from_credential(&m.credential).map(|d| (m.index.u32(), d)))
        .collect();

    // Unmodelled proposals AND unexpected sender variants both funnel into
    // the same fail-closed collection. `authorize` rejects the commit if this
    // is non-empty at the end of `inspect`.
    let mut unmodelled: Vec<&'static str> = Vec::new();

    // Committer's own leaf index and the external-commit joiner's DID (only
    // one of the two applies).
    let (committer_leaf, external_joiner) = match sender {
        Sender::Member(idx) => (Some(idx.u32()), None),
        Sender::NewMemberCommit => (None, did_from_credential(committer_credential)),
        // External senders and NewMemberProposal are not expected on a commit
        // path per RFC 9420; record the anomaly so `authorize` rejects the
        // whole commit rather than silently accepting when no adds/updates
        // happen to be present.
        Sender::NewMemberProposal => {
            unmodelled.push("UnexpectedSender:NewMemberProposal");
            (None, None)
        }
        Sender::External(_) => {
            unmodelled.push("UnexpectedSender:External");
            (None, None)
        }
    };

    let mut adds: Vec<AddFact> = Vec::new();
    for add in staged.add_proposals() {
        let leaf = add.add_proposal().key_package().leaf_node();
        let did = did_from_credential(leaf.credential()).unwrap_or_default();
        let mls_sig_pub = {
            let slice = leaf.signature_key().as_slice();
            let mut buf = [0u8; 32];
            if slice.len() == 32 {
                buf.copy_from_slice(slice);
            }
            // Length !=32 → leave zeroed; `verify_pops` will reject when
            // the resulting key does not verify the PoP.
            buf
        };
        let pop_bytes = crate::mls::pop::extract_pop_from_leaf(leaf).map(|s| s.to_bytes().to_vec());
        adds.push(AddFact {
            credential_did: did,
            mls_sig_pub,
            pop_bytes,
        });
    }

    let mut removes: Vec<RemoveFact> = Vec::new();
    for rem in staged.remove_proposals() {
        let idx = rem.remove_proposal().removed().u32();
        removes.push(RemoveFact {
            leaf_index: idx,
            credential_did: leaf_to_did.get(&idx).cloned(),
        });
    }

    let mut updates: Vec<UpdateFact> = Vec::new();
    // Update proposals carry credential-stability semantics: the update
    // targets the PROPOSER's own leaf, which is not necessarily the
    // committer. `QueuedUpdateProposal::sender()` gives the proposer's
    // sender; only `Sender::Member(idx)` is meaningful here, so we attribute
    // the update to that leaf and record the anomaly for anything else.
    for upd in staged.update_proposals() {
        let new_did =
            did_from_credential(upd.update_proposal().leaf_node().credential()).unwrap_or_default();
        match upd.sender() {
            Sender::Member(idx) => {
                let leaf = idx.u32();
                updates.push(UpdateFact {
                    leaf_index: leaf,
                    old_did: leaf_to_did.get(&leaf).cloned(),
                    new_did,
                });
            }
            _ => unmodelled.push("UpdateProposalFromNonMemberSender"),
        }
    }
    // The path-in-commit leaf update is emitted by the committer and rotates
    // the committer's own leaf. For a `Sender::NewMemberCommit` the path
    // leaf is the joiner's OWN new leaf — that DID is already checked via
    // `external_joiner`. Skipping it here avoids a spurious credential-
    // stability reject against an empty pre-commit slot.
    if let Some(committer_slot) = committer_leaf {
        if let Some(leaf) = staged.update_path_leaf_node() {
            let new_did = did_from_credential(leaf.credential()).unwrap_or_default();
            updates.push(UpdateFact {
                leaf_index: committer_slot,
                old_did: leaf_to_did.get(&committer_slot).cloned(),
                new_did,
            });
        }
    }

    // Sweep every queued proposal so we don't miss a type openmls exposes
    // that isn't in the four typed iterators.
    //
    // Add / Remove / Update: handled above.
    //
    // ExternalInit: RFC 9420 §12.2 — the joining member's own initialisation
    // signal inside an external commit. Only valid when the sender is
    // `NewMemberCommit`; the joiner is already checked via `external_joiner`.
    // Reject it if it appears elsewhere.
    //
    // Everything else — PSK, ReInit, GroupContextExtensions, SelfRemove,
    // Custom, … — is fail-closed.
    let sender_is_new_member = matches!(sender, Sender::NewMemberCommit);
    for qp in staged.queued_proposals() {
        let ty = qp.proposal().proposal_type();
        match ty {
            ProposalType::Add | ProposalType::Remove | ProposalType::Update => {}
            ProposalType::ExternalInit if sender_is_new_member => {}
            _ => unmodelled.push(name_of(&ty)),
        }
    }

    // The committer's own DID, regardless of sender variant. For the
    // anomalous senders (External/NewMemberProposal) this is whatever
    // openmls populated `committer_credential` with — irrelevant in
    // practice since `unmodelled` already rejects those before this field
    // would be consulted.
    let committer_did = did_from_credential(committer_credential);

    // Identity-based, NOT leaf-index-based: a `Sender::Member` self-leave
    // removes the leaf holding the committer's own DID, which
    // `leaf_to_did`-derived `RemoveFact::credential_did` already captures.
    // A `Sender::NewMemberCommit` rejoin can ALSO carry a Remove — openmls
    // auto-generates one to clean up the rejoiner's own stale prior leaf
    // when they reuse the same MLS signature key — and that remove targets
    // the joiner's own (pre-commit) leaf too, identified by the SAME DID.
    // `committer_leaf` is `None` for `NewMemberCommit`, so a leaf-index
    // comparison would miss this; comparing DIDs catches both shapes.
    let self_removal = committer_did
        .as_deref()
        .map(|did| {
            removes
                .iter()
                .any(|r| r.credential_did.as_deref() == Some(did))
        })
        .unwrap_or(false);

    CommitFacts {
        adds,
        removes,
        updates,
        external_joiner,
        unmodelled,
        self_removal,
        committer_did,
    }
}

fn name_of(t: &ProposalType) -> &'static str {
    match t {
        ProposalType::Add => "Add",
        ProposalType::Update => "Update",
        ProposalType::Remove => "Remove",
        ProposalType::PreSharedKey => "PreSharedKey",
        ProposalType::Reinit => "ReInit",
        ProposalType::ExternalInit => "ExternalInit",
        ProposalType::GroupContextExtensions => "GroupContextExtensions",
        ProposalType::SelfRemove => "SelfRemove",
        ProposalType::Custom(_) => "Custom",
        _ => "Unknown",
    }
}

/// Apply the Phase-1 policy to `facts`. Returns `Ok(())` if the commit may
/// be merged, `Err(reason)` if it must be rejected without advancing the
/// epoch. Any error condition (mutex poisoning, absent connection, SQL
/// failure) is treated as reject — this is a fail-closed check.
pub(crate) fn authorize(
    conn: &Arc<Mutex<Option<Connection>>>,
    space_id: &str,
    facts: &CommitFacts,
) -> Result<(), String> {
    if !facts.unmodelled.is_empty() {
        return Err(format!(
            "Rejecting MLS commit for space {space_id}: unmodelled proposal type(s) {:?} — Phase-1 authorization is fail-closed",
            facts.unmodelled
        ));
    }

    for u in &facts.updates {
        match &u.old_did {
            Some(old) if old != &u.new_did => {
                return Err(format!(
                    "Rejecting MLS commit for space {space_id}: leaf {} DID changed from {old} to {} (credential-stability violation)",
                    u.leaf_index, u.new_did
                ));
            }
            None => {
                return Err(format!(
                    "Rejecting MLS commit for space {space_id}: Update at leaf {} has no prior credential (unexpected shape)",
                    u.leaf_index
                ));
            }
            Some(_) => {}
        }
    }

    let mut addees: Vec<&str> = facts
        .adds
        .iter()
        .map(|a| a.credential_did.as_str())
        .collect();
    if let Some(j) = &facts.external_joiner {
        addees.push(j.as_str());
    }

    if !addees.is_empty() {
        let guard = conn
            .lock()
            .map_err(|e| format!("Authorization mutex poisoned: {e}"))?;
        let conn_ref = guard.as_ref().ok_or_else(|| {
            "No database connection available for MLS commit authorization".to_string()
        })?;
        for did in addees {
            if did.is_empty() {
                return Err(format!(
                    "Rejecting MLS commit for space {space_id}: addee credential is empty or non-UTF8"
                ));
            }
            if !is_space_member(conn_ref, space_id, did)? {
                return Err(format!(
                    "Rejecting MLS commit for space {space_id}: addee {did} is not a member of this space (haex_space_members ⋈ haex_identities)"
                ));
            }
        }
    }

    Ok(())
}

/// Phase-2 check: every Add proposal's KeyPackage MUST carry a
/// proof-of-possession leaf extension that binds the leaf's MLS signature
/// key to the credential DID's identity key. Runs AFTER
/// [`authorize`] (which handles Phase-1 checks); rejecting here means the
/// staged commit is not merged.
///
/// External-commit joiners are NOT checked here — openmls 0.8.1's external
/// commit builder does not expose leaf-node extensions, so the joining
/// leaf carries none, and we cannot embed a PoP on it. Phase-1's addee
/// membership check still fires against `haex_space_members`, which stops
/// "join as a total stranger" but not "impersonate an existing member's
/// DID by putting it in the external-commit credential". Closing that
/// requires either a vendored openmls patch or an upstream feature — filed
/// as a follow-up.
pub(crate) fn verify_pops(space_id: &str, facts: &CommitFacts) -> Result<(), String> {
    for add in &facts.adds {
        let pop_bytes = add.pop_bytes.as_deref().ok_or_else(|| {
            format!(
                "Rejecting MLS commit for space {space_id}: addee {} — KeyPackage \
                 is missing the required proof-of-possession leaf extension",
                add.credential_did
            )
        })?;
        let sig = ed25519_dalek::Signature::try_from(pop_bytes).map_err(|e| {
            format!(
                "Rejecting MLS commit for space {space_id}: addee {} — malformed PoP \
                 signature bytes: {e}",
                add.credential_did
            )
        })?;
        let identity_pub = crate::ucan::public_key_from_did(&add.credential_did).map_err(|e| {
            format!(
                "Rejecting MLS commit for space {space_id}: addee {} — cannot resolve \
                     identity key from DID: {e}",
                add.credential_did
            )
        })?;
        crate::mls::pop::verify_pop(&identity_pub, &add.mls_sig_pub, &add.credential_did, &sig)
            .map_err(|e| {
                format!(
                    "Rejecting MLS commit for space {space_id}: addee {} — PoP does not \
                     verify against the credential-DID identity key: {e}",
                    add.credential_did
                )
            })?;
    }
    Ok(())
}

/// Phase-3 check: a membership-changing commit (at least one Add or Remove
/// proposal) requires the committer to hold `CapabilityLevel::Invite`-or-
/// higher for this space. This is the interim rule from §5.7 of the plan —
/// no dedicated `Cap::ManageMembers`, just the existing hierarchical
/// lattice (`Admin > Invite > Write > Read`).
///
/// Exemptions:
/// - No Add/Remove at all (key rotation, PSK, ordinary application
///   traffic) — no capability requirement.
/// - A member removing ONLY themselves — exactly one Remove, targeting
///   their own leaf (`CommitFacts::self_removal`), and no Adds — must
///   always be allowed. Leaving a space can never require a capability the
///   leaver may not hold; if a self-remove is bundled with anything else
///   (another Remove, or an Add), the capability requirement still applies
///   to the commit as a whole.
///
/// Trust model: like [`authorize`]'s addee-membership check, this reads
/// `haex_ucan_tokens`, a CRDT-synced table — as trustworthy as the write-
/// side row-authorization that guards it (see the W4 `CapabilitySet` /
/// `row_cap` work), not re-verified here. This mirrors the trust boundary
/// `crate::space_delivery::local::ucan::load_active_ucan_for_audience`
/// already relies on.
pub(crate) fn authorize_committer_capability(
    conn: &Arc<Mutex<Option<Connection>>>,
    space_id: &str,
    facts: &CommitFacts,
) -> Result<(), String> {
    let membership_changing = !facts.adds.is_empty() || !facts.removes.is_empty();
    if !membership_changing {
        return Ok(());
    }

    let is_pure_self_leave =
        facts.adds.is_empty() && facts.self_removal && facts.removes.len() == 1;
    if is_pure_self_leave {
        return Ok(());
    }

    let committer_did = facts.committer_did.as_deref().ok_or_else(|| {
        format!(
            "Rejecting MLS commit for space {space_id}: membership-changing commit has no \
             resolvable committer DID"
        )
    })?;

    let guard = conn
        .lock()
        .map_err(|e| format!("Authorization mutex poisoned: {e}"))?;
    let conn_ref = guard.as_ref().ok_or_else(|| {
        "No database connection available for MLS commit authorization".to_string()
    })?;

    match committer_capability(conn_ref, space_id, committer_did)? {
        Some(level) if level.allows(&CapabilityLevel::Invite) => Ok(()),
        Some(level) => Err(format!(
            "Rejecting MLS commit for space {space_id}: committer {committer_did} holds \
             {level:?} but membership changes require Invite-or-higher"
        )),
        None => Err(format!(
            "Rejecting MLS commit for space {space_id}: committer {committer_did} holds no \
             capability for this space"
        )),
    }
}

/// Highest-ranked, non-expired `CapabilityLevel` held by `did` in
/// `space_id`, or `None` if none. Mirrors
/// `crate::space_delivery::local::ucan::load_active_ucan_for_audience`'s
/// query shape (same table, same `expires_at` filter) but returns the
/// parsed level directly since the caller only needs the rank, not the
/// token string.
fn committer_capability(
    conn: &Connection,
    space_id: &str,
    did: &str,
) -> Result<Option<CapabilityLevel>, String> {
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let mut stmt = conn
        .prepare(
            "SELECT capability FROM haex_ucan_tokens \
             WHERE space_id = ?1 AND audience_did = ?2 AND expires_at > ?3",
        )
        .map_err(|e| format!("Failed to prepare capability lookup: {e}"))?;

    let rows = stmt
        .query_map(rusqlite::params![space_id, did, now_secs], |row| {
            row.get::<_, String>(0)
        })
        .map_err(|e| format!("Capability lookup failed for space={space_id} did={did}: {e}"))?;

    let mut best: Option<CapabilityLevel> = None;
    for row in rows {
        let capability_str = row.map_err(|e| format!("Failed to read capability row: {e}"))?;
        if let Some(level) = CapabilityLevel::from_capability_string(&capability_str) {
            best = Some(match best {
                Some(current) if current >= level => current,
                _ => level,
            });
        }
    }
    Ok(best)
}

fn is_space_member(conn: &Connection, space_id: &str, did: &str) -> Result<bool, String> {
    // Post delete-log refactor there is no `haex_tombstone` column on
    // `haex_space_members` / `haex_identities`; revocation is expressed by
    // row absence (the delete-log apply path removes the row). See
    // `crate::crdt::transformer` where the tombstone filter was removed on
    // main-table selects.
    conn.query_row(
        "SELECT COUNT(*) FROM haex_space_members m \
         JOIN haex_identities i ON m.identity_id = i.id \
         WHERE m.space_id = ?1 AND i.did = ?2",
        rusqlite::params![space_id, did],
        |row| row.get::<_, i64>(0),
    )
    .map(|c| c > 0)
    .map_err(|e| format!("Membership lookup failed for space={space_id} did={did}: {e}"))
}

#[cfg(test)]
#[path = "authorization_tests.rs"]
mod tests;
