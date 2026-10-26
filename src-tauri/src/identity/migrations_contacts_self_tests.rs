//! Migration tests for `0042_contacts_self` (spec 060 FR-001, research R1): every vault has the own
//! contact "Ich" with a fixed id and constant values, an upgraded vault gets it once, a row that
//! already came in through the sync does not stop the migration, and the row is stamped so it syncs.

// These tests read the row and its HLC column directly, which the CRDT write path does not
// expose.
#![allow(clippy::disallowed_methods)]

use haex_crdt::rusqlite::params;
use haex_crdt::Database;

use super::migrations_contacts_self::CONTACTS_SELF_0042;
use super::migrations_tests::{migration_source_before, open};
use crate::contacts::ids::SELF_CONTACT_ID;
use crate::identity::holzi_migration_source;

const NIL_DEVICE: &str = "00000000-0000-0000-0000-000000000000";

/// The columns of the own contact's row, with the HLC column as `quote(...)` (`NULL` when
/// unstamped).
fn row(db: &Database) -> Vec<(String, String, String, String, String, String)> {
    db.with_connection(|conn| {
        let mut statement = conn.prepare(
            "SELECT id, origin_kind, origin_device, origin_caller, created_at, \
             quote(haex_hlc_no_sync) FROM haex_contacts WHERE id = ?1",
        )?;
        let rows = statement
            .query_map(params![SELF_CONTACT_ID], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
    .expect("read the own contact")
}

fn contacts(db: &Database) -> i64 {
    db.with_connection(|conn| {
        Ok(
            conn.query_row("SELECT COUNT(*) FROM haex_contacts", [], |r| {
                r.get::<_, i64>(0)
            })?,
        )
    })
    .expect("count")
}

#[test]
fn the_sql_names_the_constant_id() {
    assert!(
        CONTACTS_SELF_0042.contains(SELF_CONTACT_ID),
        "the migration SQL and SELF_CONTACT_ID must be the same id"
    );
}

#[tokio::test]
async fn migration_0042_gives_a_fresh_vault_exactly_one_own_contact() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = open(dir.path(), "self-fresh", true, holzi_migration_source());
        let rows = row(&db);
        assert_eq!(rows.len(), 1, "one own contact");
        let (id, kind, device, caller, created, _) = &rows[0];
        assert_eq!(id, SELF_CONTACT_ID);
        assert_eq!(kind, "holzi");
        assert_eq!(device, NIL_DEVICE);
        assert_eq!(caller, "internal:self");
        assert_eq!(created, "1970-01-01T00:00:00.000Z");
        assert_eq!(contacts(&db), 1, "and nothing else");
    })
    .await
    .expect("join");
}

#[tokio::test]
async fn migration_0042_upgrades_a_vault_from_the_schema_before_it() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let old = open(
            dir.path(),
            "self-upgrade",
            true,
            migration_source_before("0042_contacts_self"),
        );
        assert!(row(&old).is_empty(), "the old schema has no own contact");
        drop(old);
        let db = open(dir.path(), "self-upgrade", false, holzi_migration_source());
        assert_eq!(row(&db).len(), 1);
    })
    .await
    .expect("join");
}

#[tokio::test]
async fn a_row_that_synced_in_before_the_migration_does_not_stop_it() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let old = open(
            dir.path(),
            "self-synced",
            true,
            migration_source_before("0042_contacts_self"),
        );
        // A newer device synced the row here before this device ran the migration.
        old.write(|tx| {
            tx.execute(
                "INSERT INTO haex_contacts (id, origin_kind, origin_device, origin_caller, \
                 created_at, updated_at) VALUES (?1, 'holzi', ?2, 'internal:self', 't', 't')",
                &[&SELF_CONTACT_ID, &NIL_DEVICE],
            )?;
            Ok(())
        })
        .expect("insert the synced row");
        drop(old);
        let db = open(dir.path(), "self-synced", false, holzi_migration_source());
        assert_eq!(row(&db).len(), 1, "still one, and the vault opened");
    })
    .await
    .expect("join");
}

#[tokio::test]
async fn the_seeded_row_is_stamped_so_it_syncs() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = open(dir.path(), "self-stamped", true, holzi_migration_source());
        let rows = row(&db);
        assert_ne!(
            rows[0].5, "NULL",
            "haex-crdt stamps a row a migration wrote, or it would never reach another device"
        );
    })
    .await
    .expect("join");
}
