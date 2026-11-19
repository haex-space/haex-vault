//! Migration tests for `0027_passwords_owner` (spec 038, data-model.md, research R2): a genesis vault
//! and a vault upgraded from the schema before it both have the column `owner`, and existing entries
//! keep `NULL` (they belong to the user). That haex-crdt tracks the column after an upgrade is
//! tested in `tests/vault_upgrade.rs` (it needs the old trigger version).

// These tests read `sqlite_master` and write rows directly, which the CRDT write path does not
// expose.
#![allow(clippy::disallowed_methods)]

use haex_crdt::Database;

use super::migrations_tests::{column_names, migration_source_before, open};
use crate::identity::haex_migration_source;

const TABLE: &str = "haex_passwords_item_details";

fn update_trigger_sql(db: &Database) -> String {
    db.with_connection(|conn| {
        Ok(conn.query_row(
            "SELECT sql FROM sqlite_master \
             WHERE type = 'trigger' AND name = 'z_dirty_haex_passwords_item_details_update'",
            [],
            |r| r.get::<_, String>(0),
        )?)
    })
    .expect("the update trigger of the entries must exist")
}

#[tokio::test]
async fn migration_0027_gives_a_fresh_vault_the_owner_column() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = open(dir.path(), "owner-fresh", true, haex_migration_source());
        assert!(column_names(&db, TABLE).contains(&"owner".to_string()));
        assert!(
            update_trigger_sql(&db).contains("owner"),
            "owner must be tracked"
        );
    })
    .await
    .expect("join");
}

#[tokio::test]
async fn migration_0027_upgrades_a_vault_and_existing_entries_stay_the_users() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let old = open(
            dir.path(),
            "owner-upgrade",
            true,
            migration_source_before("0027_passwords_owner"),
        );
        old.with_connection(|conn| {
            conn.execute(
                "INSERT INTO haex_passwords_item_details (id, title) VALUES ('old', 'Bank')",
                [],
            )?;
            Ok(())
        })
        .expect("insert an entry before the migration");
        assert!(!column_names(&old, TABLE).contains(&"owner".to_string()));
        drop(old);

        let db = open(dir.path(), "owner-upgrade", false, haex_migration_source());
        assert!(column_names(&db, TABLE).contains(&"owner".to_string()));
        let owner: Option<String> = db
            .with_connection(|conn| {
                Ok(conn.query_row(
                    "SELECT owner FROM haex_passwords_item_details WHERE id = 'old'",
                    [],
                    |r| r.get(0),
                )?)
            })
            .expect("read the owner");
        assert_eq!(
            owner, None,
            "an entry from before the migration belongs to the user"
        );
    })
    .await
    .expect("join");
}
