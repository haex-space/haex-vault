//! Tests for the folders of Wallet (spec 092, Phase W): the fixed root `wallet` that appears with
//! the first folder, what cannot be done to it, and the rule that a pass lies at the top level or in
//! a Wallet folder while every other entry lies anywhere but there.

// The tests read raw columns that the CRDT write path does not expose.
#![allow(clippy::disallowed_methods, clippy::redundant_closure)]

use haex_crdt::rusqlite::params;
use haex_crdt::Database;

use super::groups::{self, siblings};
use super::items;
use super::kind::EntryType;
use super::model::{GroupPatch, ItemInput, Patch, Target, TargetKind};
use super::test_support::open_test_vault;
use super::{trash, WALLET_GROUP_ID};
use crate::error::HolziError;
use crate::storage::query;

fn write<R>(
    db: &Database,
    f: impl FnOnce(&mut haex_crdt::CrdtTransaction<'_>) -> crate::error::Result<R>,
) -> crate::error::Result<R> {
    db.write(|tx| f(tx).map_err(haex_crdt::Error::from))
        .map_err(HolziError::from)
}

fn folder(db: &Database, name: &str, parent: Option<&str>) -> String {
    write(db, |tx| {
        groups::create_group(tx, name, None, None, None, parent)
    })
    .expect("folder")
}

fn entry(db: &Database, kind: &EntryType, group: Option<&str>) -> crate::error::Result<String> {
    write(db, |tx| {
        items::create_item_as(tx, &ItemInput::default(), group, kind, |_, _| Ok(()))
    })
}

fn target(kind: TargetKind, id: &str) -> Target {
    Target {
        kind,
        id: id.to_string(),
    }
}

fn reason_of<R: std::fmt::Debug>(result: crate::error::Result<R>) -> String {
    match result {
        Err(HolziError::InvalidInput { reason }) => reason,
        other => panic!("expected InvalidInput, got {other:?}"),
    }
}

fn root_rows(db: &Database) -> i64 {
    db.with_connection(|conn| {
        Ok(conn.query_row(
            "SELECT COUNT(*) FROM haex_passwords_groups WHERE id = ?1",
            params![WALLET_GROUP_ID],
            |r| r.get::<_, i64>(0),
        )?)
    })
    .expect("count")
}

#[test]
fn the_first_wallet_folder_creates_the_root_and_the_second_does_not() {
    let (_dir, db) = open_test_vault();
    assert_eq!(root_rows(&db), 0, "no root before the first folder");
    let first = folder(&db, "Work", Some(WALLET_GROUP_ID));
    assert_eq!(root_rows(&db), 1);
    folder(&db, "Travel", Some(WALLET_GROUP_ID));
    folder(&db, "Trains", Some(&first));
    assert_eq!(root_rows(&db), 1, "one root, however many folders");
}

#[test]
fn the_root_has_no_name_and_the_top_level_does_not_list_it() {
    let (_dir, db) = open_test_vault();
    folder(&db, "Work", Some(WALLET_GROUP_ID));
    let own = folder(&db, "Logins", None);
    let top: Vec<String> = query::read(&db, |q| siblings(q, None).map_err(haex_crdt::Error::from))
        .expect("top")
        .into_iter()
        .map(|g| g.id)
        .collect();
    assert_eq!(top, vec![own], "only the password manager's own folder");
    let below: Vec<Option<String>> = query::read(&db, |q| {
        siblings(q, Some(WALLET_GROUP_ID)).map_err(haex_crdt::Error::from)
    })
    .expect("below")
    .into_iter()
    .map(|g| g.name)
    .collect();
    assert_eq!(below, vec![Some("Work".to_string())]);
}

#[test]
fn the_root_cannot_be_renamed_deleted_or_moved() {
    let (_dir, db) = open_test_vault();
    let work = folder(&db, "Work", Some(WALLET_GROUP_ID));
    let other = folder(&db, "Other", None);
    let rename = write(&db, |tx| {
        groups::update_group(
            tx,
            WALLET_GROUP_ID,
            &GroupPatch {
                name: Patch::Set("Mine".into()),
                ..GroupPatch::default()
            },
        )
    });
    assert_eq!(reason_of(rename), "wallet");
    let delete = write(&db, |tx| {
        trash::trash(tx, &[target(TargetKind::Group, WALLET_GROUP_ID)])
    });
    assert_eq!(reason_of(delete), "wallet");
    let moved = write(&db, |tx| {
        groups::move_targets(
            tx,
            &[target(TargetKind::Group, WALLET_GROUP_ID)],
            Some(&other),
        )
    });
    assert_eq!(reason_of(moved), "wallet");
    // A folder below it can be renamed and trashed like any.
    write(&db, |tx| {
        groups::update_group(
            tx,
            &work,
            &GroupPatch {
                name: Patch::Set("Job".into()),
                ..GroupPatch::default()
            },
        )
    })
    .expect("rename a wallet folder");
    write(&db, |tx| {
        trash::trash(tx, &[target(TargetKind::Group, &work)])
    })
    .expect("trash a wallet folder");
}

#[test]
fn a_pass_goes_to_the_top_level_or_a_wallet_folder_and_nowhere_else() {
    let (_dir, db) = open_test_vault();
    let wallet = folder(&db, "Work", Some(WALLET_GROUP_ID));
    let own = folder(&db, "Logins", None);
    let pass = entry(&db, &EntryType::Pass, None).expect("a pass at the top level");
    entry(&db, &EntryType::Pass, Some(&wallet)).expect("a pass made in a wallet folder");
    assert_eq!(
        reason_of(entry(&db, &EntryType::Pass, Some(&own))),
        "pass_place",
        "not in a password manager folder"
    );
    write(&db, |tx| {
        groups::move_targets(tx, &[target(TargetKind::Item, &pass)], Some(&wallet))
    })
    .expect("moved into a wallet folder");
    write(&db, |tx| {
        groups::move_targets(tx, &[target(TargetKind::Item, &pass)], None)
    })
    .expect("moved to the top level");
    let refused = write(&db, |tx| {
        groups::move_targets(tx, &[target(TargetKind::Item, &pass)], Some(&own))
    });
    assert_eq!(reason_of(refused), "pass_place");
}

#[test]
fn a_login_and_an_unknown_type_stay_out_of_wallet_folders() {
    let (_dir, db) = open_test_vault();
    let wallet = folder(&db, "Work", Some(WALLET_GROUP_ID));
    let own = folder(&db, "Logins", None);
    assert_eq!(
        reason_of(entry(&db, &EntryType::Login, Some(&wallet))),
        "login_place"
    );
    assert_eq!(
        reason_of(entry(
            &db,
            &EntryType::Unknown("sshkey".into()),
            Some(&wallet)
        )),
        "login_place"
    );
    let login = entry(&db, &EntryType::Login, Some(&own)).expect("a login in its folder");
    let refused = write(&db, |tx| {
        groups::move_targets(tx, &[target(TargetKind::Item, &login)], Some(&wallet))
    });
    assert_eq!(reason_of(refused), "login_place");
    let root = write(&db, |tx| {
        groups::move_targets(
            tx,
            &[target(TargetKind::Item, &login)],
            Some(WALLET_GROUP_ID),
        )
    });
    assert_eq!(reason_of(root), "login_place", "nor onto the root itself");
}

#[test]
fn a_folder_stays_on_its_side() {
    let (_dir, db) = open_test_vault();
    let work = folder(&db, "Work", Some(WALLET_GROUP_ID));
    let travel = folder(&db, "Travel", Some(WALLET_GROUP_ID));
    let own = folder(&db, "Logins", None);
    write(&db, |tx| {
        groups::move_targets(tx, &[target(TargetKind::Group, &work)], Some(&travel))
    })
    .expect("within Wallet");
    write(&db, |tx| {
        groups::move_targets(
            tx,
            &[target(TargetKind::Group, &work)],
            Some(WALLET_GROUP_ID),
        )
    })
    .expect("to the top of Wallet");
    for (what, to) in [
        (&work, Some(own.as_str())),
        (&work, None),
        (&own, Some(work.as_str())),
        (&own, Some(WALLET_GROUP_ID)),
    ] {
        let refused = write(&db, |tx| {
            groups::move_targets(tx, &[target(TargetKind::Group, what)], to)
        });
        assert_eq!(reason_of(refused), "folder_place", "{what} to {to:?}");
    }
}
