//! Unit tests for the Phase-1 authorization policy.
//!
//! End-to-end coverage over an actual `MlsManager::decrypt` cycle lives in
//! `src-tauri/tests/mls_lifecycle.rs` (see
//! `external_commit_rejoin_roundtrip` for the accepted path and
//! `external_commit_from_unseeded_joiner_is_rejected` for the fail-closed
//! path); these tests synthesize `CommitFacts` directly so each §7 row of
//! the plan can be exercised without spinning up a real openmls group.

use std::sync::{Arc, Mutex};

use rusqlite::Connection;

use super::{
    authorize, authorize_committer_capability, authorize_local_removal, verify_pops, AddFact,
    CommitFacts, PresentedCapability, UpdateFact,
};
use crate::ucan::CapabilitySet;

/// Minimal schema: the two tables the addee-membership check joins, plus
/// `haex_ucan_tokens` for the Phase-3 committer-capability lookup. No
/// `haex_tombstone` column — in the delete-log model post `crate::crdt`
/// refactor, revocation is expressed by row absence, not a tombstone flag.
/// `haex_ucan_tokens` never had a tombstone column even before that
/// refactor (see production schema `0000_jazzy_chat.sql`).
const MEMBERSHIP_SQL: &str = "
CREATE TABLE haex_identities (
    id TEXT PRIMARY KEY,
    did TEXT NOT NULL
);
CREATE TABLE haex_space_members (
    id TEXT PRIMARY KEY,
    space_id TEXT NOT NULL,
    identity_id TEXT NOT NULL,
    role TEXT
);
CREATE TABLE haex_ucan_tokens (
    id TEXT PRIMARY KEY,
    space_id TEXT NOT NULL,
    token TEXT NOT NULL,
    capability TEXT NOT NULL,
    issuer_did TEXT NOT NULL,
    audience_did TEXT NOT NULL,
    issued_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL
);
";

fn fresh_db() -> Arc<Mutex<Option<Connection>>> {
    let conn = Connection::open_in_memory().expect("open_in_memory");
    conn.execute_batch(MEMBERSHIP_SQL)
        .expect("create membership tables");
    Arc::new(Mutex::new(Some(conn)))
}

fn seed_member(conn: &Arc<Mutex<Option<Connection>>>, space_id: &str, did: &str) {
    let guard = conn.lock().unwrap();
    let c = guard.as_ref().unwrap();
    let identity_id = format!("id-{did}");
    let member_id = format!("mem-{space_id}-{did}");
    c.execute(
        "INSERT INTO haex_identities (id, did) VALUES (?1, ?2)",
        rusqlite::params![identity_id, did],
    )
    .expect("insert identity");
    c.execute(
        "INSERT INTO haex_space_members (id, space_id, identity_id, role) VALUES (?1, ?2, ?3, 'member')",
        rusqlite::params![member_id, space_id, identity_id],
    )
    .expect("insert member");
}

/// Simulate the delete-log apply path: remove a member row so the DID is no
/// longer present when `authorize` looks it up. Revocation = absence.
fn remove_member(conn: &Arc<Mutex<Option<Connection>>>, space_id: &str, did: &str) {
    let guard = conn.lock().unwrap();
    let c = guard.as_ref().unwrap();
    c.execute(
        "DELETE FROM haex_space_members \
         WHERE space_id = ?1 \
           AND identity_id IN (SELECT id FROM haex_identities WHERE did = ?2)",
        rusqlite::params![space_id, did],
    )
    .expect("remove member");
}

/// Grant `did` a `space/<capability>` UCAN token for `space_id`, expiring
/// far in the future (2286 — same order of magnitude as production's
/// `MEMBER_UCAN_EXPIRES_IN_SECONDS`, just a literal here since this module
/// has no access to that constant's crate path).
fn grant_capability(
    conn: &Arc<Mutex<Option<Connection>>>,
    space_id: &str,
    did: &str,
    capability: &str,
) {
    grant_capability_expiring_at(conn, space_id, did, capability, 9_999_999_999);
}

fn grant_capability_expiring_at(
    conn: &Arc<Mutex<Option<Connection>>>,
    space_id: &str,
    did: &str,
    capability: &str,
    expires_at: i64,
) {
    let guard = conn.lock().unwrap();
    let c = guard.as_ref().unwrap();
    let id = format!("ucan-{space_id}-{did}-{capability}-{expires_at}");
    c.execute(
        "INSERT INTO haex_ucan_tokens \
         (id, space_id, token, capability, issuer_did, audience_did, issued_at, expires_at) \
         VALUES (?1, ?2, 'test-token', ?3, 'did:key:zIssuer', ?4, 0, ?5)",
        rusqlite::params![id, space_id, capability, did, expires_at],
    )
    .expect("insert ucan token");
}

fn add(did: &str) -> AddFact {
    // Existing Phase-1 tests exercise `authorize` only, which does not
    // consume `mls_sig_pub` or `pop_bytes`; zeros / `None` are fine.
    // Phase-2 tests below construct `AddFact` inline with real values.
    AddFact {
        credential_did: did.to_string(),
        mls_sig_pub: [0u8; 32],
        pop_bytes: None,
    }
}

fn update_stable(leaf: u32, did: &str) -> UpdateFact {
    UpdateFact {
        leaf_index: leaf,
        old_did: Some(did.to_string()),
        new_did: did.to_string(),
    }
}

fn update_changing(leaf: u32, old_did: &str, new_did: &str) -> UpdateFact {
    UpdateFact {
        leaf_index: leaf,
        old_did: Some(old_did.to_string()),
        new_did: new_did.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Addee membership (§7 Add rows)
// ---------------------------------------------------------------------------

#[test]
fn add_of_a_member_did_is_accepted() {
    let db = fresh_db();
    let space = "space-a";
    seed_member(&db, space, "did:key:zBob");
    let facts = CommitFacts {
        adds: vec![add("did:key:zBob")],
        ..CommitFacts::default()
    };
    authorize(&db, space, &facts).expect("legit addee must be accepted");
}

#[test]
fn add_of_a_non_member_did_is_rejected() {
    let db = fresh_db();
    let space = "space-a";
    // Seed a different DID as member, not the one being added.
    seed_member(&db, space, "did:key:zAlice");
    let facts = CommitFacts {
        adds: vec![add("did:key:zEve")],
        ..CommitFacts::default()
    };
    let err = authorize(&db, space, &facts).expect_err("non-member addee must be rejected");
    assert!(
        err.contains("not a member") && err.contains("zEve"),
        "expected addee-not-member error, got: {err}"
    );
}

#[test]
fn add_of_a_removed_member_is_rejected() {
    // Post delete-log refactor a revoked member is simply absent from
    // `haex_space_members` (the delete-log apply path removes the row); a
    // successful membership lookup requires the row to be there. Simulate
    // that by seeding then removing.
    let db = fresh_db();
    let space = "space-a";
    seed_member(&db, space, "did:key:zEx");
    remove_member(&db, space, "did:key:zEx");
    let facts = CommitFacts {
        adds: vec![add("did:key:zEx")],
        ..CommitFacts::default()
    };
    let err = authorize(&db, space, &facts)
        .expect_err("removed addee must be rejected — absence in haex_space_members = revocation");
    assert!(err.contains("not a member"), "unexpected error: {err}");
}

#[test]
fn add_scoped_to_a_different_space_is_rejected() {
    let db = fresh_db();
    seed_member(&db, "space-other", "did:key:zBob");
    let facts = CommitFacts {
        adds: vec![add("did:key:zBob")],
        ..CommitFacts::default()
    };
    let err = authorize(&db, "space-a", &facts)
        .expect_err("membership in a different space must not authorize an add here");
    assert!(err.contains("not a member"), "unexpected error: {err}");
}

#[test]
fn mixed_one_member_one_stranger_add_rejects_the_whole_commit() {
    let db = fresh_db();
    let space = "space-a";
    seed_member(&db, space, "did:key:zBob");
    let facts = CommitFacts {
        adds: vec![add("did:key:zBob"), add("did:key:zEve")],
        ..CommitFacts::default()
    };
    let err = authorize(&db, space, &facts)
        .expect_err("a single non-member addee poisons the whole commit");
    assert!(err.contains("zEve"), "unexpected error: {err}");
}

#[test]
fn empty_credential_bytes_are_rejected() {
    let db = fresh_db();
    let space = "space-a";
    let facts = CommitFacts {
        adds: vec![add("")],
        ..CommitFacts::default()
    };
    let err = authorize(&db, space, &facts).expect_err("empty DID must be rejected up-front");
    assert!(err.contains("empty"), "unexpected error: {err}");
}

// ---------------------------------------------------------------------------
// External-commit joiner (§7 External rows)
// ---------------------------------------------------------------------------

#[test]
fn external_commit_from_a_member_is_accepted() {
    let db = fresh_db();
    let space = "space-a";
    seed_member(&db, space, "did:key:zRejoiner");
    let facts = CommitFacts {
        external_joiner: Some("did:key:zRejoiner".to_string()),
        ..CommitFacts::default()
    };
    authorize(&db, space, &facts).expect("member rejoin must be accepted");
}

#[test]
fn external_commit_from_a_non_member_is_rejected() {
    let db = fresh_db();
    let space = "space-a";
    let facts = CommitFacts {
        external_joiner: Some("did:key:zStranger".to_string()),
        ..CommitFacts::default()
    };
    let err = authorize(&db, space, &facts)
        .expect_err("non-member external-commit joiner must be rejected");
    assert!(err.contains("zStranger"), "unexpected error: {err}");
}

// ---------------------------------------------------------------------------
// Credential stability on Update (§7 Update rows)
// ---------------------------------------------------------------------------

#[test]
fn update_that_preserves_did_is_accepted() {
    let db = fresh_db();
    let facts = CommitFacts {
        updates: vec![update_stable(3, "did:key:zBob")],
        ..CommitFacts::default()
    };
    authorize(&db, "space-a", &facts).expect("same-DID update must merge");
}

#[test]
fn update_that_changes_did_is_rejected() {
    let db = fresh_db();
    let facts = CommitFacts {
        updates: vec![update_changing(3, "did:key:zBob", "did:key:zEve")],
        ..CommitFacts::default()
    };
    let err = authorize(&db, "space-a", &facts)
        .expect_err("changing the credential DID must be rejected");
    assert!(
        err.contains("credential-stability"),
        "unexpected error: {err}"
    );
}

#[test]
fn update_over_an_empty_slot_is_rejected() {
    let db = fresh_db();
    let facts = CommitFacts {
        updates: vec![UpdateFact {
            leaf_index: 99,
            old_did: None,
            new_did: "did:key:zSomeone".to_string(),
        }],
        ..CommitFacts::default()
    };
    let err = authorize(&db, "space-a", &facts)
        .expect_err("Update targeting an empty slot is unmodelled — reject");
    assert!(
        err.contains("no prior credential"),
        "unexpected error: {err}"
    );
}

// ---------------------------------------------------------------------------
// Fail-closed on unmodelled proposals (§7 Unmodelled row)
// ---------------------------------------------------------------------------

#[test]
fn unmodelled_proposal_types_reject_the_whole_commit() {
    let db = fresh_db();
    let space = "space-a";
    seed_member(&db, space, "did:key:zBob");
    // Even a commit that would otherwise pass (legit addee) is rejected as
    // soon as an unmodelled proposal type is present.
    let facts = CommitFacts {
        adds: vec![add("did:key:zBob")],
        unmodelled: vec!["PreSharedKey"],
        ..CommitFacts::default()
    };
    let err = authorize(&db, space, &facts)
        .expect_err("unmodelled proposal must reject the whole commit fail-closed");
    assert!(
        err.contains("unmodelled") && err.contains("PreSharedKey"),
        "unexpected error: {err}"
    );
}

// ---------------------------------------------------------------------------
// Regressions: Remove-only + self-removal are allowed in Phase 1
// (committer-capability check is Phase 3; Phase 1 only surfaces facts).
// ---------------------------------------------------------------------------

#[test]
fn plain_remove_commit_is_accepted_in_phase_1() {
    let db = fresh_db();
    let space = "space-a";
    // No addee to check, no unmodelled types, no updates → policy has nothing
    // to reject on. Phase 3 will add the committer-capability rule that
    // would reject a read-only member's Remove.
    let facts = CommitFacts {
        removes: vec![super::RemoveFact {
            leaf_index: 2,
            credential_did: Some("did:key:zTarget".to_string()),
        }],
        ..CommitFacts::default()
    };
    authorize(&db, space, &facts)
        .expect("Phase-1 policy does not gate removes; committer-cap check is Phase 3");
}

#[test]
fn self_removal_records_the_flag_and_stays_permitted() {
    // Sanity: the `self_removal` flag has no effect in Phase 1, but the
    // shape stays permitted so a member leaving does not get stuck.
    let db = fresh_db();
    let facts = CommitFacts {
        removes: vec![super::RemoveFact {
            leaf_index: 4,
            credential_did: Some("did:key:zSelfLeaver".to_string()),
        }],
        self_removal: true,
        ..CommitFacts::default()
    };
    authorize(&db, "space-a", &facts).expect("self-leave must not be blocked by Phase-1 policy");
}

// ---------------------------------------------------------------------------
// Empty commit (application traffic surrounding path) — no-op accepts
// ---------------------------------------------------------------------------

#[test]
fn commit_with_no_membership_change_is_accepted() {
    let db = fresh_db();
    // Fully empty `CommitFacts`: no adds, no removes, no updates, no
    // unmodelled proposals. `authorize` has nothing to reject on and returns
    // Ok. Stable-DID path-in-commit rotation is covered by
    // `update_that_preserves_did_is_accepted`.
    let facts = CommitFacts::default();
    authorize(&db, "space-a", &facts)
        .expect("a commit without adds/removes/updates/unmodelled is a no-op for Phase-1");
}

// ---------------------------------------------------------------------------
// Phase-2: proof-of-possession leaf-extension verification
// ---------------------------------------------------------------------------

fn real_identity() -> (String, ed25519_dalek::SigningKey) {
    let sk = ed25519_dalek::SigningKey::from_bytes(&rand::random());
    let did = crate::ucan::did_key_from_public_key(&sk.verifying_key());
    (did, sk)
}

fn add_with_valid_pop(
    did: &str,
    identity_sk: &ed25519_dalek::SigningKey,
    mls_sig_pub: [u8; 32],
) -> AddFact {
    let sig = crate::mls::pop::sign_pop(identity_sk, &mls_sig_pub, did);
    AddFact {
        credential_did: did.to_string(),
        mls_sig_pub,
        pop_bytes: Some(sig.to_bytes().to_vec()),
    }
}

#[test]
fn verify_pops_accepts_a_valid_pop() {
    let (did, id_sk) = real_identity();
    let mls_sig_pub = rand::random::<[u8; 32]>();
    let facts = CommitFacts {
        adds: vec![add_with_valid_pop(&did, &id_sk, mls_sig_pub)],
        ..CommitFacts::default()
    };
    verify_pops("space-a", &facts).expect("a valid PoP must be accepted");
}

#[test]
fn verify_pops_rejects_missing_extension() {
    let (did, _) = real_identity();
    // Simulate an addee whose KeyPackage has no PoP leaf extension —
    // `inspect` would set `pop_bytes: None` in that case.
    let facts = CommitFacts {
        adds: vec![AddFact {
            credential_did: did.clone(),
            mls_sig_pub: rand::random(),
            pop_bytes: None,
        }],
        ..CommitFacts::default()
    };
    let err =
        verify_pops("space-a", &facts).expect_err("addee without PoP extension must be rejected");
    assert!(
        err.contains("missing the required proof-of-possession"),
        "unexpected error: {err}"
    );
}

#[test]
fn verify_pops_rejects_malformed_pop_bytes() {
    let (did, _) = real_identity();
    let facts = CommitFacts {
        adds: vec![AddFact {
            credential_did: did,
            mls_sig_pub: rand::random(),
            // Wrong length (5 bytes) — not a 64-byte Ed25519 signature.
            pop_bytes: Some(vec![0u8; 5]),
        }],
        ..CommitFacts::default()
    };
    let err = verify_pops("space-a", &facts).expect_err("malformed PoP length must be rejected");
    assert!(err.contains("malformed"), "unexpected error: {err}");
}

#[test]
fn verify_pops_rejects_pop_signed_by_a_different_identity_key() {
    // Attacker mints a KeyPackage naming the victim's DID, but signs the
    // PoP with their OWN identity key. The receiver resolves `identity_pub`
    // from the victim DID and rejects on signature verification failure.
    let (victim_did, _victim_sk) = real_identity();
    let (_attacker_did, attacker_sk) = real_identity();
    let mls_sig_pub = rand::random::<[u8; 32]>();
    let facts = CommitFacts {
        adds: vec![add_with_valid_pop(&victim_did, &attacker_sk, mls_sig_pub)],
        ..CommitFacts::default()
    };
    let err = verify_pops("space-a", &facts)
        .expect_err("PoP signed by attacker's identity key must not verify against victim DID");
    assert!(err.contains("does not verify"), "unexpected error: {err}");
}

#[test]
fn verify_pops_rejects_pop_covering_a_different_mls_sig_key() {
    // Attacker replays a legitimate PoP the victim's identity key signed
    // over some other MLS signature key, but puts a different key on their
    // KeyPackage. `verify_pop` fails because the covered message differs.
    let (victim_did, victim_sk) = real_identity();
    let original_mls_sig_pub = rand::random::<[u8; 32]>();
    let sig = crate::mls::pop::sign_pop(&victim_sk, &original_mls_sig_pub, &victim_did);
    let facts = CommitFacts {
        adds: vec![AddFact {
            credential_did: victim_did,
            // KP carries a DIFFERENT MLS sig key from what the PoP covers.
            mls_sig_pub: rand::random(),
            pop_bytes: Some(sig.to_bytes().to_vec()),
        }],
        ..CommitFacts::default()
    };
    let err = verify_pops("space-a", &facts)
        .expect_err("PoP bound to a different MLS sig key must not verify");
    assert!(err.contains("does not verify"), "unexpected error: {err}");
}

#[test]
fn verify_pops_rejects_when_did_does_not_resolve() {
    // A syntactically invalid DID cannot be resolved to a public key. Reject
    // with the resolution error rather than blindly accepting.
    let facts = CommitFacts {
        adds: vec![add_with_valid_pop(
            "did:key:not-a-real-did",
            &ed25519_dalek::SigningKey::from_bytes(&rand::random()),
            rand::random(),
        )],
        ..CommitFacts::default()
    };
    let err = verify_pops("space-a", &facts).expect_err("unresolvable DID must be rejected");
    assert!(
        err.contains("cannot resolve identity key"),
        "unexpected error: {err}"
    );
}

#[test]
fn verify_pops_is_a_no_op_with_no_adds() {
    // An external commit or a remove-only commit carries no Add proposals;
    // Phase-2 has nothing to do. (External-commit joiners are documented as
    // out of scope for Phase 2 due to openmls-0.8.1's builder limitation.)
    let facts = CommitFacts {
        external_joiner: Some("did:key:zAnyone".to_string()),
        ..CommitFacts::default()
    };
    verify_pops("space-a", &facts).expect("verify_pops has nothing to do when there are no adds");
}

// ---------------------------------------------------------------------------
// Phase-3: committer-capability gate (§7 rows involving the committer)
// ---------------------------------------------------------------------------

fn remove_of(leaf_index: u32, did: &str) -> super::RemoveFact {
    super::RemoveFact {
        leaf_index,
        credential_did: Some(did.to_string()),
    }
}

#[test]
fn no_membership_change_needs_no_capability() {
    let db = fresh_db();
    // Committer holds nothing at all — no capability presented — yet an
    // empty commit (key rotation / PSK / ordinary traffic) must still pass.
    let facts = CommitFacts {
        committer_did: Some("did:key:zNobody".to_string()),
        ..CommitFacts::default()
    };
    authorize_committer_capability(&db, "space-a", &facts, None)
        .expect("a commit with no removes needs no committer capability");
}

#[test]
fn add_only_commit_never_requires_committer_capability() {
    // Plan §5.0 (BLOCKING review finding): a receive-gate that required the
    // committer to hold Invite-or-higher on Adds would wedge leader-relayed
    // `ClaimInvite` Adds, since the elected P2P delivery leader may hold
    // only Read/Write. Adds are bounded end-to-end by Phase-1 addee-check +
    // Phase-2 PoP + the ClaimInvite handler's own upstream UCAN consumption
    // — NOT by this gate. No capability presented, no removes — must merge.
    let db = fresh_db();
    let space = "space-a";
    let facts = CommitFacts {
        adds: vec![add("did:key:zSomeone")],
        committer_did: Some("did:key:zReadOnlyLeader".to_string()),
        ..CommitFacts::default()
    };
    authorize_committer_capability(&db, space, &facts, None)
        .expect("an Add-only commit must never require a committer capability proof");
}

#[test]
fn remove_of_an_active_member_with_valid_invite_capability_is_accepted() {
    let db = fresh_db();
    let space = "space-a";
    seed_member(&db, space, "did:key:zOwner");
    let facts = CommitFacts {
        removes: vec![remove_of(2, "did:key:zOwner")],
        committer_did: Some("did:key:zAdmin".to_string()),
        ..CommitFacts::default()
    };
    let presented = PresentedCapability {
        audience_did: "did:key:zAdmin".to_string(),
        capabilities: CapabilitySet::builder().admin(false).build(),
    };
    authorize_committer_capability(&db, space, &facts, Some(&presented))
        .expect("a presented Admin capability must be allowed to remove any active member");
}

#[test]
fn remove_of_an_active_member_with_read_capability_presented_is_rejected() {
    // Mirrors the plan's §7 row: "member removes the space owner | rejected
    // (owner is a member; committer needs the capability)".
    let db = fresh_db();
    let space = "space-a";
    seed_member(&db, space, "did:key:zOwner");
    let facts = CommitFacts {
        removes: vec![remove_of(1, "did:key:zOwner")],
        committer_did: Some("did:key:zReadOnly".to_string()),
        ..CommitFacts::default()
    };
    let presented = PresentedCapability {
        audience_did: "did:key:zReadOnly".to_string(),
        capabilities: CapabilitySet::builder().read(false).build(),
    };
    let err = authorize_committer_capability(&db, space, &facts, Some(&presented))
        .expect_err("read-only member must not be able to kick the owner");
    assert!(
        err.contains("Read") && err.contains("Invite or Admin"),
        "unexpected error: {err}"
    );
}

#[test]
fn write_capability_does_not_allow_membership_changes() {
    // Under the orthogonal [`CapabilitySet`] model, [`Cap::Write`] is
    // independent of the membership-changing caps [`Cap::Invite`] and
    // [`Cap::Admin`]. Holding Write alone must not satisfy the gate.
    let db = fresh_db();
    let space = "space-a";
    seed_member(&db, space, "did:key:zTarget");
    let facts = CommitFacts {
        removes: vec![remove_of(3, "did:key:zTarget")],
        committer_did: Some("did:key:zWriter".to_string()),
        ..CommitFacts::default()
    };
    let presented = PresentedCapability {
        audience_did: "did:key:zWriter".to_string(),
        capabilities: CapabilitySet::builder().write(false).build(),
    };
    let err = authorize_committer_capability(&db, space, &facts, Some(&presented))
        .expect_err("Write must not satisfy the Invite/Admin gate");
    assert!(err.contains("Write"), "unexpected error: {err}");
}

#[test]
fn remove_of_an_active_member_with_no_capability_presented_is_rejected() {
    let db = fresh_db();
    let space = "space-a";
    seed_member(&db, space, "did:key:zTarget");
    let facts = CommitFacts {
        removes: vec![remove_of(1, "did:key:zTarget")],
        committer_did: Some("did:key:zNobody".to_string()),
        ..CommitFacts::default()
    };
    let err = authorize_committer_capability(&db, space, &facts, None)
        .expect_err("removing an active member with no proof presented at all must be rejected");
    assert!(err.contains("none presented"), "unexpected error: {err}");
}

#[test]
fn presented_capability_with_mismatched_audience_is_rejected() {
    // The UCAN was validly issued and chained, but to a DIFFERENT DID than
    // the one that actually signed this commit (MLS-authenticated
    // `committer_did`) — a captured/misattributed proof must not authorize
    // an unrelated committer.
    let db = fresh_db();
    let space = "space-a";
    seed_member(&db, space, "did:key:zTarget");
    let facts = CommitFacts {
        removes: vec![remove_of(1, "did:key:zTarget")],
        committer_did: Some("did:key:zRealCommitter".to_string()),
        ..CommitFacts::default()
    };
    let presented = PresentedCapability {
        audience_did: "did:key:zSomeoneElse".to_string(),
        capabilities: CapabilitySet::builder().admin(false).build(),
    };
    let err = authorize_committer_capability(&db, space, &facts, Some(&presented))
        .expect_err("a capability presented for a different DID must not authorize this commit");
    assert!(
        err.contains("does not match the commit's committer"),
        "unexpected error: {err}"
    );
}

#[test]
fn remove_with_unresolvable_target_did_still_requires_capability() {
    // Fail-closed pin: a `RemoveFact::credential_did == None` (leaf slot
    // was already empty in the pre-commit view — anomalous) must NOT
    // receive the target-gone exemption, since we cannot verify the
    // leaf's identity. This guards against a future change to
    // `authorize_committer_capability`'s `None`-arm silently opening
    // the gate. A `None` here should read as "not provably gone" and
    // force a capability proof.
    let db = fresh_db();
    let space = "space-a";
    let facts = CommitFacts {
        removes: vec![super::RemoveFact {
            leaf_index: 1,
            credential_did: None,
        }],
        committer_did: Some("did:key:zNobody".to_string()),
        ..CommitFacts::default()
    };
    let err = authorize_committer_capability(&db, space, &facts, None).expect_err(
        "a Remove whose target DID cannot be resolved must not get the target-gone exemption",
    );
    assert!(
        err.contains("none presented"),
        "expected the standard 'proof required, none presented' rejection, got: {err}"
    );
}

#[test]
fn remove_of_active_member_without_proof_stays_rejected_until_crdt_converges() {
    // Regression guard for the deferred CRDT-lag divergence risk
    // (CodeRabbit finding on PR #782, documented in
    // `authorize_committer_capability`'s docstring under "KNOWN
    // DIVERGENCE RISK"). This test pins the SAFE direction of the
    // exemption: while the target is still an active `haex_space_members`
    // row on this receiver, a proofless Remove must remain rejected.
    // Once CRDT converges and the delete-log removes the row, the
    // exemption fires; that second half is covered by
    // `remove_of_an_already_departed_member_needs_no_capability`.
    let db = fresh_db();
    let space = "space-a";
    // Simulate the "out-of-order membership deletion" scenario: the MLS
    // Remove commit arrives before the CRDT delete has propagated. The
    // receiver's `haex_space_members` still lists the target as active.
    seed_member(&db, space, "did:key:zAlreadyRemovedOnSender");
    let facts = CommitFacts {
        removes: vec![remove_of(1, "did:key:zAlreadyRemovedOnSender")],
        committer_did: Some("did:key:zRelayLeader".to_string()),
        ..CommitFacts::default()
    };
    let err = authorize_committer_capability(&db, space, &facts, None).expect_err(
        "while CRDT still shows the target as active, a proofless Remove must be rejected — \
         retry on the next sync round once the delete propagates",
    );
    assert!(err.contains("none presented"), "unexpected error: {err}");

    // Now converge: apply the delete and retry — the exemption fires.
    remove_member(&db, space, "did:key:zAlreadyRemovedOnSender");
    authorize_committer_capability(&db, space, &facts, None)
        .expect("after CRDT convergence the target-gone exemption must let the retry through");
}

#[test]
fn remove_of_an_already_departed_member_needs_no_capability() {
    // The receive-side mirror of `authorize_local_removal`'s exemption: the
    // leader rotating keys after a member's self-leave already propagated
    // (their `haex_space_members` row is gone on every peer) must not be
    // blocked just because the leader itself may hold only Read/Write.
    let db = fresh_db();
    let space = "space-a";
    // No seed_member call — target is not (or no longer) an active member.
    let facts = CommitFacts {
        removes: vec![remove_of(1, "did:key:zAlreadyGone")],
        committer_did: Some("did:key:zAnyLeader".to_string()),
        ..CommitFacts::default()
    };
    authorize_committer_capability(&db, space, &facts, None)
        .expect("removing an already-departed member's stale leaf needs no capability proof");
}

#[test]
fn remove_of_mixed_gone_and_active_members_still_requires_capability() {
    // ALL removed targets must be already-gone for the exemption to apply —
    // one still-active target among several removes must not slip through.
    let db = fresh_db();
    let space = "space-a";
    seed_member(&db, space, "did:key:zStillHere");
    // "did:key:zAlreadyGone" is deliberately not seeded.
    let facts = CommitFacts {
        removes: vec![
            remove_of(1, "did:key:zAlreadyGone"),
            remove_of(2, "did:key:zStillHere"),
        ],
        committer_did: Some("did:key:zNobody".to_string()),
        ..CommitFacts::default()
    };
    let err = authorize_committer_capability(&db, space, &facts, None).expect_err(
        "a bundle removing one active member alongside a departed one must still require proof",
    );
    assert!(err.contains("none presented"), "unexpected error: {err}");
}

#[test]
fn pure_self_leave_needs_no_capability() {
    // Exactly one Remove, targeting the committer's own leaf, no Adds — the
    // committer holds NOTHING and must still be allowed to leave.
    let db = fresh_db();
    let space = "space-a";
    let facts = CommitFacts {
        removes: vec![remove_of(5, "did:key:zLeaver")],
        self_removal: true,
        committer_did: Some("did:key:zLeaver".to_string()),
        ..CommitFacts::default()
    };
    authorize_committer_capability(&db, space, &facts, None)
        .expect("a member leaving on their own must never require a capability");
}

#[test]
fn external_rejoin_cleanup_remove_of_own_stale_leaf_needs_no_capability() {
    // Regression: openmls auto-generates a Remove for a rejoining member's
    // own stale leaf when the external commit reuses the same MLS signature
    // key (real scenario exercised end-to-end by
    // `external_commit_rejoin_roundtrip` in `mls_lifecycle.rs`, which
    // originally broke this test suite when the committer-capability gate
    // was first wired in). `committer_leaf` is `None` for
    // `Sender::NewMemberCommit`, so a leaf-index comparison always misses
    // it; `inspect` recognises it by comparing the removed leaf's MLS
    // signature key against the commit's own update-path leaf key. Here we
    // assert only the consequence: `self_removal` set on such a commit
    // exempts it.
    let db = fresh_db();
    let space = "space-a";
    let facts = CommitFacts {
        removes: vec![remove_of(1, "did:key:zRejoiner")],
        self_removal: true,
        committer_did: Some("did:key:zRejoiner".to_string()),
        external_joiner: Some("did:key:zRejoiner".to_string()),
        ..CommitFacts::default()
    };
    authorize_committer_capability(&db, space, &facts, None)
        .expect("rejoin cleanup of one's own stale leaf must never require a capability");
}

#[test]
fn external_joiner_removing_a_leaf_that_is_not_its_own_requires_capability() {
    // The DID-spoof shape: an external-commit joiner asserts a victim's DID
    // in its credential (Phase-1 lets it through because the victim IS a
    // member, and `verify_pops` cannot check an external commit's leaf) and
    // bundles a single Remove of the victim's leaf. `inspect` must NOT set
    // `self_removal` there — the removed leaf's MLS signature key is the
    // victim's, not the joiner's — so the commit stays subject to the gate.
    // This test pins the caller half: without the exemption, a joiner
    // presenting nothing is rejected. The victim must be seeded as an
    // active member — otherwise the target-gone exemption would (correctly,
    // but not what this test is pinning) let it through.
    let db = fresh_db();
    let space = "space-a";
    seed_member(&db, space, "did:key:zVictim");
    let facts = CommitFacts {
        removes: vec![remove_of(1, "did:key:zVictim")],
        self_removal: false,
        committer_did: Some("did:key:zVictim".to_string()),
        external_joiner: Some("did:key:zVictim".to_string()),
        ..CommitFacts::default()
    };
    let err = authorize_committer_capability(&db, space, &facts, None).expect_err(
        "an external-commit joiner removing a leaf that is not its own must need the capability",
    );
    assert!(err.contains("none presented"), "unexpected error: {err}");
}

#[test]
fn self_leave_bundled_with_another_remove_requires_capability() {
    // The committer removes themselves AND someone else in the same commit.
    // The bundled extra removal means the exemption does not apply — the
    // capability gate still fires for the commit as a whole. Both targets
    // must be seeded as active members so the target-gone exemption does
    // not swallow the case this test is pinning.
    let db = fresh_db();
    let space = "space-a";
    seed_member(&db, space, "did:key:zLeaver");
    seed_member(&db, space, "did:key:zOther");
    let facts = CommitFacts {
        removes: vec![
            remove_of(5, "did:key:zLeaver"),
            remove_of(6, "did:key:zOther"),
        ],
        self_removal: true,
        committer_did: Some("did:key:zLeaver".to_string()),
        ..CommitFacts::default()
    };
    let err = authorize_committer_capability(&db, space, &facts, None).expect_err(
        "self-leave bundled with removing someone else must still require the capability",
    );
    assert!(err.contains("none presented"), "unexpected error: {err}");
}

#[test]
fn self_leave_bundled_with_an_add_requires_capability() {
    let db = fresh_db();
    let space = "space-a";
    seed_member(&db, space, "did:key:zLeaver");
    let facts = CommitFacts {
        adds: vec![add("did:key:zNewcomer")],
        removes: vec![remove_of(5, "did:key:zLeaver")],
        self_removal: true,
        committer_did: Some("did:key:zLeaver".to_string()),
        ..CommitFacts::default()
    };
    let err = authorize_committer_capability(&db, space, &facts, None)
        .expect_err("self-leave bundled with an add must still require the capability");
    assert!(err.contains("none presented"), "unexpected error: {err}");
}

#[test]
fn missing_committer_did_is_rejected_when_membership_changing() {
    let db = fresh_db();
    let space = "space-a";
    seed_member(&db, space, "did:key:zTarget");
    let facts = CommitFacts {
        removes: vec![remove_of(1, "did:key:zTarget")],
        committer_did: None,
        ..CommitFacts::default()
    };
    let err = authorize_committer_capability(&db, space, &facts, None)
        .expect_err("an unresolvable committer DID must reject a membership-changing commit");
    assert!(
        err.contains("no resolvable committer DID"),
        "unexpected error: {err}"
    );
}

// ---------------------------------------------------------------------------
// Local-path gate (CodeRabbit finding on PR #781): `remove_member` never
// passes through `decrypt`/`inspect`, so `authorize_local_removal` is the
// only thing gating it.
// ---------------------------------------------------------------------------

#[test]
fn removing_an_active_member_requires_invite_or_higher() {
    let db = fresh_db();
    let space = "space-a";
    seed_member(&db, space, "did:key:zTarget");
    grant_capability(&db, space, "did:key:zReadOnly", "space/read");
    let err = authorize_local_removal(&db, space, "did:key:zReadOnly", "did:key:zTarget")
        .expect_err("read-only committer must not be able to locally kick an active member");
    assert!(
        err.contains("Read") && err.contains("Invite or Admin"),
        "unexpected error: {err}"
    );
}

#[test]
fn removing_an_active_member_with_invite_capability_is_allowed() {
    let db = fresh_db();
    let space = "space-a";
    seed_member(&db, space, "did:key:zTarget");
    grant_capability(&db, space, "did:key:zAdmin", "space/invite");
    let proof_required = authorize_local_removal(&db, space, "did:key:zAdmin", "did:key:zTarget")
        .expect("invite-capable committer must be allowed to locally kick an active member");
    assert!(
        proof_required,
        "removing an active member must report that a receive-side proof is required"
    );
}

#[test]
fn removing_an_already_departed_member_needs_no_capability() {
    // Mirrors the leader-side rekey-after-self-leave flow: the target's
    // `haex_space_members` row is already gone (they left on their own),
    // so this call is only catching up MLS state — no capability required
    // from whichever peer happens to be the elected delivery leader.
    let db = fresh_db();
    let space = "space-a";
    // No seed_member call — target is not (or no longer) an active member.
    let proof_required =
        authorize_local_removal(&db, space, "did:key:zAnyLeader", "did:key:zAlreadyGone")
            .expect("cleaning up a departed member's stale leaf must never require a capability");
    assert!(
        !proof_required,
        "an exempt removal must report that no receive-side proof is required"
    );
}

#[test]
fn removing_an_active_member_with_no_capability_grant_is_rejected() {
    let db = fresh_db();
    let space = "space-a";
    seed_member(&db, space, "did:key:zTarget");
    // No grant_capability call for the committer at all.
    let err = authorize_local_removal(&db, space, "did:key:zNobody", "did:key:zTarget")
        .expect_err("a committer with no capability grant must not be able to kick anyone");
    assert!(
        err.contains("holds no capability"),
        "unexpected error: {err}"
    );
}

#[test]
fn expired_capability_token_does_not_satisfy_local_removal() {
    let db = fresh_db();
    let space = "space-a";
    seed_member(&db, space, "did:key:zTarget");
    // Token exists but expired a long time ago (unix epoch + 1 second).
    grant_capability_expiring_at(&db, space, "did:key:zStale", "space/admin", 1);
    let err = authorize_local_removal(&db, space, "did:key:zStale", "did:key:zTarget")
        .expect_err("an expired capability token must not count");
    assert!(
        err.contains("holds no capability"),
        "unexpected error: {err}"
    );
}

#[test]
fn highest_of_several_capability_tokens_satisfies_local_removal() {
    // A member can hold several orthogonal grants at once (e.g. read +
    // invite from separate claims); the gate must pick the best one.
    let db = fresh_db();
    let space = "space-a";
    seed_member(&db, space, "did:key:zTarget");
    grant_capability(&db, space, "did:key:zMulti", "space/read");
    grant_capability(&db, space, "did:key:zMulti", "space/invite");
    authorize_local_removal(&db, space, "did:key:zMulti", "did:key:zTarget")
        .expect("holding invite alongside read must still satisfy the local removal gate");
}
