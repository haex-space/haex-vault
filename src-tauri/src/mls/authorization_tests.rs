//! Unit tests for the Phase-1 authorization policy.
//!
//! End-to-end coverage over an actual `MlsManager::decrypt` cycle lives in
//! `manager_tests.rs` (see `decrypt_rejects_add_of_non_member`); these tests
//! synthesize `CommitFacts` directly so each §7 row of the plan can be
//! exercised without spinning up a real openmls group.

use std::sync::{Arc, Mutex};

use rusqlite::Connection;

use super::{authorize, AddFact, CommitFacts, UpdateFact};

/// Minimal schema: only the two tables the addee-membership check joins.
/// `haex_tombstone` mirrors the CRDT convention used by `select_with_crdt`.
const MEMBERSHIP_SQL: &str = "
CREATE TABLE haex_identities (
    id TEXT PRIMARY KEY,
    did TEXT NOT NULL,
    haex_tombstone INTEGER
);
CREATE TABLE haex_space_members (
    id TEXT PRIMARY KEY,
    space_id TEXT NOT NULL,
    identity_id TEXT NOT NULL,
    role TEXT,
    haex_tombstone INTEGER
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
        "INSERT INTO haex_identities (id, did, haex_tombstone) VALUES (?1, ?2, 0)",
        rusqlite::params![identity_id, did],
    )
    .expect("insert identity");
    c.execute(
        "INSERT INTO haex_space_members (id, space_id, identity_id, role, haex_tombstone) VALUES (?1, ?2, ?3, 'member', 0)",
        rusqlite::params![member_id, space_id, identity_id],
    )
    .expect("insert member");
}

fn tombstone_member(conn: &Arc<Mutex<Option<Connection>>>, space_id: &str, did: &str) {
    let guard = conn.lock().unwrap();
    let c = guard.as_ref().unwrap();
    c.execute(
        "UPDATE haex_space_members SET haex_tombstone = 1 \
         WHERE space_id = ?1 \
           AND identity_id IN (SELECT id FROM haex_identities WHERE did = ?2)",
        rusqlite::params![space_id, did],
    )
    .expect("tombstone member");
}

fn add(did: &str) -> AddFact {
    AddFact {
        credential_did: did.to_string(),
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
fn add_of_a_tombstoned_member_is_rejected() {
    let db = fresh_db();
    let space = "space-a";
    seed_member(&db, space, "did:key:zEx");
    tombstone_member(&db, space, "did:key:zEx");
    let facts = CommitFacts {
        adds: vec![add("did:key:zEx")],
        ..CommitFacts::default()
    };
    let err = authorize(&db, space, &facts)
        .expect_err("tombstoned addee must be rejected via IFNULL filter");
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
    // A commit that only carries an update-path leaf with a stable DID and
    // no queued proposals — e.g. a periodic key rotation. Empty facts pass.
    let facts = CommitFacts::default();
    authorize(&db, "space-a", &facts)
        .expect("a commit without adds/removes/updates/unmodelled is a no-op for Phase-1");
}
