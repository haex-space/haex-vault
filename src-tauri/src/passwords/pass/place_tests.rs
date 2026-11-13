//! The place proposed for a pass from a file (spec 092, T052/T056, FR-019 as amended): a dated
//! ticket gets the tag `Ticket`, a folder only when the user has made one, everything else the top
//! of Wallet; an existing folder of another case is reused, not duplicated.

#![allow(clippy::disallowed_methods)]

use std::sync::Arc;

use haex_crdt::Database;

use super::model::PassKind;
use super::place::{proposal, resolve_path, ticket_like};
use crate::passwords::groups;
use crate::passwords::test_support::open_test_vault;
use crate::passwords::WALLET_GROUP_ID;

fn vault() -> (tempfile::TempDir, Arc<Database>) {
    let (dir, db) = open_test_vault();
    (dir, Arc::new(db))
}

fn folder(db: &Database, name: &str, parent: &str) -> String {
    db.write(|tx| {
        groups::create_group(tx, name, None, None, None, Some(parent)).map_err(Into::into)
    })
    .expect("folder")
}

fn propose(
    db: &Database,
    kind: PassKind,
    dated: bool,
    provider: Option<&str>,
) -> super::place::Proposal {
    let provider = provider.map(str::to_owned);
    db.write(|tx| Ok(proposal(tx, &kind, dated, provider.as_deref())?))
        .expect("proposal")
}

#[test]
fn only_a_ticket_or_a_boarding_pass_is_ticket_like() {
    assert!(ticket_like(&PassKind::Ticket));
    assert!(ticket_like(&PassKind::BoardingPass));
    for kind in [
        PassKind::LoyaltyCard,
        PassKind::Voucher,
        PassKind::Membership,
        PassKind::Other,
    ] {
        assert!(!ticket_like(&kind));
    }
}

#[test]
fn a_dated_ticket_gets_the_tag_and_no_folder_nobody_made() {
    let (_dir, db) = vault();
    let p = propose(&db, PassKind::Ticket, true, Some("Example Rail"));
    assert_eq!(p.tag_name.as_deref(), Some("Ticket"));
    assert_eq!(p.folder_path, None);
}

#[test]
fn an_undated_ticket_and_every_other_pass_get_nothing() {
    let (_dir, db) = vault();
    for (kind, dated) in [
        (PassKind::Ticket, false),
        (PassKind::LoyaltyCard, true),
        (PassKind::Voucher, true),
        (PassKind::Other, true),
    ] {
        let p = propose(&db, kind, dated, None);
        assert_eq!(p, Default::default());
    }
}

#[test]
fn a_folder_the_user_made_is_proposed_with_its_own_spelling() {
    let (_dir, db) = vault();
    let tickets = folder(&db, "tickets", WALLET_GROUP_ID);
    let p = propose(&db, PassKind::BoardingPass, true, Some("Example Rail"));
    assert_eq!(p.folder_path, Some(vec!["tickets".to_owned()]));
    // And a folder of the provider below it.
    folder(&db, "EXAMPLE RAIL", &tickets);
    let p = propose(&db, PassKind::BoardingPass, true, Some("Example Rail"));
    assert_eq!(
        p.folder_path,
        Some(vec!["tickets".to_owned(), "EXAMPLE RAIL".to_owned()])
    );
}

#[test]
fn confirming_a_path_creates_what_is_missing_and_reuses_what_exists() {
    let (_dir, db) = vault();
    let work = folder(&db, "Work", WALLET_GROUP_ID);
    let id = db
        .write(|tx| resolve_path(tx, &["work".to_owned(), "Trips".to_owned()]).map_err(Into::into))
        .expect("resolve")
        .expect("a folder");
    let again = db
        .write(|tx| resolve_path(tx, &["WORK".to_owned(), "trips".to_owned()]).map_err(Into::into))
        .expect("resolve")
        .expect("a folder");
    assert_eq!(id, again, "the same folder, not a second one");
    let names: i64 = db
        .with_connection(|c| {
            Ok(c.query_row(
                "SELECT COUNT(*) FROM haex_passwords_groups WHERE parent_id = ?1",
                [&work],
                |r| r.get(0),
            )?)
        })
        .expect("count");
    assert_eq!(names, 1);
    let top = db
        .write(|tx| resolve_path(tx, &[]).map_err(Into::into))
        .expect("top");
    assert_eq!(top, None);
}
