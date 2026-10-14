//! Migration tests for `0033_contacts` (spec 051, data-model.md): a fresh vault and a vault from the
//! schema before both end with the twelve tables, each tracked by haex-crdt and without a UNIQUE
//! constraint; deleting a contact takes its values, cards, group memberships, targets, phone links
//! and issues with it.

// These tests read `sqlite_master` and `pragma_*` directly, which the CRDT write path does not
// expose.
#![allow(clippy::disallowed_methods)]

use haex_crdt::Database;

use super::migrations_extensions_tests::{has_hlc_column, unique_indexes};
use super::migrations_tests::{migration_source_before, open, table_exists};
use crate::identity::holzi_migration_source;

const TABLES: [&str; 12] = [
    "haex_contacts",
    "haex_contacts_values",
    "haex_contacts_cards",
    "haex_contacts_groups",
    "haex_contacts_group_members",
    "haex_contacts_binaries",
    "haex_contacts_targets",
    "haex_contacts_phone_links",
    "haex_contacts_phone_issues",
    "haex_contacts_phone_runs",
    "haex_contacts_own_card",
    "haex_contacts_blocked_vaults",
];

/// The tables that hang off a contact and must go with it.
const DEPENDENT: [&str; 6] = [
    "haex_contacts_values",
    "haex_contacts_cards",
    "haex_contacts_group_members",
    "haex_contacts_targets",
    "haex_contacts_phone_links",
    "haex_contacts_phone_issues",
];

fn assert_schema(db: &Database) {
    for table in TABLES {
        assert!(table_exists(db, table), "{table} exists");
        assert!(
            has_hlc_column(db, table),
            "{table} reaches every own device"
        );
        assert_eq!(
            unique_indexes(db, table),
            0,
            "{table}: a UNIQUE constraint would halt the sync"
        );
    }
}

fn device(db: &Database) -> String {
    crate::storage::query::read(db, |q| {
        use crate::storage::query::Query;
        q.query_row(
            "SELECT vault_device_uuid FROM known_devices LIMIT 1",
            &[],
            |row| row.get::<_, String>(0),
        )
    })
    .expect("read")
    .expect("the vault knows its own device")
}

fn count(db: &Database, table: &str) -> i64 {
    crate::storage::query::read(db, |q| {
        use crate::storage::query::Query;
        q.query_row(&format!("SELECT COUNT(*) FROM {table}"), &[], |row| {
            row.get(0)
        })
    })
    .expect("count")
    .unwrap_or(0)
}

#[tokio::test]
async fn migration_0033_gives_a_fresh_vault_the_address_book() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = open(dir.path(), "contacts-fresh", true, holzi_migration_source());
        assert_schema(&db);
    })
    .await
    .expect("join");
}

#[tokio::test]
async fn migration_0033_upgrades_a_vault_from_the_schema_before_it() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let old = open(
            dir.path(),
            "contacts-upgrade",
            true,
            migration_source_before("0033_contacts"),
        );
        assert!(!table_exists(&old, "haex_contacts"));
        drop(old);
        let db = open(
            dir.path(),
            "contacts-upgrade",
            false,
            holzi_migration_source(),
        );
        assert_schema(&db);
    })
    .await
    .expect("join");
}

#[tokio::test]
async fn deleting_a_contact_takes_everything_that_hangs_off_it() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = open(
            dir.path(),
            "contacts-cascade",
            true,
            holzi_migration_source(),
        );
        let device = device(&db);
        db.write(|tx| {
            tx.execute(
                "INSERT INTO haex_contacts (id, given_name, origin_kind, origin_device, \
                 origin_caller, created_at, updated_at) \
                 VALUES ('c1', 'Anna', 'holzi', ?1, 'user', 't', 't')",
                &[&device],
            )?;
            tx.execute(
                "INSERT INTO haex_contacts_values (id, contact_id, kind, label, value, position) \
                 VALUES ('v1', 'c1', 'phone', 'mobile', '0170', 0)",
                &[],
            )?;
            tx.execute(
                "INSERT INTO haex_contacts_cards (vault_pubkey, contact_id, card, issued_at, \
                 received_at) VALUES ('pk', 'c1', x'00', 't', 't')",
                &[],
            )?;
            tx.execute(
                "INSERT INTO haex_contacts_groups (id, name) VALUES ('g1', 'Arbeit')",
                &[],
            )?;
            tx.execute(
                "INSERT INTO haex_contacts_group_members (id, group_id, contact_id) \
                 VALUES ('m1', 'g1', 'c1')",
                &[],
            )?;
            tx.execute(
                "INSERT INTO haex_contacts_targets (id, contact_id, vault_device_uuid, \
                 account_type, account_name) VALUES ('t1', 'c1', ?1, '', '')",
                &[&device],
            )?;
            tx.execute(
                "INSERT INTO haex_contacts_phone_links (id, vault_device_uuid, contact_id, \
                 raw_contact_id, lookup_key, account_type, account_name, version, read_only, \
                 base) VALUES ('l1', ?1, 'c1', 7, 'lk', '', '', 1, 0, '{}')",
                &[&device],
            )?;
            tx.execute(
                "INSERT INTO haex_contacts_phone_issues (id, vault_device_uuid, contact_id, \
                 reason, at) VALUES ('i1', ?1, 'c1', 'read_only', 't')",
                &[&device],
            )?;
            Ok(())
        })
        .expect("insert a contact with everything");
        for table in DEPENDENT {
            assert_eq!(count(&db, table), 1, "{table} before");
        }

        db.write(|tx| {
            tx.execute("DELETE FROM haex_contacts WHERE id = 'c1'", &[])?;
            Ok(())
        })
        .expect("delete the contact");
        for table in DEPENDENT {
            assert_eq!(count(&db, table), 0, "{table} after");
        }
        assert_eq!(count(&db, "haex_contacts_groups"), 1, "the group stays");
    })
    .await
    .expect("join");
}

#[tokio::test]
async fn migration_0034_keeps_the_known_raw_contacts_on_this_device() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = open(dir.path(), "contacts-known", true, holzi_migration_source());
        let table = "haex_contacts_phone_known_no_sync";
        assert!(table_exists(&db, table));
        assert!(!has_hlc_column(&db, table), "device-local: never synced");
    })
    .await
    .expect("join");
}
