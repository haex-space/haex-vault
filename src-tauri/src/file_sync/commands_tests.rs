//! The logic behind the commands: create checks the rule and stores a two-way rule as it is,
//! update and pause change the row, delete leaves files alone, the list carries the status, and the
//! waiting deletions belong to a known rule.

// These tests insert devices directly, which the CRDT write path does not expose.
#![allow(clippy::disallowed_methods)]

use std::path::PathBuf;

use haex_crdt::rusqlite::params;

use super::*;
use crate::file_sync::model::{SyncRuleDeleteMode, SyncRuleDirection, SyncRulePlace};
use crate::file_sync::status::SyncRuleState;
use crate::files::SourceRef;
use crate::files::{FilesError, FilesErrorCode};
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

#[test]
fn no_access_on_a_read_is_not_reported_as_read_only() {
    let read = place_error(FilesError::new(FilesErrorCode::NoAccess, "cannot read"));
    assert_eq!(read.code, SyncRuleErrorCode::PlaceError);

    let write = place_error(FilesError::new(FilesErrorCode::ReadOnly, "cannot write"));
    assert_eq!(write.code, SyncRuleErrorCode::ReadOnly);
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
async fn a_two_way_rule_is_stored_as_it_is() {
    let (_dir, db) = setup();
    let mut both = draft("/home/a", "/home/b");
    both.direction = SyncRuleDirection::Both;
    let rule = create(&db, DEVICE, &own(), both).await.unwrap();
    assert_eq!(rule.direction, SyncRuleDirection::Both);
    assert_eq!(
        rule.place_a.path, "/home/a",
        "the places stay in their order"
    );
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

#[tokio::test(flavor = "multi_thread")]
async fn the_waiting_deletions_of_an_unknown_rule_are_not_found() {
    let (_dir, db) = setup();
    let error = pending_list(&db, DEVICE, "nope").await.unwrap_err();
    assert_eq!(error.code, SyncRuleErrorCode::NotFound);
    let rule = create(&db, DEVICE, &own(), draft("/home/a", "/home/b"))
        .await
        .unwrap();
    assert!(pending_list(&db, DEVICE, &rule.id)
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn the_log_depth_is_checked_and_the_runs_belong_to_a_known_rule() {
    let (_dir, db) = setup();
    assert_eq!(log_depth(&db, DEVICE).await.unwrap(), 10);
    let error = set_log_depth(&db, DEVICE, 101).await.unwrap_err();
    assert_eq!(error.code, SyncRuleErrorCode::InvalidDepth);
    set_log_depth(&db, DEVICE, 0).await.unwrap();
    assert_eq!(log_depth(&db, DEVICE).await.unwrap(), 0);
    let error = runs(&db, DEVICE, "nope").await.unwrap_err();
    assert_eq!(error.code, SyncRuleErrorCode::NotFound);
    assert!(run_items(&db, DEVICE, "nope", 0, 10)
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_preview_keeps_the_state_unless_the_draft_moves_the_places() {
    let (_dir, db) = setup();
    let rule = create(&db, DEVICE, &own(), draft("/home/a", "/home/b"))
        .await
        .unwrap();
    let row = store::StateRow {
        path: "x.txt".to_owned(),
        a: None,
        b: None,
        kept: false,
        hash: None,
    };
    let id = rule.id.clone();
    db.write_blocking(move |tx| {
        store::put_state(tx, DEVICE, &id, &row)?;
        Ok(())
    })
    .unwrap();
    let (_, state) = preview_rule(&db, DEVICE, &own(), Some(&rule.id), None)
        .await
        .unwrap();
    assert_eq!(state.len(), 1);
    let mut renamed = draft("/home/a", "/home/b");
    renamed.name = "Neu".to_owned();
    let (previewed, state) = preview_rule(&db, DEVICE, &own(), Some(&rule.id), Some(renamed))
        .await
        .unwrap();
    assert_eq!(previewed.name, "Neu");
    assert_eq!(state.len(), 1, "a new name keeps the state");
    let moved = draft("/home/a", "/home/c");
    let (_, state) = preview_rule(&db, DEVICE, &own(), Some(&rule.id), Some(moved))
        .await
        .unwrap();
    assert!(state.is_empty(), "new places start over");
    let (fresh, state) = preview_rule(&db, DEVICE, &own(), None, Some(draft("/x", "/y")))
        .await
        .unwrap();
    assert_eq!(fresh.id, "preview");
    assert!(state.is_empty());
    let overlap = preview_rule(&db, DEVICE, &own(), None, Some(draft("/home/a/sub", "/z")))
        .await
        .unwrap_err();
    assert_eq!(overlap.code, SyncRuleErrorCode::Overlap);
    let missing = preview_rule(&db, DEVICE, &own(), None, None)
        .await
        .unwrap_err();
    assert_eq!(missing.code, SyncRuleErrorCode::NotFound);
}
