use std::sync::{Arc, Mutex};

use rusqlite::Connection;

use super::MlsManager;

/// Schema for the MLS-backing tables. In production these come from the
/// Drizzle-generated migration (`0000_jazzy_chat.sql`); unit tests bring their
/// own copy so we do not need to run the JS-side migration pipeline.
const MLS_TABLES_SQL: &str = "
CREATE TABLE haex_mls_values_no_sync (
    store_type TEXT NOT NULL,
    key_bytes BLOB NOT NULL,
    value_blob BLOB NOT NULL,
    PRIMARY KEY (store_type, key_bytes)
);
CREATE TABLE haex_mls_list_no_sync (
    store_type TEXT NOT NULL,
    key_bytes BLOB NOT NULL,
    index_num INTEGER NOT NULL,
    value_blob BLOB NOT NULL,
    PRIMARY KEY (store_type, key_bytes, index_num)
);
CREATE TABLE haex_mls_epoch_key_pairs_no_sync (
    group_id BLOB NOT NULL,
    epoch_bytes BLOB NOT NULL,
    leaf_index INTEGER NOT NULL,
    value_blob BLOB NOT NULL,
    PRIMARY KEY (group_id, epoch_bytes, leaf_index)
);
";

fn fresh_mls_conn() -> Arc<Mutex<Option<Connection>>> {
    let conn = Connection::open_in_memory().expect("open_in_memory");
    conn.execute_batch(MLS_TABLES_SQL)
        .expect("create MLS tables");
    Arc::new(Mutex::new(Some(conn)))
}

/// A real (identity DID, signing key) pair. PoP verification decodes the
/// credential DID via `did:key`, so test DIDs must be genuine — an arbitrary
/// placeholder string like `did:key:zALICE` no longer round-trips.
struct TestIdentity {
    did: String,
    signing_key: ed25519_dalek::SigningKey,
}

impl TestIdentity {
    fn new() -> Self {
        let signing_key = ed25519_dalek::SigningKey::from_bytes(&rand::random());
        let did = crate::ucan::did_key_from_public_key(&signing_key.verifying_key());
        Self { did, signing_key }
    }
}

struct TestMls {
    alice: MlsManager,
    space_id: String,
}

impl TestMls {
    fn new() -> Self {
        let alice_identity = TestIdentity::new();
        let alice = MlsManager::new(fresh_mls_conn());
        alice
            .init_identity(&alice_identity.did)
            .expect("init_identity");
        let space_id = "space-test-1".to_string();
        alice.create_group(&space_id).expect("create_group");
        Self { alice, space_id }
    }

    /// Build a KeyPackage + PoP carrying `identity.did` as its credential and
    /// signed by `identity.signing_key`, using a fresh in-memory MLS provider
    /// so we can pretend to be a different identity from the group owner.
    fn build_key_package(&self, identity: &TestIdentity) -> (Vec<u8>, Vec<u8>) {
        self.build_key_package_with_pop_signer(&identity.did, &identity.signing_key)
    }

    /// Build a KeyPackage whose credential DID is `credential_did` but whose
    /// PoP is produced by `pop_signer` — pass a key other than the one
    /// `credential_did` resolves to in order to simulate a forged PoP.
    fn build_key_package_with_pop_signer(
        &self,
        credential_did: &str,
        pop_signer: &ed25519_dalek::SigningKey,
    ) -> (Vec<u8>, Vec<u8>) {
        let bob = MlsManager::new(fresh_mls_conn());
        bob.init_identity(credential_did).expect("init_identity");
        bob.generate_key_packages(1, pop_signer)
            .expect("generate_key_packages")
            .pop()
            .expect("at least one key package")
    }
}

#[test]
fn add_member_rejects_credential_did_mismatch() {
    let h = TestMls::new();
    let evil = TestIdentity::new();
    let good = TestIdentity::new();
    let (evil_kp, evil_pop) = h.build_key_package(&evil);
    let err = h
        .alice
        .add_member(&h.space_id, &evil_kp, &good.did, &evil_pop)
        .expect_err("add_member must reject a KeyPackage whose credential DID does not match");
    assert!(
        format!("{err}").contains("credential DID mismatch"),
        "expected mismatch error, got: {err}"
    );
}

#[test]
fn add_member_accepts_matching_credential() {
    let h = TestMls::new();
    let good = TestIdentity::new();
    let (good_kp, good_pop) = h.build_key_package(&good);
    h.alice
        .add_member(&h.space_id, &good_kp, &good.did, &good_pop)
        .expect(
            "add_member must accept a KeyPackage whose credential DID matches and PoP verifies",
        );
}

#[test]
fn add_member_rejects_invalid_pop() {
    let h = TestMls::new();
    let good = TestIdentity::new();
    let attacker = TestIdentity::new();
    let (kp, forged_pop) = h.build_key_package_with_pop_signer(&good.did, &attacker.signing_key);
    let err = h
        .alice
        .add_member(&h.space_id, &kp, &good.did, &forged_pop)
        .expect_err(
            "add_member must reject a PoP signed by a different identity than the credential DID resolves to",
        );
    assert!(
        format!("{err}").contains("proof-of-possession"),
        "expected proof-of-possession error, got: {err}"
    );
}
