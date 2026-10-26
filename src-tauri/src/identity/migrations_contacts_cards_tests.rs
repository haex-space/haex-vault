//! Migration tests for `0043_contacts_cards_hash` (spec 060 research R5, data-model.md): the cards
//! of other vaults are keyed by their hash, several may belong to one vault, the table is tracked
//! and has no UNIQUE constraint, and an upgrade drops the rows of the old table on purpose.

// These tests read `sqlite_master` and `pragma_*` directly, which the CRDT write path does not
// expose.
#![allow(clippy::disallowed_methods)]

use haex_crdt::rusqlite::params;
use haex_crdt::Database;

use super::migrations_extensions_tests::{has_hlc_column, unique_indexes};
use super::migrations_tests::{migration_source_before, open, table_exists};
use crate::identity::holzi_migration_source;

const TABLE: &str = "haex_contacts_cards";

fn columns(db: &Database) -> Vec<(String, i64)> {
    db.with_connection(|conn| {
        let mut statement =
            conn.prepare("SELECT name, pk FROM pragma_table_info(?1) ORDER BY cid")?;
        let rows = statement
            .query_map(params![TABLE], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
    .expect("columns")
}

fn indexed(db: &Database, column: &str) -> bool {
    db.with_connection(|conn| {
        Ok(conn.query_row(
            "SELECT COUNT(*) FROM pragma_index_list(?1) l \
             JOIN pragma_index_info(l.name) i WHERE i.name = ?2 AND l.\"unique\" = 0",
            params![TABLE, column],
            |r| r.get::<_, i64>(0),
        )? > 0)
    })
    .expect("indexes")
}

fn assert_shape(db: &Database) {
    assert!(table_exists(db, TABLE));
    assert!(
        has_hlc_column(db, TABLE),
        "the cards reach every own device"
    );
    let columns = columns(db);
    let names: Vec<&str> = columns.iter().map(|(name, _)| name.as_str()).collect();
    for wanted in [
        "hash",
        "vault_pubkey",
        "contact_id",
        "card",
        "issued_at",
        "received_at",
    ] {
        assert!(names.contains(&wanted), "{wanted} column");
    }
    let key: Vec<&str> = columns
        .iter()
        .filter(|(_, pk)| *pk > 0)
        .map(|(name, _)| name.as_str())
        .collect();
    assert_eq!(key, ["hash"], "the hash alone is the primary key");
    assert!(
        indexed(db, "vault_pubkey"),
        "the cards of a vault are found"
    );
    assert!(
        indexed(db, "contact_id"),
        "the cards of a contact are found"
    );
    assert_eq!(
        unique_indexes(db, TABLE),
        0,
        "a UNIQUE constraint would halt the sync"
    );
}

fn insert_contact(db: &Database, id: &str) {
    db.write(|tx| {
        tx.execute(
            "INSERT INTO haex_contacts (id, given_name, origin_kind, origin_device, \
             origin_caller, created_at, updated_at) \
             VALUES (?1, 'Anna', 'holzi', 'd', 'user', 't', 't')",
            &[&id],
        )?;
        Ok(())
    })
    .expect("insert a contact");
}

fn count(db: &Database) -> i64 {
    db.with_connection(|conn| {
        Ok(
            conn.query_row(&format!("SELECT COUNT(*) FROM {TABLE}"), [], |r| {
                r.get::<_, i64>(0)
            })?,
        )
    })
    .expect("count")
}

#[tokio::test]
async fn migration_0043_gives_a_fresh_vault_the_hash_keyed_table() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = open(dir.path(), "cards-fresh", true, holzi_migration_source());
        assert_shape(&db);
    })
    .await
    .expect("join");
}

#[tokio::test]
async fn migration_0043_drops_the_rows_of_the_old_table_on_purpose() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let old = open(
            dir.path(),
            "cards-upgrade",
            true,
            migration_source_before("0043_contacts_cards_hash"),
        );
        insert_contact(&old, "c1");
        old.write(|tx| {
            tx.execute(
                "INSERT INTO haex_contacts_cards (vault_pubkey, contact_id, card, issued_at, \
                 received_at) VALUES ('pk', 'c1', x'00', 't', 't')",
                &[],
            )?;
            Ok(())
        })
        .expect("insert an old-shape card");
        assert_eq!(count(&old), 1);
        drop(old);
        let db = open(dir.path(), "cards-upgrade", false, holzi_migration_source());
        assert_shape(&db);
        assert_eq!(count(&db), 0, "the old rows cannot get a hash in SQL");
    })
    .await
    .expect("join");
}

#[tokio::test]
async fn several_cards_of_one_vault_are_several_rows_and_follow_their_contact() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = open(dir.path(), "cards-many", true, holzi_migration_source());
        insert_contact(&db, "c1");
        db.write(|tx| {
            for hash in ["h1", "h2"] {
                tx.execute(
                    "INSERT INTO haex_contacts_cards (hash, vault_pubkey, contact_id, card, \
                     issued_at, received_at) VALUES (?1, 'pk', 'c1', x'00', 't', 't')",
                    &[&hash],
                )?;
            }
            Ok(())
        })
        .expect("two cards of one vault");
        assert_eq!(count(&db), 2);
        db.write(|tx| {
            tx.execute("DELETE FROM haex_contacts WHERE id = 'c1'", &[])?;
            Ok(())
        })
        .expect("delete the contact");
        assert_eq!(count(&db), 0, "the cards go with their contact");
    })
    .await
    .expect("join");
}
