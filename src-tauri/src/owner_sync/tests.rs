#[cfg(test)]
mod tests {
    use rusqlite::Connection;
    use uuid::Uuid;

    use crate::owner_sync::scope::{
        classify_peer, resolve_vault_owner_did, select_sync_scope, PeerClass,
    };

    /// Create the minimal subset of `haex_identities` + `haex_spaces` the
    /// resolver joins over. Columns mirror the production Drizzle schema
    /// (`src/database/schemas/identity.ts`, `src/database/schemas/spaces.ts`)
    /// for the `NOT NULL` constraints the JOIN depends on; purely cosmetic
    /// columns are omitted.
    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().expect("in-memory DB");
        conn.execute_batch(
            "CREATE TABLE haex_identities (
                id TEXT PRIMARY KEY NOT NULL,
                did TEXT NOT NULL,
                name TEXT NOT NULL,
                source TEXT NOT NULL DEFAULT 'contact',
                private_key TEXT
            );
            CREATE TABLE haex_spaces (
                id TEXT PRIMARY KEY NOT NULL,
                type TEXT NOT NULL DEFAULT 'online',
                status TEXT NOT NULL DEFAULT 'active',
                name TEXT NOT NULL,
                owner_identity_id TEXT NOT NULL
            );",
        )
        .expect("create tables");
        conn
    }

    #[test]
    fn returns_owner_did_of_vault_space() {
        let conn = setup_db();
        let identity_id = Uuid::new_v4().to_string();
        let owner_did = format!("did:key:{}", Uuid::new_v4());

        conn.execute(
            "INSERT INTO haex_identities (id, did, name, source) VALUES (?1, ?2, 'Me', 'own')",
            rusqlite::params![identity_id, owner_did],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO haex_spaces (id, type, name, owner_identity_id) \
             VALUES (?1, 'vault', 'My Vault', ?2)",
            rusqlite::params![Uuid::new_v4().to_string(), identity_id],
        )
        .unwrap();

        let resolved = resolve_vault_owner_did(&conn);
        assert_eq!(resolved, Ok(Some(owner_did)));
    }

    #[test]
    fn returns_none_when_no_vault_space() {
        let conn = setup_db();
        let identity_id = Uuid::new_v4().to_string();
        let some_did = format!("did:key:{}", Uuid::new_v4());

        conn.execute(
            "INSERT INTO haex_identities (id, did, name, source) VALUES (?1, ?2, 'Me', 'own')",
            rusqlite::params![identity_id, some_did],
        )
        .unwrap();
        // A non-vault space owned by the same identity must not match.
        conn.execute(
            "INSERT INTO haex_spaces (id, type, name, owner_identity_id) \
             VALUES (?1, 'online', 'A Shared Space', ?2)",
            rusqlite::params![Uuid::new_v4().to_string(), identity_id],
        )
        .unwrap();

        let resolved = resolve_vault_owner_did(&conn);
        assert_eq!(resolved, Ok(None));
    }

    #[test]
    fn classifies_matching_did_as_owner_device() {
        let owner_did = format!("did:key:{}", Uuid::new_v4());
        assert_eq!(
            classify_peer(&owner_did, &owner_did),
            PeerClass::OwnerDevice
        );
    }

    #[test]
    fn classifies_different_did_as_foreign() {
        let owner_did = format!("did:key:{}", Uuid::new_v4());
        let peer_did = format!("did:key:{}", Uuid::new_v4());
        assert_eq!(classify_peer(&peer_did, &owner_did), PeerClass::Foreign);
    }

    #[test]
    fn classification_is_case_sensitive_and_untrimmed() {
        let owner_did = "did:key:zABC".to_string();
        // Case difference must not match — DIDs are case-sensitive identifiers.
        assert_eq!(
            classify_peer("did:key:zabc", &owner_did),
            PeerClass::Foreign
        );
        // Surrounding whitespace must not be normalized away.
        assert_eq!(
            classify_peer(" did:key:zABC", &owner_did),
            PeerClass::Foreign
        );
    }

    fn tables(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn owner_device_gets_full_table_set() {
        let all = tables(&["haex_passwords", "haex_space_devices"]);
        let space_scoped = tables(&["haex_space_devices"]);
        assert_eq!(
            select_sync_scope(PeerClass::OwnerDevice, &all, &space_scoped),
            all
        );
    }

    #[test]
    fn foreign_gets_only_space_scoped_set() {
        let all = tables(&["haex_passwords", "haex_space_devices"]);
        let space_scoped = tables(&["haex_space_devices"]);
        assert_eq!(
            select_sync_scope(PeerClass::Foreign, &all, &space_scoped),
            space_scoped
        );
    }

    /// Security gate: a foreign peer must never be offered vault-private tables,
    /// while an owner device must receive the full set including them.
    #[test]
    fn foreign_peer_never_receives_vault_private_tables() {
        let private = ["haex_passwords", "haex_vault_settings", "haex_identities"];
        let space_scoped_names = ["haex_space_devices", "haex_space_members"];

        let mut all_names: Vec<&str> = private.to_vec();
        all_names.extend_from_slice(&space_scoped_names);
        let all = tables(&all_names);
        let space_scoped = tables(&space_scoped_names);

        let foreign_scope = select_sync_scope(PeerClass::Foreign, &all, &space_scoped);
        for private_table in private {
            assert!(
                !foreign_scope.contains(&private_table.to_string()),
                "foreign peer must not receive private table {private_table}"
            );
        }

        let owner_scope = select_sync_scope(PeerClass::OwnerDevice, &all, &space_scoped);
        for private_table in private {
            assert!(
                owner_scope.contains(&private_table.to_string()),
                "owner device must receive private table {private_table}"
            );
        }
    }
}
