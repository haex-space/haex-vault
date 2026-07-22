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

struct TestMls {
    alice: MlsManager,
    space_id: String,
}

impl TestMls {
    fn new() -> Self {
        let alice = MlsManager::new(fresh_mls_conn());
        alice
            .init_identity("did:key:zALICE")
            .expect("init_identity");
        let space_id = "space-test-1".to_string();
        alice.create_group(&space_id).expect("create_group");
        Self { alice, space_id }
    }

    /// Build a KeyPackage carrying `did` as its credential, using a fresh in-memory
    /// MLS provider so we can pretend to be a *different* identity from the group owner.
    fn build_key_package(&self, did: &str) -> Vec<u8> {
        let bob = MlsManager::new(fresh_mls_conn());
        bob.init_identity(did).expect("init_identity");
        bob.generate_key_packages(1)
            .expect("generate_key_packages")
            .pop()
            .expect("at least one key package")
    }
}

#[test]
fn add_member_rejects_credential_did_mismatch() {
    let h = TestMls::new();
    let evil_kp = h.build_key_package("did:key:zEVIL");
    let err = h
        .alice
        .add_member(&h.space_id, &evil_kp, "did:key:zGOOD")
        .expect_err("add_member must reject a KeyPackage whose credential DID does not match");
    assert!(
        format!("{err}").contains("credential DID mismatch"),
        "expected mismatch error, got: {err}"
    );
}

#[test]
fn add_member_accepts_matching_credential() {
    let h = TestMls::new();
    let good_kp = h.build_key_package("did:key:zGOOD");
    h.alice
        .add_member(&h.space_id, &good_kp, "did:key:zGOOD")
        .expect("add_member must accept a KeyPackage whose credential DID matches");
}
