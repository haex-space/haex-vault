#[cfg(test)]
mod tests {
    use rusqlite::Connection;
    use uuid::Uuid;

    use crate::owner_sync::scope::resolve_vault_owner_did;

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
}
