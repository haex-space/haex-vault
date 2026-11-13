//! Migration tests for `0061_passwords_pass` (spec 092, data-model.md, research R1): a fresh vault and
//! a vault upgraded from the schema before it both have the `kind` column and the two pass tables,
//! and every entry that exists keeps `kind = 'login'`. That haex-crdt tracks the new column after an
//! upgrade is tested in `tests/vault_upgrade.rs` (it needs the old trigger version).

// These tests read `sqlite_master` and write rows directly, which the CRDT write path does not
// expose.
#![allow(clippy::disallowed_methods)]

use haex_crdt::Database;

use super::migrations_tests::{column_names, migration_source_before, open, table_exists};
use crate::identity::holzi_migration_source;

const ITEMS: &str = "haex_passwords_item_details";
const DETAILS: &str = "haex_passwords_pass_details";
const BARCODES: &str = "haex_passwords_pass_barcodes";

fn sql_of(db: &Database, kind: &str, name: &str) -> Option<String> {
    db.with_connection(|conn| {
        let mut stmt =
            conn.prepare("SELECT sql FROM sqlite_master WHERE type = ?1 AND name = ?2")?;
        let mut rows = stmt.query(haex_crdt::rusqlite::params![kind, name])?;
        Ok(match rows.next()? {
            Some(row) => row.get::<_, Option<String>>(0)?,
            None => None,
        })
    })
    .expect("read sqlite_master")
}

fn unique_indexes(db: &Database, table: &str) -> Vec<String> {
    db.with_connection(|conn| {
        let mut stmt = conn.prepare(
            "SELECT name FROM pragma_index_list(?1) WHERE \"unique\" = 1 AND origin <> 'pk'",
        )?;
        let rows = stmt.query_map([table], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    })
    .expect("list the indexes")
}

fn count(db: &Database, sql: &str) -> i64 {
    db.with_connection(|conn| Ok(conn.query_row(sql, [], |r| r.get::<_, i64>(0))?))
        .expect("count rows")
}

fn names(columns: &[&str]) -> Vec<String> {
    columns.iter().map(|c| (*c).to_owned()).collect()
}

#[tokio::test]
async fn migration_0061_gives_a_fresh_vault_the_kind_column_and_the_pass_tables() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = open(dir.path(), "pass-fresh", true, holzi_migration_source());

        assert!(column_names(&db, ITEMS).contains(&"kind".to_string()));
        let trigger = sql_of(&db, "trigger", "z_dirty_haex_passwords_item_details_update")
            .expect("the update trigger of the entries must exist");
        assert!(trigger.contains("kind"), "kind must be tracked");

        for table in [DETAILS, BARCODES] {
            assert!(table_exists(&db, table), "{table} must exist");
            assert!(
                unique_indexes(&db, table).is_empty(),
                "{table} must have no unique constraint besides the key (034 FR-037)"
            );
            for suffix in ["insert", "update", "delete"] {
                let name = format!("z_dirty_{table}_{suffix}");
                assert!(
                    sql_of(&db, "trigger", &name).is_some(),
                    "{name} must exist: the table is CRDT-tracked"
                );
            }
            let ddl = sql_of(&db, "table", table).expect("table sql");
            assert!(
                !ddl.to_uppercase().contains("WITHOUT ROWID"),
                "{table} must not be WITHOUT ROWID (034 FR-037)"
            );
        }

        let details = column_names(&db, DETAILS);
        for column in [
            "item_id",
            "pass_kind",
            "start_date",
            "start_at",
            "start_zone",
            "parent_id",
            "source_id",
        ] {
            assert!(details.contains(&column.to_string()), "{DETAILS}.{column}");
        }
        let barcodes = column_names(&db, BARCODES);
        for column in [
            "id", "item_id", "format", "value", "label", "position", "flag",
        ] {
            assert!(
                barcodes.contains(&column.to_string()),
                "{BARCODES}.{column}"
            );
        }
        // The data model names exactly these columns (plus the CRDT columns haex-crdt adds).
        assert!(names(&["item_id"]).iter().all(|c| details.contains(c)));
    })
    .await
    .expect("join");
}

#[tokio::test]
async fn migration_0061_upgrades_a_vault_and_every_old_entry_stays_a_login() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let old = open(
            dir.path(),
            "pass-upgrade",
            true,
            migration_source_before("0061_passwords_pass"),
        );
        old.with_connection(|conn| {
            conn.execute(
                "INSERT INTO haex_passwords_item_details (id, title) VALUES ('old', 'Bank')",
                [],
            )?;
            Ok(())
        })
        .expect("insert an entry before the migration");
        assert!(!column_names(&old, ITEMS).contains(&"kind".to_string()));
        assert!(!table_exists(&old, DETAILS));
        drop(old);

        let db = open(dir.path(), "pass-upgrade", false, holzi_migration_source());
        assert!(column_names(&db, ITEMS).contains(&"kind".to_string()));
        assert!(table_exists(&db, DETAILS) && table_exists(&db, BARCODES));
        let kind: String = db
            .with_connection(|conn| {
                Ok(conn.query_row(
                    "SELECT kind FROM haex_passwords_item_details WHERE id = 'old'",
                    [],
                    |r| r.get(0),
                )?)
            })
            .expect("read the kind");
        assert_eq!(
            kind, "login",
            "an entry from before the migration is a login"
        );
    })
    .await
    .expect("join");
}

#[tokio::test]
async fn deleting_an_entry_removes_its_pass_rows_and_a_dangling_parent_is_accepted() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = open(dir.path(), "pass-cascade", true, holzi_migration_source());
        db.with_connection(|conn| {
            conn.execute_batch("PRAGMA foreign_keys = ON;")?;
            conn.execute(
                "INSERT INTO haex_passwords_item_details (id, title, kind) \
                 VALUES ('p1', 'Concert', 'pass')",
                [],
            )?;
            // The parent names no entry: there is no foreign key on it (research R8).
            conn.execute(
                "INSERT INTO haex_passwords_pass_details (item_id, parent_id) \
                 VALUES ('p1', 'not-synced-yet')",
                [],
            )?;
            conn.execute(
                "INSERT INTO haex_passwords_pass_barcodes (id, item_id, format, value) \
                 VALUES ('c1', 'p1', 'qr', 'SECRET-MARKER-migration')",
                [],
            )?;
            Ok(())
        })
        .expect("insert a pass with a dangling parent");
        assert_eq!(
            count(&db, "SELECT COUNT(*) FROM haex_passwords_pass_details"),
            1
        );

        db.with_connection(|conn| {
            conn.execute_batch("PRAGMA foreign_keys = ON;")?;
            conn.execute(
                "DELETE FROM haex_passwords_item_details WHERE id = 'p1'",
                [],
            )?;
            Ok(())
        })
        .expect("delete the entry");
        assert_eq!(
            count(&db, "SELECT COUNT(*) FROM haex_passwords_pass_details"),
            0
        );
        assert_eq!(
            count(&db, "SELECT COUNT(*) FROM haex_passwords_pass_barcodes"),
            0
        );
    })
    .await
    .expect("join");
}
