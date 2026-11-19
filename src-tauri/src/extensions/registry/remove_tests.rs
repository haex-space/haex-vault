// The tests read registry rows directly to see what a removal left.
#![allow(clippy::disallowed_methods)]

use std::path::Path;
use std::sync::Arc;

use super::*;
use crate::extensions::registry::install::install;
use crate::passwords::test_support::open_test_vault;
use crate::storage::known_devices;
use crate::storage::query::{self, Query};
use crate::vault_gate::VaultGate;

fn count(db: &haex_crdt::Database, sql: &str, ext: Uuid) -> i64 {
    query::read(db, |r| {
        r.query_row(sql, &[&ext.to_string()], |row| row.get(0))
    })
    .unwrap()
    .unwrap_or(0)
}

#[test]
fn a_removal_leaves_a_tombstone_with_its_own_hlc_and_deletes_the_registry_rows() {
    let (_dir, db) = open_test_vault();
    let vault = VaultGate::new().vault_db(Arc::new(db.clone())).unwrap();
    let device = vault
        .read_blocking(|q| known_devices::list_devices(q))
        .unwrap()[0]
        .vault_device_uuid;
    let bytes = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/extension_bundles/good-notes-like.xt"),
    )
    .unwrap();
    let ext = install(&vault, &bytes, vec![], false, device, 1)
        .unwrap()
        .ids
        .extension_id;

    remove(&vault, ext, true, 2).unwrap();
    let (state, purge_data, purge_hlc, hlc): (String, i64, Option<String>, String) =
        query::read(&db, |r| {
            r.query_row(
                "SELECT state, purge_data, purge_hlc, haex_hlc_no_sync FROM extensions WHERE id = ?1",
                &[&ext.to_string()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
        })
        .unwrap()
        .unwrap();
    assert_eq!((state.as_str(), purge_data), ("removed", 1));
    assert_eq!(
        purge_hlc.as_deref(),
        Some(hlc.as_str()),
        "the HLC of the removing write"
    );
    for table in [
        "extension_bundles",
        "extension_migrations",
        "extension_permissions",
        "extension_limits",
    ] {
        let sql = format!("SELECT COUNT(*) FROM {table} WHERE extension_id = ?1");
        assert_eq!(count(&db, &sql, ext), 0, "{table}");
    }
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM extension_blobs WHERE orphaned_at IS NULL AND ?1 <> ''",
            ext
        ),
        0,
        "no file refers to the BLOBs any more"
    );

    assert!(matches!(
        remove(&vault, ext, true, 3),
        Err(HaexError::ExtensionNotFound)
    ));
    install(&vault, &bytes, vec![], false, device, 4).unwrap();
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM extension_blobs WHERE orphaned_at IS NOT NULL AND ?1 <> ''",
            ext
        ),
        0,
        "a reinstall keeps its BLOBs"
    );
}
