//! The name as a row of the vault, against a real temporary vault.

use haex_crdt::rusqlite::params;

use super::*;
use crate::identity::VAULT_SCOPE_UUID;
use crate::remote_storage::test_support::vault;

fn stored_scope(db: &VaultDb) -> Option<String> {
    db.read_blocking(|q| {
        q.query_row(
            "SELECT vault_device_uuid FROM preferences WHERE key = ?1",
            params![PREF_KEY],
            |row| row.get::<_, String>(0),
        )
    })
    .unwrap()
}

#[test]
fn a_vault_without_a_name_has_the_default() {
    let (_dir, db) = vault();
    assert_eq!(
        db.read_blocking(|q| read(q)).unwrap(),
        AssistantName::default()
    );
}

#[test]
fn the_name_is_one_row_for_the_whole_vault() {
    let (_dir, db) = vault();
    let luna = AssistantName::new("Luna", "Luhna").unwrap();

    db.write_blocking({
        let luna = luna.clone();
        move |tx| write(tx, &luna)
    })
    .unwrap();

    assert_eq!(db.read_blocking(|q| read(q)).unwrap(), luna);
    assert_eq!(
        stored_scope(&db).as_deref(),
        Some(VAULT_SCOPE_UUID.to_string().as_str())
    );
}

#[test]
fn resetting_brings_the_default_back() {
    let (_dir, db) = vault();
    let luna = AssistantName::new("Luna", "Luhna").unwrap();
    db.write_blocking(move |tx| write(tx, &luna)).unwrap();

    db.write_blocking(reset).unwrap();

    assert_eq!(
        db.read_blocking(|q| read(q)).unwrap(),
        AssistantName::default()
    );
    assert_eq!(
        stored_scope(&db),
        None,
        "the row is gone, not set to the default"
    );
}

#[test]
fn a_row_that_cannot_be_used_reads_as_the_default() {
    let (_dir, db) = vault();
    db.write_blocking(|tx| {
        crate::storage::preferences::insert_or_update(tx, PrefScope::Vault, PREF_KEY, "kaputt")
    })
    .unwrap();
    assert_eq!(
        db.read_blocking(|q| read(q)).unwrap(),
        AssistantName::default()
    );
}
