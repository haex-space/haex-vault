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
//! - **Self-removal recorded but unenforced**: this phase does not check the
//!   committer's own capability, so a self-leave is trivially allowed. The
//!   flag is surfaced so a later phase that adds a committer-capability
//!   check can grant the standard "leave is always allowed" exemption.
//!
//! **Not** in Phase 1 (deferred to Phase 2/3 of the plan):
//!
//! - Committer authorization (needs `CapabilityLevel::Invite`-or-higher on
//!   the committer). Without this, a read-only member can still legitimately
//!   remove another member, which Phase 1 does not stop.
//! - Proof-of-possession as a KeyPackage extension. Without it the addee
//!   membership check only raises the bar from "add any stranger" to
//!   "impersonate an existing member's DID". Phase 2 closes that.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use openmls::credentials::{Credential, CredentialType};
use openmls::framing::Sender;
use openmls::group::{MlsGroup, StagedCommit};
use openmls::messages::proposals::{Proposal, ProposalType};
use rusqlite::Connection;

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
    /// we expect). Surfaced for Phase-3 committer-capability checks; Phase-1
    /// does not gate removes.
    #[allow(dead_code)]
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
    /// Recorded but not consumed by Phase-1 policy; the Phase-3 committer-
    /// capability check will use these to enforce "read-only members cannot
    /// remove anyone else".
    #[allow(dead_code)]
    pub removes: Vec<RemoveFact>,
    pub updates: Vec<UpdateFact>,
    /// For an external commit (`Sender::NewMemberCommit`), the joiner's DID
    /// from their own leaf credential. Checked with the same membership rule
    /// as an Add proposal — the joiner is essentially "adding themselves".
    pub external_joiner: Option<String>,
    /// Descriptions of any queued proposal whose type this module does not
    /// model. Non-empty means the commit is rejected fail-closed.
    pub unmodelled: Vec<&'static str>,
    /// True if the committer removed their own leaf (a leave). Not consumed
    /// by Phase-1 policy; surfaced so a later committer-capability check can
    /// grant the standard leave exemption.
    #[allow(dead_code)]
    pub self_removal: bool,
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

    let self_removal = committer_leaf
        .map(|c| removes.iter().any(|r| r.leaf_index == c))
        .unwrap_or(false);

    CommitFacts {
        adds,
        removes,
        updates,
        external_joiner,
        unmodelled,
        self_removal,
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
