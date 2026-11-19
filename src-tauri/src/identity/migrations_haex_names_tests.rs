//! Migration tests for `0065_haex_names` (spec 054): a vault from before the rename keeps its
//! contacts and their addresses under the new names, the maintenance table has its new name, a vault
//! created after the rename ends the same, and the migration runs once.

// These tests write rows and read `sqlite_master` directly, which the CRDT write path does not
// expose.
#![allow(clippy::disallowed_methods)]

use haex_crdt::rusqlite::params;
use haex_crdt::Database;

use super::migrations_tests::{migration_source_before, open, table_exists};
use crate::identity::haex_migration_source;

const MIGRATION: &str = "0065_haex_names";
const NIL_DEVICE: &str = "00000000-0000-0000-0000-000000000000";

fn origin_kind(db: &Database, id: &str) -> String {
    db.with_connection(|conn| {
        Ok(conn.query_row(
            "SELECT origin_kind FROM haex_contacts WHERE id = ?1",
            params![id],
            |r| r.get::<_, String>(0),
        )?)
    })
    .expect("origin kind")
}

fn value_kinds(db: &Database, contact: &str) -> Vec<String> {
    db.with_connection(|conn| {
        let mut statement = conn.prepare(
            "SELECT kind FROM haex_contacts_values WHERE contact_id = ?1 ORDER BY position",
        )?;
        let rows = statement.query_map(params![contact], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    })
    .expect("value kinds")
}

#[tokio::test]
async fn a_vault_from_before_the_rename_gets_the_new_names() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let old = open(dir.path(), "names-old", true, migration_source_before(MIGRATION));
        assert!(table_exists(&old, "holzi_maintenance_no_sync"));
        old.write(|tx| {
            tx.execute(
                "INSERT INTO haex_contacts (id, origin_kind, origin_device, origin_caller, \
                 created_at, updated_at) VALUES ('made-here', 'holzi', ?1, 'user', 't', 't'), \
                 ('from-a-card', 'card', ?1, 'user', 't', 't')",
                &[&NIL_DEVICE],
            )?;
            for (position, (id, kind)) in [
                ("v1", "holzi_vault"),
                ("v2", "holzi_nostr_relay"),
                ("v3", "holzi_iroh"),
                ("v4", "email"),
            ]
            .into_iter()
            .enumerate()
            {
                tx.execute(
                    "INSERT INTO haex_contacts_values (id, contact_id, kind, label, value, position) \
                     VALUES (?1, 'made-here', ?2, 'other', 'x', ?3)",
                    params![id, kind, position as i64],
                )?;
            }
            Ok(())
        })
        .expect("write the old rows");
        drop(old);

        let db = open(dir.path(), "names-old", false, haex_migration_source());
        assert!(!table_exists(&db, "holzi_maintenance_no_sync"));
        assert!(table_exists(&db, "haex_maintenance_no_sync"));
        assert_eq!(origin_kind(&db, "made-here"), "haex");
        assert_eq!(origin_kind(&db, "from-a-card"), "card", "other origins stay");
        assert_eq!(
            value_kinds(&db, "made-here"),
            ["haex_vault", "haex_nostr_relay", "haex_iroh", "email"]
        );
        drop(db);

        // The migration runs once: opening the migrated vault again changes nothing.
        let again = open(dir.path(), "names-old", false, haex_migration_source());
        assert_eq!(origin_kind(&again, "made-here"), "haex");
    })
    .await
    .expect("join");
}

#[tokio::test]
async fn a_vault_created_after_the_rename_has_only_the_new_names() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = open(dir.path(), "names-new", true, haex_migration_source());
        assert!(table_exists(&db, "haex_maintenance_no_sync"));
        assert!(!table_exists(&db, "holzi_maintenance_no_sync"));
        let own_kind: String = db
            .with_connection(|conn| {
                Ok(conn.query_row(
                    "SELECT origin_kind FROM haex_contacts WHERE origin_caller = 'internal:self'",
                    [],
                    |r| r.get(0),
                )?)
            })
            .expect("the own contact");
        assert_eq!(own_kind, "haex", "the own contact is made in the app");
    })
    .await
    .expect("join");
}
