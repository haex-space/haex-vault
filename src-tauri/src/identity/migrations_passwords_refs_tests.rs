//! Migration tests for `0026_passwords_refs` (spec 036, data-model.md, research R6): a genesis vault
//! and a vault upgraded from the schema before it both end with `haex_passwords_passkey_links`,
//! tracked by haex-crdt, with its two indexes and no UNIQUE constraint; a link goes with its entry
//! or its passkey, and opening the upgraded vault again changes nothing.

// These tests read `sqlite_master` and `pragma_*` and write rows directly, which the CRDT write
// path does not expose.
#![allow(clippy::disallowed_methods)]

use haex_crdt::rusqlite::params;
use haex_crdt::Database;

use super::migrations_tests::{migration_source_before, open, own_columns, table_exists};
use crate::identity::haex_migration_source;

const TABLE: &str = "haex_passwords_passkey_links";

fn count(db: &Database, sql: &str) -> i64 {
    db.with_connection(|conn| Ok(conn.query_row(sql, [], |r| r.get::<_, i64>(0))?))
        .expect("count")
}

fn index_names(db: &Database) -> Vec<String> {
    db.with_connection(|conn| {
        let mut stmt = conn.prepare(
            "SELECT name FROM sqlite_master WHERE type = 'index' AND tbl_name = ?1 \
             AND name NOT LIKE 'sqlite_autoindex_%' ORDER BY name",
        )?;
        let rows = stmt.query_map(params![TABLE], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    })
    .expect("index names")
}

fn assert_links_schema(db: &Database) {
    assert!(table_exists(db, TABLE), "{TABLE} must exist after 0026");
    assert_eq!(
        own_columns(db, TABLE),
        ["id", "item_id", "passkey_id", "created_at"],
        "columns of {TABLE}"
    );
    assert_eq!(
        index_names(db),
        [
            "idx_haex_passwords_passkey_links_item_id",
            "idx_haex_passwords_passkey_links_passkey_id",
        ]
    );
    assert_eq!(
        count(
            db,
            "SELECT COUNT(*) FROM pragma_index_list('haex_passwords_passkey_links') \
             WHERE \"unique\" = 1 AND origin IN ('c', 'u')"
        ),
        0,
        "a UNIQUE constraint would halt the sync on a conflict (034 research R2)"
    );
    assert_eq!(
        count(
            db,
            "SELECT COUNT(*) FROM pragma_table_info('haex_passwords_passkey_links') \
             WHERE name = 'haex_hlc_no_sync'"
        ),
        1,
        "links are synced, so haex-crdt must track the table"
    );
}

/// Two entries, a passkey at the first and a link from the second to it.
fn insert_link(db: &Database) {
    db.with_connection(|conn| {
        conn.execute(
            "INSERT INTO haex_passwords_item_details (id) VALUES ('source'), ('target')",
            [],
        )?;
        conn.execute(
            "INSERT INTO haex_passwords_passkeys \
             (id, item_id, credential_id, relying_party_id, user_handle, private_key, public_key) \
             VALUES ('pk', 'source', 'Y3JlZA==', 'example.com', 'dXNlcg', 'key', 'pub')",
            [],
        )?;
        conn.execute(
            "INSERT INTO haex_passwords_passkey_links (id, item_id, passkey_id) \
             VALUES ('link', 'target', 'pk')",
            [],
        )?;
        Ok(())
    })
    .expect("insert a link");
}

#[tokio::test]
async fn migration_0026_gives_a_fresh_vault_the_link_table() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = open(
            dir.path(),
            "passkey-links-fresh",
            true,
            haex_migration_source(),
        );
        assert_links_schema(&db);
    })
    .await
    .expect("join");
}

#[tokio::test]
async fn migration_0026_upgrades_a_vault_from_before_it_and_a_second_open_changes_nothing() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let old = open(
            dir.path(),
            "passkey-links-upgrade",
            true,
            migration_source_before("0026_passwords_refs"),
        );
        assert!(table_exists(&old, "haex_passwords_passkeys"));
        assert!(!table_exists(&old, TABLE));
        drop(old);

        let db = open(
            dir.path(),
            "passkey-links-upgrade",
            false,
            haex_migration_source(),
        );
        assert_links_schema(&db);
        insert_link(&db);
        drop(db);

        let again = open(
            dir.path(),
            "passkey-links-upgrade",
            false,
            haex_migration_source(),
        );
        assert_links_schema(&again);
        assert_eq!(
            count(&again, "SELECT COUNT(*) FROM haex_passwords_passkey_links"),
            1
        );
    })
    .await
    .expect("join");
}

#[tokio::test]
async fn a_link_goes_with_its_passkey_or_its_entry() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = open(
            dir.path(),
            "passkey-links-cascade",
            true,
            haex_migration_source(),
        );
        insert_link(&db);
        db.with_connection(|conn| {
            conn.execute("DELETE FROM haex_passwords_passkeys WHERE id = 'pk'", [])?;
            Ok(())
        })
        .expect("delete the passkey");
        assert_eq!(
            count(&db, "SELECT COUNT(*) FROM haex_passwords_passkey_links"),
            0,
            "deleting the passkey removes its links"
        );

        db.with_connection(|conn| {
            conn.execute(
                "INSERT INTO haex_passwords_passkeys \
                 (id, item_id, credential_id, relying_party_id, user_handle, private_key, \
                  public_key) \
                 VALUES ('pk', 'source', 'Y3JlZA==', 'example.com', 'dXNlcg', 'key', 'pub')",
                [],
            )?;
            conn.execute(
                "INSERT INTO haex_passwords_passkey_links (id, item_id, passkey_id) \
                 VALUES ('link', 'target', 'pk')",
                [],
            )?;
            conn.execute(
                "DELETE FROM haex_passwords_item_details WHERE id = 'target'",
                [],
            )?;
            Ok(())
        })
        .expect("delete the target");
        assert_eq!(
            count(&db, "SELECT COUNT(*) FROM haex_passwords_passkey_links"),
            0,
            "deleting the target entry removes its links"
        );
    })
    .await
    .expect("join");
}
