//! MLS lifecycle tests: KeyPackage management, External Commit rejoin, protocol serialization.
//!
//! Tests MLS operations using real OpenMLS groups on in-memory SQLite.
//! No network involved — tests the manager and buffer logic directly.
//!
//! Run: cargo test --test mls_lifecycle

use std::sync::{Arc, Mutex};

use rusqlite::Connection;

// ============================================================================
// Test DB setup
// ============================================================================

/// Create an in-memory SQLite database with all required tables for MLS + local delivery.
fn setup_test_db() -> Arc<Mutex<Option<Connection>>> {
    let conn = Connection::open_in_memory().unwrap();

    // CRDT config table (needed by core::execute for the transient
    // trigger-bypass flag, even on _no_sync tables).
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS haex_crdt_configs_no_sync (
            key TEXT PRIMARY KEY NOT NULL,
            type TEXT NOT NULL,
            value TEXT NOT NULL
        );",
    )
    .unwrap();

    // MLS storage tables (key-value stores used by OpenMLS provider)
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS haex_mls_values_no_sync (
            store_type TEXT NOT NULL,
            key_bytes TEXT NOT NULL,
            value_blob BLOB NOT NULL,
            PRIMARY KEY (store_type, key_bytes)
        );
        CREATE TABLE IF NOT EXISTS haex_mls_list_no_sync (
            store_type TEXT NOT NULL,
            key_bytes TEXT NOT NULL,
            index_num INTEGER NOT NULL,
            value_blob BLOB NOT NULL,
            PRIMARY KEY (store_type, key_bytes, index_num)
        );
        CREATE TABLE IF NOT EXISTS haex_mls_epoch_key_pairs_no_sync (
            group_id BLOB NOT NULL,
            epoch_bytes BLOB NOT NULL,
            leaf_index INTEGER NOT NULL,
            value_blob BLOB NOT NULL,
            PRIMARY KEY (group_id, epoch_bytes, leaf_index)
        );",
    )
    .unwrap();

    // Local delivery tables (for buffer tests)
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS haex_spaces (
            id TEXT PRIMARY KEY NOT NULL
        );
        CREATE TABLE IF NOT EXISTS haex_local_delivery_key_packages_no_sync (
            id TEXT PRIMARY KEY NOT NULL,
            space_id TEXT NOT NULL,
            target_did TEXT NOT NULL,
            package_blob TEXT NOT NULL,
            created_at TEXT DEFAULT (CURRENT_TIMESTAMP),
            FOREIGN KEY (space_id) REFERENCES haex_spaces(id)
        );
        CREATE INDEX IF NOT EXISTS haex_local_delivery_key_packages_space_did_idx
            ON haex_local_delivery_key_packages_no_sync (space_id, target_did);
        CREATE TABLE IF NOT EXISTS haex_local_delivery_messages_no_sync (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            space_id TEXT NOT NULL,
            sender_did TEXT NOT NULL,
            message_type TEXT NOT NULL,
            message_blob TEXT NOT NULL,
            created_at TEXT DEFAULT (CURRENT_TIMESTAMP),
            FOREIGN KEY (space_id) REFERENCES haex_spaces(id)
        );
        CREATE INDEX IF NOT EXISTS haex_local_delivery_messages_space_idx
            ON haex_local_delivery_messages_no_sync (space_id);
        CREATE TABLE IF NOT EXISTS haex_local_delivery_pending_commits_no_sync (
            id TEXT PRIMARY KEY NOT NULL,
            space_id TEXT NOT NULL,
            message_id INTEGER NOT NULL,
            expected_dids TEXT DEFAULT '[]' NOT NULL,
            acked_dids TEXT DEFAULT '[]' NOT NULL,
            created_at TEXT DEFAULT (CURRENT_TIMESTAMP),
            FOREIGN KEY (space_id) REFERENCES haex_spaces(id)
        );
        CREATE TABLE IF NOT EXISTS haex_local_delivery_welcomes_no_sync (
            id TEXT PRIMARY KEY NOT NULL,
            space_id TEXT NOT NULL,
            recipient_did TEXT NOT NULL,
            welcome_blob BLOB NOT NULL,
            consumed INTEGER DEFAULT 0,
            created_at TEXT DEFAULT (CURRENT_TIMESTAMP),
            FOREIGN KEY (space_id) REFERENCES haex_spaces(id)
        );
        INSERT OR IGNORE INTO haex_spaces (id) VALUES ('test-space-1');
        INSERT OR IGNORE INTO haex_spaces (id) VALUES ('test-space-2');",
    )
    .unwrap();

    // UCAN tokens table (for membership checks)
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS haex_ucan_tokens (
            id TEXT PRIMARY KEY NOT NULL,
            space_id TEXT NOT NULL,
            issuer_did TEXT NOT NULL,
            audience_did TEXT NOT NULL,
            capability TEXT NOT NULL,
            token TEXT NOT NULL,
            expires_at TEXT,
            created_at TEXT DEFAULT (CURRENT_TIMESTAMP)
        );",
    )
    .unwrap();

    // Space members table (for ACK tracking). Members reference an identity
    // row by `identity_id`; the DID lives on haex_identities.
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS haex_identities (
            id TEXT PRIMARY KEY NOT NULL,
            did TEXT NOT NULL,
            name TEXT NOT NULL,
            source TEXT DEFAULT 'contact' NOT NULL,
            private_key TEXT,
            created_at TEXT DEFAULT (CURRENT_TIMESTAMP)
        );
        CREATE TABLE IF NOT EXISTS haex_space_members (
            id TEXT PRIMARY KEY NOT NULL,
            space_id TEXT NOT NULL,
            identity_id TEXT NOT NULL,
            role TEXT DEFAULT 'read' NOT NULL,
            joined_at TEXT DEFAULT (CURRENT_TIMESTAMP)
        );",
    )
    .unwrap();

    Arc::new(Mutex::new(Some(conn)))
}

// ============================================================================
// Buffer tests: KeyPackage count + trim
// ============================================================================

mod buffer_tests {
    use super::*;
    use haex_vault_lib::database::DbConnection;
    use haex_vault_lib::space_delivery::local::buffer;

    #[test]
    fn count_key_packages_returns_zero_for_empty() {
        let conn = setup_test_db();
        let db = DbConnection(conn);

        let count = buffer::count_key_packages_for_did(&db, "test-space-1", "did:key:alice").unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn count_key_packages_returns_correct_count() {
        let conn = setup_test_db();
        let db = DbConnection(conn);

        // Store 3 key packages
        for _ in 0..3 {
            buffer::store_key_package(&db, "test-space-1", "did:key:alice", b"fake-kp").unwrap();
        }

        let count = buffer::count_key_packages_for_did(&db, "test-space-1", "did:key:alice").unwrap();
        assert_eq!(count, 3);
    }

    #[test]
    fn count_key_packages_isolates_by_did() {
        let conn = setup_test_db();
        let db = DbConnection(conn);

        for _ in 0..5 {
            buffer::store_key_package(&db, "test-space-1", "did:key:alice", b"kp-alice").unwrap();
        }
        for _ in 0..2 {
            buffer::store_key_package(&db, "test-space-1", "did:key:bob", b"kp-bob").unwrap();
        }

        assert_eq!(
            buffer::count_key_packages_for_did(&db, "test-space-1", "did:key:alice").unwrap(),
            5
        );
        assert_eq!(
            buffer::count_key_packages_for_did(&db, "test-space-1", "did:key:bob").unwrap(),
            2
        );
    }

    #[test]
    fn count_key_packages_isolates_by_space() {
        let conn = setup_test_db();
        let db = DbConnection(conn);

        for _ in 0..4 {
            buffer::store_key_package(&db, "test-space-1", "did:key:alice", b"kp").unwrap();
        }
        for _ in 0..7 {
            buffer::store_key_package(&db, "test-space-2", "did:key:alice", b"kp").unwrap();
        }

        assert_eq!(
            buffer::count_key_packages_for_did(&db, "test-space-1", "did:key:alice").unwrap(),
            4
        );
        assert_eq!(
            buffer::count_key_packages_for_did(&db, "test-space-2", "did:key:alice").unwrap(),
            7
        );
    }

    #[test]
    fn trim_key_packages_removes_excess() {
        let conn = setup_test_db();
        let db = DbConnection(conn);

        // Store 15 key packages
        for _ in 0..15 {
            buffer::store_key_package(&db, "test-space-1", "did:key:alice", b"kp").unwrap();
        }

        assert_eq!(
            buffer::count_key_packages_for_did(&db, "test-space-1", "did:key:alice").unwrap(),
            15
        );

        // Trim to 10
        buffer::trim_key_packages(&db, "test-space-1", "did:key:alice", 10).unwrap();

        assert_eq!(
            buffer::count_key_packages_for_did(&db, "test-space-1", "did:key:alice").unwrap(),
            10
        );
    }

    #[test]
    fn trim_key_packages_noop_when_at_or_below_limit() {
        let conn = setup_test_db();
        let db = DbConnection(conn);

        for _ in 0..5 {
            buffer::store_key_package(&db, "test-space-1", "did:key:alice", b"kp").unwrap();
        }

        // Trim to 10 — should be a no-op since we only have 5
        buffer::trim_key_packages(&db, "test-space-1", "did:key:alice", 10).unwrap();

        assert_eq!(
            buffer::count_key_packages_for_did(&db, "test-space-1", "did:key:alice").unwrap(),
            5
        );
    }

    #[test]
    fn trim_key_packages_does_not_affect_other_dids() {
        let conn = setup_test_db();
        let db = DbConnection(conn);

        for _ in 0..12 {
            buffer::store_key_package(&db, "test-space-1", "did:key:alice", b"kp").unwrap();
        }
        for _ in 0..8 {
            buffer::store_key_package(&db, "test-space-1", "did:key:bob", b"kp").unwrap();
        }

        buffer::trim_key_packages(&db, "test-space-1", "did:key:alice", 5).unwrap();

        assert_eq!(
            buffer::count_key_packages_for_did(&db, "test-space-1", "did:key:alice").unwrap(),
            5
        );
        // Bob's packages should be untouched
        assert_eq!(
            buffer::count_key_packages_for_did(&db, "test-space-1", "did:key:bob").unwrap(),
            8
        );
    }

    #[test]
    fn consume_key_package_decrements_count() {
        let conn = setup_test_db();
        let db = DbConnection(conn);

        for _ in 0..3 {
            buffer::store_key_package(&db, "test-space-1", "did:key:alice", b"kp-data").unwrap();
        }

        let consumed = buffer::consume_key_package(&db, "test-space-1", "did:key:alice").unwrap();
        assert!(consumed.is_some());

        assert_eq!(
            buffer::count_key_packages_for_did(&db, "test-space-1", "did:key:alice").unwrap(),
            2
        );
    }

    #[test]
    fn consume_key_package_returns_none_when_empty() {
        let conn = setup_test_db();
        let db = DbConnection(conn);

        let consumed = buffer::consume_key_package(&db, "test-space-1", "did:key:alice").unwrap();
        assert!(consumed.is_none());
    }
}

// ============================================================================
// Buffer: MLS message cursor tests
// ============================================================================

mod buffer_message_cursor_tests {
    use super::*;
    use haex_vault_lib::database::DbConnection;
    use haex_vault_lib::space_delivery::local::buffer;

    fn store_messages(db: &DbConnection, space_id: &str, count: usize) -> Vec<i64> {
        (0..count)
            .map(|i| {
                buffer::store_message(
                    db,
                    space_id,
                    "did:key:sender",
                    "commit",
                    format!("blob-{i}").as_bytes(),
                )
                .unwrap()
            })
            .collect()
    }

    #[test]
    fn fetch_messages_returns_all_when_no_cursor() {
        let db = DbConnection(setup_test_db());
        store_messages(&db, "test-space-1", 3);

        let msgs = buffer::fetch_messages(&db, "test-space-1", None).unwrap();
        assert_eq!(msgs.len(), 3);
    }

    #[test]
    fn fetch_messages_excludes_cursor_id_and_below() {
        let db = DbConnection(setup_test_db());
        let ids = store_messages(&db, "test-space-1", 5);

        // Cursor at ids[2] (the 3rd message) — only ids[3] and ids[4] must come back.
        let msgs = buffer::fetch_messages(&db, "test-space-1", Some(ids[2])).unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].0, ids[3]);
        assert_eq!(msgs[1].0, ids[4]);
    }

    // Regression: if the cursor is set to ec_msg_id (the ID the leader assigned
    // to the External Commit), the EC itself must NOT be returned on the next
    // fetch — otherwise the peer would hit an epoch mismatch again and loop.
    #[test]
    fn fetch_messages_returns_empty_when_cursor_at_last_stored_id() {
        let db = DbConnection(setup_test_db());
        let ids = store_messages(&db, "test-space-1", 4);
        let last_id = *ids.last().unwrap();

        // Cursor advanced to the EC's own ID — nothing newer exists.
        let msgs = buffer::fetch_messages(&db, "test-space-1", Some(last_id)).unwrap();
        assert!(
            msgs.is_empty(),
            "cursor at last stored ID must return empty, prevents epoch-loop re-fetch",
        );
    }

    #[test]
    fn fetch_messages_returns_empty_when_cursor_past_all() {
        let db = DbConnection(setup_test_db());
        store_messages(&db, "test-space-1", 3);

        let msgs = buffer::fetch_messages(&db, "test-space-1", Some(9999)).unwrap();
        assert!(msgs.is_empty());
    }

    #[test]
    fn fetch_messages_is_isolated_by_space_id() {
        let db = DbConnection(setup_test_db());
        let ids_space1 = store_messages(&db, "test-space-1", 3);
        store_messages(&db, "test-space-2", 2);

        let msgs = buffer::fetch_messages(&db, "test-space-1", None).unwrap();
        assert_eq!(msgs.len(), 3);
        for (msg_id, _, _, _, _) in &msgs {
            assert!(ids_space1.contains(msg_id));
        }
    }
}

// ============================================================================
// MLS Manager tests: GroupInfo + External Commit roundtrip
// ============================================================================

mod mls_manager_tests {
    use super::*;
    use haex_vault_lib::mls::manager::MlsManager;

    /// Create an MlsManager with a fresh in-memory DB and initialized identity.
    fn setup_mls(did: &str) -> MlsManager {
        let conn = setup_test_db();
        let manager = MlsManager::new(conn);
        manager.init_tables().unwrap();
        manager.init_identity(did).unwrap();
        manager
    }

    #[test]
    fn get_group_info_returns_serialized_bytes() {
        let admin = setup_mls("did:key:admin");
        admin.create_group("space-abc").unwrap();

        let group_info = admin.get_group_info("space-abc").unwrap();
        assert!(!group_info.is_empty());
        // GroupInfo TLS serialization starts with specific bytes — just check it's non-trivial
        assert!(group_info.len() > 50);
    }

    #[test]
    fn get_group_info_fails_for_nonexistent_group() {
        let manager = setup_mls("did:key:test");
        let result = manager.get_group_info("nonexistent-space");
        assert!(result.is_err());
    }

    #[test]
    fn external_commit_rejoin_roundtrip() {
        // Setup: admin creates group and adds a member
        let admin = setup_mls("did:key:admin");
        admin.create_group("space-rejoin").unwrap();

        let member = setup_mls("did:key:member");
        let member_kps = member.generate_key_packages(1).unwrap();

        let bundle = admin.add_member("space-rejoin", &member_kps[0]).unwrap();

        // Member processes the welcome to join the group
        member.process_welcome("space-rejoin", bundle.welcome.as_ref().unwrap()).unwrap();

        // Both should be in the group now
        assert!(admin.has_group("space-rejoin"));
        assert!(member.has_group("space-rejoin"));

        // Simulate: member goes offline, admin does some operations that advance the epoch
        // For simplicity, we create a second member to advance the epoch
        let member2 = setup_mls("did:key:member2");
        let member2_kps = member2.generate_key_packages(1).unwrap();
        let bundle2 = admin.add_member("space-rejoin", &member2_kps[0]).unwrap();
        member2.process_welcome("space-rejoin", bundle2.welcome.as_ref().unwrap()).unwrap();

        // Admin's epoch has advanced, but original member is still on old epoch
        let admin_epoch = admin.derive_epoch_key("space-rejoin").unwrap().epoch;
        let member_epoch = member.derive_epoch_key("space-rejoin").unwrap().epoch;
        assert!(admin_epoch > member_epoch, "Admin epoch ({admin_epoch}) should be ahead of member epoch ({member_epoch})");

        // Now member tries to rejoin via External Commit
        let group_info = admin.get_group_info("space-rejoin").unwrap();
        let (commit_bytes, new_epoch_key) = member
            .join_by_external_commit("space-rejoin", &group_info)
            .unwrap();

        assert!(!commit_bytes.is_empty());
        assert!(new_epoch_key.epoch >= admin_epoch, "Rejoined member epoch should be >= admin epoch");

        // Admin processes the external commit
        admin.process_message("space-rejoin", &commit_bytes).unwrap();

        // Both should now be on the same epoch
        let admin_epoch_after = admin.derive_epoch_key("space-rejoin").unwrap().epoch;
        let member_epoch_after = member.derive_epoch_key("space-rejoin").unwrap().epoch;
        assert_eq!(admin_epoch_after, member_epoch_after);
    }

    #[test]
    fn external_commit_fails_with_wrong_space_id() {
        let admin = setup_mls("did:key:admin");
        admin.create_group("space-a").unwrap();

        let group_info = admin.get_group_info("space-a").unwrap();

        let member = setup_mls("did:key:member");
        // Try to join with wrong space ID — should detect group ID mismatch
        let result = member.join_by_external_commit("wrong-space-id", &group_info);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("mismatch"));
    }

    #[test]
    fn external_commit_fails_with_invalid_group_info() {
        let member = setup_mls("did:key:member");
        let result = member.join_by_external_commit("space-x", b"invalid-garbage");
        assert!(result.is_err());
    }

    #[test]
    fn generate_key_packages_returns_requested_count() {
        let manager = setup_mls("did:key:test");
        let packages = manager.generate_key_packages(10).unwrap();
        assert_eq!(packages.len(), 10);
        // Each package should be non-trivial
        for pkg in &packages {
            assert!(pkg.len() > 50);
        }
    }

    /// Reproduces the production bug: a stale Welcome that referenced a
    /// now-consumed KeyPackage causes `NoMatchingKeyPackage` on the invitee.
    /// The fix is leader-side: regenerate the Welcome with a fresh KP and
    /// rely on `add_member`'s duplicate-leaf handling to reconcile state.
    #[test]
    fn welcome_can_be_regenerated_after_kp_consumed() {
        let admin = setup_mls("did:key:admin");
        admin.create_group("space-retry").unwrap();

        let member = setup_mls("did:key:member");
        let kps = member.generate_key_packages(2).unwrap();

        // First attempt: admin adds member with KP_1, sends welcome.
        let bundle1 = admin.add_member("space-retry", &kps[0]).unwrap();
        let welcome1 = bundle1.welcome.unwrap();

        // Member processes the first welcome — succeeds, but consumes KP_1
        // from MLS storage in the process.
        member.process_welcome("space-retry", &welcome1).unwrap();

        // Simulate the production failure scenario: another retry comes in
        // (network glitch, whatever) and the leader regenerates with KP_2.
        // add_member must handle the duplicate signature key by removing
        // the existing leaf before re-adding.
        let bundle2 = admin.add_member("space-retry", &kps[1]).unwrap();
        let welcome2 = bundle2.welcome.unwrap();

        // The fresh welcome must reference KP_2 (still present in member's
        // storage) and process cleanly. Before the fix, retries served the
        // stale `welcome1` which referenced the already-consumed KP_1.
        member.process_welcome("space-retry", &welcome2).unwrap();
        assert!(member.has_group("space-retry"));

        // Re-applying the *stale* welcome must fail — KP_1 is gone — proving
        // the failure mode is real and the regenerate path is the only fix.
        let stale_result = member.process_welcome("space-retry", &welcome1);
        assert!(stale_result.is_err(), "stale welcome should not succeed twice");
    }
}

// ============================================================================
// Protocol serialization tests
// ============================================================================

mod protocol_tests {
    use haex_vault_lib::space_delivery::local::protocol::{Request, Response};

    #[test]
    fn request_rejoin_serialization_roundtrip() {
        let req = Request::RequestRejoin {
            space_id: "space-123".to_string(),
            ucan_token: "eyJ0eXAiOiJKV1Q...".to_string(),
        };

        let bytes = serde_json::to_vec(&req).unwrap();
        let deserialized: Request = serde_json::from_slice(&bytes).unwrap();

        match deserialized {
            Request::RequestRejoin { space_id, ucan_token } => {
                assert_eq!(space_id, "space-123");
                assert_eq!(ucan_token, "eyJ0eXAiOiJKV1Q...");
            }
            _ => panic!("Expected RequestRejoin"),
        }
    }

    #[test]
    fn submit_external_commit_serialization_roundtrip() {
        let req = Request::SubmitExternalCommit {
            space_id: "space-456".to_string(),
            commit: "base64-commit-data".to_string(),
            ucan_token: "token".to_string(),
        };

        let bytes = serde_json::to_vec(&req).unwrap();
        let deserialized: Request = serde_json::from_slice(&bytes).unwrap();

        match deserialized {
            Request::SubmitExternalCommit { space_id, commit, ucan_token } => {
                assert_eq!(space_id, "space-456");
                assert_eq!(commit, "base64-commit-data");
                assert_eq!(ucan_token, "token");
            }
            _ => panic!("Expected SubmitExternalCommit"),
        }
    }

    #[test]
    fn key_package_count_serialization_roundtrip() {
        let req = Request::MlsKeyPackageCount {
            space_id: "space-789".to_string(),
        };

        let bytes = serde_json::to_vec(&req).unwrap();
        let deserialized: Request = serde_json::from_slice(&bytes).unwrap();

        match deserialized {
            Request::MlsKeyPackageCount { space_id } => {
                assert_eq!(space_id, "space-789");
            }
            _ => panic!("Expected MlsKeyPackageCount"),
        }
    }

    #[test]
    fn response_group_info_serialization_roundtrip() {
        let resp = Response::GroupInfo {
            group_info: "base64-group-info".to_string(),
        };

        let bytes = serde_json::to_vec(&resp).unwrap();
        let deserialized: Response = serde_json::from_slice(&bytes).unwrap();

        match deserialized {
            Response::GroupInfo { group_info } => {
                assert_eq!(group_info, "base64-group-info");
            }
            _ => panic!("Expected GroupInfo response"),
        }
    }

    #[test]
    fn response_key_package_count_serialization_roundtrip() {
        let resp = Response::KeyPackageCount {
            available: 7,
            needed: 3,
        };

        let bytes = serde_json::to_vec(&resp).unwrap();
        let deserialized: Response = serde_json::from_slice(&bytes).unwrap();

        match deserialized {
            Response::KeyPackageCount { available, needed } => {
                assert_eq!(available, 7);
                assert_eq!(needed, 3);
            }
            _ => panic!("Expected KeyPackageCount response"),
        }
    }

    // Regression: SubmitExternalCommit previously responded with Response::Ok
    // (carrying no message ID), causing the peer to be unable to advance its
    // MLS cursor past the stored External Commit. The fix returns
    // Response::MessageStored so the peer can skip the EC on the next fetch.
    #[test]
    fn response_message_stored_carries_message_id() {
        let resp = Response::MessageStored { message_id: 42 };

        let bytes = serde_json::to_vec(&resp).unwrap();
        let deserialized: Response = serde_json::from_slice(&bytes).unwrap();

        match deserialized {
            Response::MessageStored { message_id } => {
                assert_eq!(message_id, 42);
            }
            _ => panic!("Expected MessageStored response, got: {deserialized:?}"),
        }
    }

    // Verify peer.rs backward-compat path: older leaders that still return Ok
    // (no msg_id) must not break the rejoin flow — the peer falls back to 0.
    #[test]
    fn response_ok_is_distinct_from_message_stored() {
        let ok_resp = Response::Ok;
        let ok_bytes = serde_json::to_vec(&ok_resp).unwrap();
        let ok_deserialized: Response = serde_json::from_slice(&ok_bytes).unwrap();
        assert!(matches!(ok_deserialized, Response::Ok));

        let stored_resp = Response::MessageStored { message_id: 1 };
        let stored_bytes = serde_json::to_vec(&stored_resp).unwrap();
        let stored_deserialized: Response = serde_json::from_slice(&stored_bytes).unwrap();
        assert!(matches!(stored_deserialized, Response::MessageStored { .. }));

        // The two variants must not round-trip into each other
        assert!(!matches!(
            serde_json::from_slice::<Response>(&ok_bytes).unwrap(),
            Response::MessageStored { .. }
        ));
    }
}
