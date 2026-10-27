//! The logic behind the commands: create checks the rule, two-way rules wait for a later version,
//! update and pause change the row, delete leaves files alone, the list carries the status.

// These tests insert devices directly, which the CRDT write path does not expose.
#![allow(clippy::disallowed_methods)]

use std::path::PathBuf;

use haex_crdt::rusqlite::params;

use super::*;
use crate::file_sync::model::{SyncRuleDeleteMode, SyncRulePlace};
use crate::file_sync::status::SyncRuleState;
use crate::files::SourceRef;
use crate::remote_storage::test_support::vault;

const DEVICE: &str = "11111111-1111-4111-8111-111111111111";

fn setup() -> (tempfile::TempDir, VaultDb) {
    let (dir, db) = vault();
    db.write_blocking(|tx| {
        tx.execute(
            "INSERT INTO known_devices (installation_uuid, vault_device_uuid, alias, first_seen) \
             VALUES ('inst', ?1, NULL, 0)",
            params![DEVICE],
        )?;
        Ok(())
    })
    .unwrap();
    (dir, db)
}

fn own() -> OwnPlaces {
    OwnPlaces::new(vec![PathBuf::from("/data/holzi")])
}

fn draft(a: &str, b: &str) -> SyncRuleDraft {
    let place = |path: &str| SyncRulePlace {
        source: SourceRef::Device,
        path: path.to_owned(),
    };
    SyncRuleDraft {
        name: "Fotos".to_owned(),
        place_a: place(a),
        place_b: place(b),
        direction: SyncRuleDirection::AToB,
        delete_mode: SyncRuleDeleteMode::Keep,
        interval_secs: 900,
        enabled: true,
        allow_decrypted: false,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn create_checks_and_stores_a_rule() {
    let (_dir, db) = setup();
    let rule = create(&db, DEVICE, &own(), draft("/home/a/", "/home/b"))
        .await
        .unwrap();
    assert_eq!(rule.place_a.path, "/home/a", "normalised");
    let overlap = create(&db, DEVICE, &own(), draft("/home/a/sub", "/home/z"))
        .await
        .unwrap_err();
    assert_eq!(overlap.code, SyncRuleErrorCode::Overlap);
}

#[tokio::test(flavor = "multi_thread")]
async fn two_way_rules_wait_for_a_later_version() {
    let (_dir, db) = setup();
    let mut both = draft("/home/a", "/home/b");
    both.direction = SyncRuleDirection::Both;
    let error = create(&db, DEVICE, &own(), both).await.unwrap_err();
    assert_eq!(error.code, SyncRuleErrorCode::Unsupported);
}

#[tokio::test(flavor = "multi_thread")]
async fn update_pause_and_delete() {
    let (_dir, db) = setup();
    let rule = create(&db, DEVICE, &own(), draft("/home/a", "/home/b"))
        .await
        .unwrap();
    let mut changed = draft("/home/a", "/home/b");
    changed.name = "Bilder".to_owned();
    let updated = update(&db, DEVICE, &own(), &rule.id, changed)
        .await
        .unwrap();
    assert_eq!(updated.name, "Bilder");
    let paused = set_enabled(&db, DEVICE, &rule.id, false).await.unwrap();
    assert!(!paused.enabled);
    let views = list(&db, DEVICE, None).await.unwrap();
    assert_eq!(views[0].status.state, SyncRuleState::Paused);
    delete(&db, DEVICE, &rule.id).await.unwrap();
    assert_eq!(
        delete(&db, DEVICE, &rule.id).await.unwrap_err().code,
        SyncRuleErrorCode::NotFound
    );
    assert!(list(&db, DEVICE, None).await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn an_update_does_not_count_the_rule_itself_as_overlap() {
    let (_dir, db) = setup();
    let rule = create(&db, DEVICE, &own(), draft("/home/a", "/home/b"))
        .await
        .unwrap();
    update(
        &db,
        DEVICE,
        &own(),
        &rule.id,
        draft("/home/a/sub", "/home/b"),
    )
    .await
    .expect("its own old places do not overlap");
}
