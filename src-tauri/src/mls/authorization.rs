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
//! [`Cap::Invite`] or [`Cap::Admin`] before a membership-changing commit
//! may merge, with a self-leave exemption (see
//! [`authorize_committer_capability`]). This is the interim rule from §5.7
//! of the plan; a dedicated orthogonal `Cap::ManageMembers` is deferred to
//! the `CapabilitySet` migration (Phase 4).
//!
//! [`authorize_local_removal`] extends the Phase-3 gate to commits **we
//! originate** locally (`MlsManager::remove_member`) — those never pass
//! through `decrypt`/`inspect` at all, so nothing gated them until this was
//! added (a CodeRabbit finding on PR #781). See that function's docs for
//! why `MlsManager::add_member` is deliberately NOT gated the same way.
//!
//! **§5.8 follow-up (UCAN-on-commit) — Removes only.** The receive-side gate
//! in [`authorize_committer_capability`] no longer reads `haex_ucan_tokens`
//! (vault-private, does not sync — see the history in
//! `docs/plans/2026-08-13-mls-receive-gate-ucan-on-commit.md` §1/§3 for why
//! that made every non-granting-admin receiver reject legitimate commits).
//! Instead the committer's UCAN chain travels alongside the commit on the
//! wire (`Request::MlsSendMessage::committer_ucan` +
//! `committer_commit_bind_sig`), gets verified by the caller
//! (`space_delivery::local::ucan::resolve_presented_committer_capability`,
//! which owns the UCAN-chain-walk blast radius), and arrives here as an
//! already-verified [`PresentedCapability`]. Restricted to **Remove**
//! proposals only (plan §5.0): Adds are already bounded end-to-end by the
//! Phase-1 addee check, Phase-2 PoP verification, and the `ClaimInvite`
//! handler consuming the invite's own UCAN upstream of `add_member` — a
//! blanket committer-capability check on Adds would wedge the leader-relayed
//! `ClaimInvite` path (the relaying leader may hold only Read/Write) and
//! external-commit rejoins by Read/Write members.
//!
//! **Not yet closed:**
//!
//! - External-commit joiners are not PoP-verified (openmls 0.8.1 limitation,
//!   documented on [`verify_pops`]). Because of this, nothing in this module
//!   may treat a `Sender::NewMemberCommit` credential DID as authenticated —
//!   see the `self_removal` derivation in [`inspect`].
//! - The interim `Invite`-or-higher rule is a blunt hierarchical gate, not
//!   the orthogonal "may remove but not invite" / "may invite but not kick"
//!   split the plan's final design calls for.
//! - Delivery paths outside local/P2P space delivery (e.g. `mls_decrypt` /
//!   `mls_process_message` Tauri commands, used for online-space message
//!   processing) have no UCAN-on-commit plumbing and always pass `None` for
//!   `presented_capability` — a Remove on those paths only merges via the
//!   target-already-gone exemption. Out of scope for this plan (P2P-only).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use openmls::credentials::{Credential, CredentialType};
use openmls::framing::Sender;
use openmls::group::{MlsGroup, StagedCommit};
use openmls::messages::proposals::{Proposal, ProposalType};
use rusqlite::Connection;

use crate::ucan::{Cap, CapabilitySet};

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
    /// we expect). Feeds the `Sender::NewMemberCommit` arm of
    /// [`CommitFacts::self_removal`] as a *secondary* condition next to the
    /// authoritative MLS-signature-key comparison — a rejoining
    /// external-commit joiner has no pre-commit leaf of their own, so there
    /// is no leaf index to compare against. On its own a DID match would be
    /// worthless there: the credential on an external commit is
    /// self-asserted and PoP-unverified (see [`verify_pops`]).
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
    /// True if every Remove in the commit targets the committer's own leaf
    /// (a leave, or an external-commit rejoin cleaning up its own stale
    /// leaf). Consumed by [`authorize_committer_capability`] to grant the
    /// leave exemption — only when it's the ONLY membership change in the
    /// commit (see there). Established from MLS-authenticated facts, never
    /// from the self-asserted credential DID alone; see the derivation in
    /// [`inspect`].
    pub self_removal: bool,
    /// DID resolved from the committer's own credential (`Sender::Member`'s
    /// own leaf, or the joiner's credential for `Sender::NewMemberCommit`).
    /// `None` for the anomalous sender variants (which `unmodelled` already
    /// rejects) so no caller can accidentally authorize against a
    /// meaningless credential. Feeds the capability lookup in
    /// [`authorize_committer_capability`].
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
pub(crate) fn did_from_credential(cred: &Credential) -> Option<String> {
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

    // Pre-commit MLS signature key per leaf. Unlike the credential DID this
    // is the key the leaf's own signatures verify against, so it is the only
    // MLS-authenticated way to recognise "this Remove targets the sender's
    // own (stale) leaf" for a `Sender::NewMemberCommit`, which has no
    // pre-commit leaf index of its own. See `self_removal` below.
    let leaf_to_sig_key: HashMap<u32, Vec<u8>> = group
        .members()
        .map(|m| (m.index.u32(), m.signature_key.clone()))
        .collect();

    // Unmodelled proposals AND unexpected sender variants both funnel into
    // the same fail-closed collection. `authorize` rejects the commit if this
    // is non-empty at the end of `inspect`.
    let mut unmodelled: Vec<&'static str> = Vec::new();

    // The committer's own DID. Resolved once and reused for both
    // `external_joiner` and `CommitFacts::committer_did`.
    let mut committer_did = did_from_credential(committer_credential);

    // Committer's own leaf index and the external-commit joiner's DID (only
    // one of the two applies).
    let (committer_leaf, external_joiner) = match sender {
        Sender::Member(idx) => (Some(idx.u32()), None),
        Sender::NewMemberCommit => (None, committer_did.clone()),
        // External senders and NewMemberProposal are not expected on a commit
        // path per RFC 9420; record the anomaly so `authorize` rejects the
        // whole commit rather than silently accepting when no adds/updates
        // happen to be present. Drop `committer_did` too — whatever openmls
        // put in `committer_credential` for those variants is meaningless,
        // and a `None` keeps `authorize_committer_capability` fail-closed
        // even if it is ever reached without `authorize` running first.
        Sender::NewMemberProposal => {
            unmodelled.push("UnexpectedSender:NewMemberProposal");
            committer_did = None;
            (None, None)
        }
        Sender::External(_) => {
            unmodelled.push("UnexpectedSender:External");
            committer_did = None;
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

    // Two genuinely different shapes of "the committer removed only their
    // own leaf", each established from MLS-authenticated facts only.
    //
    // Deliberately NOT a plain `credential_did == committer_did` comparison:
    // the credential on an external commit is self-asserted and NOT
    // PoP-verified (openmls 0.8.1 limitation, see `verify_pops`), so a DID
    // match alone would turn the leave exemption in
    // `authorize_committer_capability` into "evict any member whose DID you
    // can name" — exactly the hole the committer-capability gate exists to
    // close.
    let self_removal = match sender {
        // A `Sender::Member` leave removes the sender's own leaf. The leaf
        // index comes from the MLS framing, which `process_message` has
        // already authenticated against that leaf's signature key.
        Sender::Member(idx) => {
            let own = idx.u32();
            !removes.is_empty() && removes.iter().all(|r| r.leaf_index == own)
        }
        // A rejoining external-commit member has no pre-commit leaf of their
        // own, so there is no index to compare. openmls auto-generates a
        // Remove for the rejoiner's stale leaf precisely when the new leaf
        // REUSES THE SAME MLS SIGNATURE KEY (which `join_by_external_commit`
        // does — one persisted signer per identity), so key equality against
        // the commit's own update-path leaf is what identifies the cleanup.
        // The credential DID must match as well, as a secondary check.
        Sender::NewMemberCommit => {
            match (staged.update_path_leaf_node(), committer_did.as_deref()) {
                (Some(own_leaf), Some(did)) => {
                    let own_sig_key = own_leaf.signature_key().as_slice();
                    !removes.is_empty()
                        && removes.iter().all(|r| {
                            leaf_to_sig_key.get(&r.leaf_index).map(|k| k.as_slice())
                                == Some(own_sig_key)
                                && r.credential_did.as_deref() == Some(did)
                        })
                }
                _ => false,
            }
        }
        // Anomalous senders are rejected via `unmodelled`; never exempt them.
        Sender::NewMemberProposal | Sender::External(_) => false,
    };

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

/// A committer capability proof presented alongside a received commit,
/// already verified by the caller
/// (`space_delivery::local::ucan::resolve_presented_committer_capability`):
/// UCAN signature, expiry, `prf`-chain walk to the space root, and the
/// self-certifying `space_id` binding all passed. This module only makes
/// the two decisions that remain application policy — does `audience_did`
/// match the commit's authenticated committer, and does `level` meet the
/// Invite-or-higher floor — never touching raw UCAN/JWT parsing itself
/// (plan §5.8/§9: keeps this module out of the UCAN-verify blast radius).
#[derive(Debug, Clone)]
pub struct PresentedCapability {
    /// `aud` of the outermost presented UCAN token.
    pub audience_did: String,
    /// The [`CapabilitySet`] the presented token carries for this space —
    /// the orthogonal set of `(Cap, delegatable)` grants attached to the
    /// leaf. The receive-gate consults it via
    /// [`CapabilitySet::can`] (Invite / Admin) rather than a hierarchical
    /// "level" check.
    pub capabilities: CapabilitySet,
}

/// Phase-3 check, restricted to **Remove** proposals (plan §5.0 — see the
/// module header for why Adds are excluded): removing an ACTIVE member
/// requires the committer to hold [`Cap::Invite`] or [`Cap::Admin`]. This
/// is the interim rule from §5.7 of the plan — no dedicated
/// `Cap::ManageMembers`, just the two membership-changing caps under the
/// orthogonal [`CapabilitySet`] model.
///
/// Exemptions:
/// - No Remove at all (an Add-only commit, key rotation, PSK, ordinary
///   application traffic) — no capability requirement.
/// - A member removing ONLY themselves — exactly one Remove, targeting
///   their own leaf (`CommitFacts::self_removal`), and no Adds — must
///   always be allowed. Leaving a space can never require a capability the
///   leaver may not hold; if a self-remove is bundled with another Remove
///   the capability requirement still applies to the commit as a whole.
/// - **Target-already-gone** — every removed leaf's DID is no longer an
///   active member of `haex_space_members` on this receiver. Symmetric with
///   [`authorize_local_removal`]'s exemption: the removal was already
///   authorized upstream by whatever removed the row (e.g. any peer that
///   holds Invite/Admin — including the elected delivery leader who may
///   hold only Read/Write — rotating keys after a member's self-leave
///   already propagated via the shared-space delete-log), and
///   `haex_space_members` is space-scoped CRDT state so this check is
///   exercisable on every receiver, not just the granting admin.
///
///   **KNOWN DIVERGENCE RISK (plan §5.8 followup — CodeRabbit finding on
///   PR #782).** Row absence in `haex_space_members` has two meanings on a
///   receiver: (a) the delete propagated and the member is gone, and
///   (b) this receiver has not yet applied the ADD in the first place
///   (fresh peer, or CRDT sync lagging the MLS commit fan-out). Case (b)
///   means a proofless Remove commit from an Invite-lacking committer can
///   still be accepted here while peers who HAVE applied the ADD reject
///   the same commit, splitting the MLS group. The safest tightening is a
///   positive-evidence check (delete-log entry keyed on the target's
///   identity), which requires adding DID/identity to the shared-space
///   delete-log schema — deferred to the follow-up task tracked in
///   `docs/plans/2026-08-13-mls-receive-gate-ucan-on-commit.md` §"Deferred
///   follow-ups". Until then the exemption is intentional: it keeps the
///   legitimate delivery-leader-rekey-after-self-leave path working on
///   receivers who have converged on the departure, at the cost of the
///   divergence window in case (b). Multi-peer attack coverage for this
///   window is filed in the plan's outstanding e2e-spec matrix.
///
/// Otherwise `presented` must be `Some`, its `audience_did` must equal the
/// commit's MLS-authenticated `CommitFacts::committer_did`, and its
/// [`CapabilitySet`] must hold [`Cap::Invite`] or [`Cap::Admin`]. The caller
/// has already verified the UCAN chain and the separate commit-bind
/// signature over this exact commit's bytes before constructing
/// `presented` — this function trusts both.
pub(crate) fn authorize_committer_capability(
    conn: &Arc<Mutex<Option<Connection>>>,
    space_id: &str,
    facts: &CommitFacts,
    presented: Option<&PresentedCapability>,
) -> Result<(), String> {
    if facts.removes.is_empty() {
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

    // KNOWN DIVERGENCE RISK: `is_space_member=false` can mean either "the
    // delete propagated" or "this receiver never applied the ADD". Both
    // fire the exemption today. See the docstring above for the deferred
    // positive-evidence fix (plan §5.8 follow-up).
    let all_targets_already_gone = with_authz_conn(conn, |conn_ref| {
        for r in &facts.removes {
            let still_active = match r.credential_did.as_deref() {
                Some(did) => is_space_member(conn_ref, space_id, did)?,
                // No pre-commit leaf DID resolved for this Remove target —
                // defensively treat as "not provably gone" so the gate
                // still requires proof rather than silently exempting.
                None => true,
            };
            if still_active {
                return Ok(false);
            }
        }
        Ok(true)
    })?;
    if all_targets_already_gone {
        return Ok(());
    }

    let presented = presented.ok_or_else(|| {
        format!(
            "Rejecting MLS commit for space {space_id}: Remove of an active member requires a \
             committer capability proof, none presented"
        )
    })?;

    if presented.audience_did != committer_did {
        return Err(format!(
            "Rejecting MLS commit for space {space_id}: presented capability audience {} does \
             not match the commit's committer {committer_did}",
            presented.audience_did
        ));
    }

    // Semantic preservation across the CapabilityLevel → CapabilitySet
    // migration (W4 PR-3): the old `.allows(&Invite)` lattice check accepted
    // both Invite and Admin as "Invite-or-higher". Under orthogonal caps a
    // token carrying only `Cap::Admin` no longer implicitly grants
    // `Cap::Invite`, so we accept either explicitly. Any future
    // Cap::ManageMembers (plan §5.7) would be added here rather than by
    // reintroducing a lattice.
    if !(presented.capabilities.can(Cap::Invite) || presented.capabilities.can(Cap::Admin)) {
        return Err(format!(
            "Rejecting MLS commit for space {space_id}: committer {committer_did} presented \
             {:?} but membership removal requires Invite or Admin",
            presented.capabilities
        ));
    }

    Ok(())
}

/// Run `f` with the authorization connection. Every failure to obtain it —
/// poisoned mutex, no open database — is an `Err`, i.e. reject: all three
/// authorization gates in this module are fail-closed.
fn with_authz_conn<T>(
    conn: &Arc<Mutex<Option<Connection>>>,
    f: impl FnOnce(&Connection) -> Result<T, String>,
) -> Result<T, String> {
    let guard = conn
        .lock()
        .map_err(|e| format!("Authorization mutex poisoned: {e}"))?;
    let conn_ref = guard.as_ref().ok_or_else(|| {
        "No database connection available for MLS commit authorization".to_string()
    })?;
    f(conn_ref)
}

/// Shared decision of both capability gates: `did` must hold [`Cap::Invite`]
/// or [`Cap::Admin`] in `space_id`. `operation` names the gated action for
/// the rejection message ("MLS commit" / "local MLS remove").
///
/// Preserves the pre-orthogonal "Invite-or-higher" behavior by accepting
/// either cap explicitly — Admin does NOT implicitly grant Invite under
/// [`CapabilitySet`], so a raw `.can(Cap::Invite)` alone would reject a
/// pure-Admin holder. See [`authorize_committer_capability`] for the
/// matching remote-side check.
fn require_invite_or_higher(
    conn: &Connection,
    space_id: &str,
    did: &str,
    operation: &str,
) -> Result<(), String> {
    match committer_capability(conn, space_id, did)? {
        Some(set) if set.can(Cap::Invite) || set.can(Cap::Admin) => Ok(()),
        Some(set) => Err(format!(
            "Rejecting {operation} for space {space_id}: committer {did} holds {set:?} but \
             membership changes require Invite or Admin"
        )),
        None => Err(format!(
            "Rejecting {operation} for space {space_id}: committer {did} holds no capability \
             for this space"
        )),
    }
}

/// Aggregate, non-expired [`CapabilitySet`] held by `did` in `space_id`, or
/// `None` if none. Mirrors
/// `crate::space_delivery::local::ucan::load_active_ucan_for_audience`'s
/// query shape (same table, same `expires_at` filter) but returns a
/// [`CapabilitySet`] union across every non-expired token stored for the
/// (space, did) pair — under orthogonal capabilities a member can hold
/// independent `Read` + `Invite` grants as two separate rows, and both
/// must count toward "does this member hold Invite".
///
/// **Delegatable flag.** Under Task 8b the `capabilities` column carries a
/// JSON [`CapabilitySet`] per row (may be a singleton or multi-cap). Rows
/// are merged into one aggregate set for the gate — delegatable bits from
/// individual entries are collapsed to `false` on merge: the gate reads
/// only [`CapabilitySet::can`], never [`CapabilitySet::is_delegatable`],
/// so the bit is unobservable and defaulting-to-false is fail-closed.
fn committer_capability(
    conn: &Connection,
    space_id: &str,
    did: &str,
) -> Result<Option<CapabilitySet>, String> {
    // Fail closed on a broken clock rather than defaulting to 0: a `now_secs`
    // of 0 would make `expires_at > 0` true for essentially every stored
    // token, i.e. treat everything as unexpired. `load_active_ucan_for_audience`
    // has the same `unwrap_or(0)` shape for a non-authorization read; here,
    // where the result gates a membership change, silently answering "yes,
    // valid" on a clock error is the wrong default.
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| {
            format!("System clock is before UNIX epoch, refusing to evaluate token expiry: {e}")
        })?
        .as_secs() as i64;

    let mut stmt = conn
        .prepare(
            "SELECT capabilities FROM haex_ucan_tokens \
             WHERE space_id = ?1 AND audience_did = ?2 AND expires_at > ?3",
        )
        .map_err(|e| format!("Failed to prepare capability lookup: {e}"))?;

    let rows = stmt
        .query_map(rusqlite::params![space_id, did, now_secs], |row| {
            row.get::<_, String>(0)
        })
        .map_err(|e| format!("Capability lookup failed for space={space_id} did={did}: {e}"))?;

    let mut builder = CapabilitySet::builder();
    let mut saw_any = false;
    for row in rows {
        let capabilities_str = row.map_err(|e| format!("Failed to read capability row: {e}"))?;
        let Ok(set) = serde_json::from_str::<CapabilitySet>(&capabilities_str) else {
            // Skip malformed rows silently — belt-and-braces alongside the
            // Task-8b migration that drops legacy rows; a stray, un-parseable
            // row must not fail-open the gate for the whole audience.
            continue;
        };
        for entry in set.entries() {
            saw_any = true;
            builder = match entry.cap {
                Cap::Read => builder.read(false),
                Cap::Write => builder.write(false),
                Cap::Invite => builder.invite(false),
                Cap::Admin => builder.admin(false),
            };
        }
    }
    Ok(if saw_any { Some(builder.build()) } else { None })
}

/// Require the LOCAL committer to hold [`Cap::Invite`] or [`Cap::Admin`]
/// before locally originating a Remove of an ACTIVE member. Mirrors
/// [`authorize_committer_capability`]'s gate, but for commits **we create**
/// (`MlsManager::remove_member`) rather than commits we receive: those never
/// reach `decrypt`/`inspect`, so nothing gated them before this fix (a
/// CodeRabbit finding on PR #781).
///
/// Returns `Ok(true)` if the removal was gated and passed (an active
/// member is being removed and the committer holds Invite-or-higher) —
/// `MlsManager::remove_member` uses this to decide whether to also attach a
/// committer-capability proof to the outgoing envelope, since a receiver
/// will independently apply the same target-gone exemption via
/// [`authorize_committer_capability`] and would reject a proof-less commit
/// otherwise. Returns `Ok(false)` when `target_did` is exempt (see below) —
/// no proof needed downstream either.
///
/// Exempt: `target_did` is no longer an active `haex_space_members` row.
/// This covers the leader-side rekey-after-self-leave flow
/// (`reconcileMls.ts`): once a member leaves, the CRDT delete-log entry
/// removes their `haex_space_members` row on every peer, and *any* peer —
/// not necessarily one holding Invite/Admin — may become the elected P2P
/// delivery leader (`elect_leader` picks by network priority/reachability
/// only, see `space_delivery::local::election`) responsible for rotating
/// the MLS key afterward. Gating that rekey on the leader's own capability
/// would incorrectly block a legitimate, already-authorized cleanup.
///
/// NOT applied to `MlsManager::add_member`: the authority to add a member
/// comes from the invite's own delegated UCAN capability (checked when the
/// invite is created/consumed), not from the local device's capability —
/// the device calling `add_member` may just be the elected delivery leader
/// relaying a `ClaimInvite` for a token a real Invite/Admin holder issued.
/// Gating `add_member` the same way would break that relay path for any
/// leader who happens to hold only Read/Write.
pub(crate) fn authorize_local_removal(
    conn: &Arc<Mutex<Option<Connection>>>,
    space_id: &str,
    committer_did: &str,
    target_did: &str,
) -> Result<bool, String> {
    with_authz_conn(conn, |conn_ref| {
        if !is_space_member(conn_ref, space_id, target_did)? {
            return Ok(false);
        }
        require_invite_or_higher(conn_ref, space_id, committer_did, "local MLS remove")?;
        Ok(true)
    })
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
