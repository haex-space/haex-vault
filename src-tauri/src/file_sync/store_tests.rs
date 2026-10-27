//! Rows of sync rules (data-model.md): a rule round-trips, one state row per path, a change of
//! places drops the state, and rows of another device stay invisible (ADR 0018).

// These tests insert devices directly, which the CRDT write path does not expose.
#![allow(clippy::disallowed_methods)]

use haex_crdt::rusqlite::params;

use super::*;
use crate::file_sync::model::{SyncRuleDeleteMode, SyncRuleDirection};
use crate::files::SourceRef;
use crate::remote_storage::test_support::vault;
use crate::vault_gate::VaultDb;

const LAPTOP: &str = "11111111-1111-4111-8111-111111111111";
const PHONE: &str = "22222222-2222-4222-8222-222222222222";
const NOW: &str = "2026-10-26T10:00:00Z";

fn with_devices(db: &VaultDb) {
    db.write_blocking(|tx| {
        for (installation, device) in [("inst-laptop", LAPTOP), ("inst-phone", PHONE)] {
            tx.execute(
                "INSERT INTO known_devices (installation_uuid, vault_device_uuid, alias, first_seen) \
                 VALUES (?1, ?2, NULL, 0)",
                params![installation, device],
            )?;
        }
        Ok(())
    })
    .expect("devices");
}

fn draft(a: &str, b: &str) -> SyncRuleDraft {
    SyncRuleDraft {
        name: "Fotos".to_owned(),
        place_a: SyncRulePlace {
            source: SourceRef::Device,
            path: a.to_owned(),
        },
        place_b: SyncRulePlace {
            source: SourceRef::Storage {
                storage_id: "s3".to_owned(),
            },
            path: b.to_owned(),
        },
        direction: SyncRuleDirection::AToB,
        delete_mode: SyncRuleDeleteMode::Keep,
        interval_secs: 900,
        enabled: true,
        allow_decrypted: false,
    }
}

fn row(path: &str, size: u64) -> StateRow {
    StateRow {
        path: path.to_owned(),
        a: Some(Seen { size, mtime_ms: 1 }),
        b: Some(Seen { size, mtime_ms: 2 }),
        kept: false,
        hash: None,
    }
}

fn setup() -> (tempfile::TempDir, VaultDb) {
    let (dir, db) = vault();
    with_devices(&db);
    db.write_blocking(|tx| {
        insert_rule(tx, LAPTOP, "r1", &draft("/home/a", "Fotos/"), NOW)?;
        Ok(())
    })
    .expect("rule");
    (dir, db)
}

#[test]
fn a_rule_round_trips() {
    let (_dir, db) = setup();
    let got = db
        .read_blocking(|r| Ok(rule(r, LAPTOP, "r1")))
        .expect("read")
        .expect("query")
        .expect("there");
    assert_eq!(got.draft(), draft("/home/a", "Fotos/"));
    assert_eq!(got.created_at, NOW);
    assert_eq!(got.last_outcome, None);
    db.write_blocking(|tx| {
        set_result(
            tx,
            LAPTOP,
            "r1",
            NOW,
            SyncRuleOutcome::Partial,
            Some("one file"),
        )?;
        Ok(())
    })
    .expect("result");
    let got = db
        .read_blocking(|r| Ok(rules(r, LAPTOP)))
        .expect("read")
        .expect("query");
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].last_outcome, Some(SyncRuleOutcome::Partial));
    assert_eq!(got[0].last_error.as_deref(), Some("one file"));
}

#[test]
fn rules_of_another_device_are_invisible() {
    let (_dir, db) = setup();
    let phone = db
        .read_blocking(|r| Ok(rules(r, PHONE)))
        .expect("read")
        .expect("query");
    assert!(phone.is_empty());
    let changed = db
        .write_blocking(|tx| Ok(delete_rule(tx, PHONE, "r1")))
        .expect("write")
        .expect("delete");
    assert!(!changed, "the phone cannot delete the laptop's rule");
}

#[test]
fn one_state_row_per_path_and_renames_replace() {
    let (_dir, db) = setup();
    db.write_blocking(|tx| {
        put_state(tx, LAPTOP, "r1", &row("a.jpg", 1))?;
        put_state(tx, LAPTOP, "r1", &row("a.jpg", 5))?;
        put_state(tx, LAPTOP, "r1", &row("b.jpg", 2))?;
        rename_state(tx, LAPTOP, "r1", "a.jpg", "b.jpg")?;
        Ok(())
    })
    .expect("state");
    let got = db
        .read_blocking(|r| Ok(state(r, LAPTOP, "r1")))
        .expect("read")
        .expect("query");
    assert_eq!(got, [row("b.jpg", 5)]);
}

#[test]
fn changing_places_drops_state_and_pending_but_a_rename_does_not() {
    let (_dir, db) = setup();
    let pending_row = PendingRow {
        path: "x".to_owned(),
        side: SyncRuleSide::B,
        found_run_id: "run".to_owned(),
    };
    db.write_blocking(|tx| {
        put_state(tx, LAPTOP, "r1", &row("a.jpg", 1))?;
        put_pending(tx, LAPTOP, "r1", &pending_row)?;
        let mut renamed = draft("/home/a", "Fotos/");
        renamed.name = "Bilder".to_owned();
        update_rule(tx, LAPTOP, "r1", &renamed, NOW)?;
        Ok(())
    })
    .expect("rename");
    let (kept, waiting) = db
        .read_blocking(|r| Ok((state(r, LAPTOP, "r1"), pending(r, LAPTOP, "r1"))))
        .expect("read");
    assert_eq!(kept.expect("state").len(), 1);
    assert_eq!(waiting.expect("pending"), [pending_row]);
    db.write_blocking(|tx| {
        update_rule(tx, LAPTOP, "r1", &draft("/home/other", "Fotos/"), NOW)?;
        Ok(())
    })
    .expect("move");
    let (kept, waiting) = db
        .read_blocking(|r| Ok((state(r, LAPTOP, "r1"), pending(r, LAPTOP, "r1"))))
        .expect("read");
    assert!(kept.expect("state").is_empty());
    assert!(waiting.expect("pending").is_empty());
}

#[test]
fn pausing_and_deleting() {
    let (_dir, db) = setup();
    db.write_blocking(|tx| {
        assert!(set_enabled(tx, LAPTOP, "r1", false, NOW)?);
        Ok(())
    })
    .expect("pause");
    let got = db
        .read_blocking(|r| Ok(rule(r, LAPTOP, "r1")))
        .expect("read")
        .expect("query")
        .expect("there");
    assert!(!got.enabled);
    let gone = db
        .write_blocking(|tx| Ok(delete_rule(tx, LAPTOP, "r1")))
        .expect("write")
        .expect("delete");
    assert!(gone);
}
