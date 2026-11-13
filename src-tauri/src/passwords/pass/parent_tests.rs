//! Tests of the link of a pass to the entry it belongs to (spec 092, T041, FR-013, FR-015, research
//! R8): a loop is refused at write, a parent that is not in the vault is accepted, clearing works,
//! the check ends on a loop that already is stored, and trashing a parent changes nothing in its
//! passes.

// The tests write raw columns that the CRDT write path does not expose.
#![allow(clippy::disallowed_methods, clippy::redundant_closure)]

use std::sync::Arc;

use haex_crdt::Database;

use super::model::PassInput;
use super::store;
use crate::error::HolziError;
use crate::passwords::access::Caller;
use crate::passwords::model::{ItemDetail, ItemInput, Target, TargetKind};
use crate::passwords::service::PasswordsService;
use crate::passwords::test_support::open_test_vault;
use crate::vault_gate::VaultGate;

fn setup() -> (tempfile::TempDir, Arc<Database>, PasswordsService) {
    let (dir, db) = open_test_vault();
    let db = Arc::new(db);
    let vault = VaultGate::new().vault_db(db.clone()).expect("vault");
    (dir, db, PasswordsService::new(vault))
}

async fn pass(service: &PasswordsService, title: &str, parent: Option<&str>) -> ItemDetail {
    service
        .pass_create(
            &Caller::User,
            PassInput {
                title: Some(title.to_owned()),
                parent_id: parent.map(str::to_owned),
                ..PassInput::default()
            },
            None,
        )
        .await
        .expect("pass")
}

fn token(detail: &ItemDetail) -> String {
    detail.header.updated_at.clone().expect("token")
}

fn parent_of(db: &Database, id: &str) -> Option<String> {
    db.with_connection(|c| {
        Ok(c.query_row(
            "SELECT parent_id FROM haex_passwords_pass_details WHERE item_id = ?1",
            [id],
            |r| r.get(0),
        )?)
    })
    .expect("parent")
}

fn refusal(result: &Result<ItemDetail, HolziError>) -> Option<&str> {
    match result {
        Err(HolziError::PassRefused { code, .. }) => Some(code),
        _ => None,
    }
}

#[tokio::test]
async fn a_pass_belongs_to_a_login_or_to_a_pass_and_the_link_can_be_cleared() {
    let (_dir, db, service) = setup();
    let login = service
        .create_item(
            &Caller::User,
            &[],
            ItemInput {
                title: Some("Train account".to_owned()),
                ..ItemInput::default()
            },
            None,
        )
        .await
        .expect("login");
    let ticket = pass(&service, "Trip", Some(&login)).await;
    assert_eq!(ticket.header.parent_id.as_deref(), Some(login.as_str()));

    let outbound = pass(&service, "Outbound", None).await;
    let back = service
        .pass_set_parent(
            &Caller::User,
            ticket.header.id.clone(),
            token(&ticket),
            Some(outbound.header.id.clone()),
        )
        .await
        .expect("under a pass");
    assert_eq!(
        back.header.parent_id.as_deref(),
        Some(outbound.header.id.as_str())
    );

    let cleared = service
        .pass_set_parent(&Caller::User, ticket.header.id.clone(), token(&back), None)
        .await
        .expect("cleared");
    assert_eq!(cleared.header.parent_id, None);
    assert_eq!(parent_of(&db, &ticket.header.id), None);
}

#[tokio::test]
async fn a_loop_is_refused_through_the_whole_chain() {
    let (_dir, _db, service) = setup();
    let a = pass(&service, "a", None).await;
    let b = pass(&service, "b", Some(&a.header.id)).await;
    let c = pass(&service, "c", Some(&b.header.id)).await;

    for parent in [&a.header.id, &c.header.id, &b.header.id] {
        let result = service
            .pass_set_parent(
                &Caller::User,
                a.header.id.clone(),
                token(&a),
                Some(parent.clone()),
            )
            .await;
        assert_eq!(
            refusal(&result),
            Some("parent_loop"),
            "a under {parent} must be refused"
        );
    }
    // The pass further down the chain may still move up.
    let moved = service
        .pass_set_parent(
            &Caller::User,
            c.header.id.clone(),
            token(&c),
            Some(a.header.id.clone()),
        )
        .await
        .expect("c under a");
    assert_eq!(
        moved.header.parent_id.as_deref(),
        Some(a.header.id.as_str())
    );
}

#[tokio::test]
async fn a_parent_that_is_not_in_the_vault_is_accepted() {
    let (_dir, _db, service) = setup();
    let a = pass(&service, "a", None).await;
    let soft = service
        .pass_set_parent(
            &Caller::User,
            a.header.id.clone(),
            token(&a),
            Some("not-synced-yet".to_owned()),
        )
        .await
        .expect("a soft link");
    assert_eq!(soft.header.parent_id.as_deref(), Some("not-synced-yet"));
}

#[tokio::test]
async fn the_walk_ends_on_a_loop_that_is_already_stored() {
    let (_dir, db, service) = setup();
    let x = pass(&service, "x", None).await;
    let y = pass(&service, "y", Some(&x.header.id)).await;
    // Two devices linked in opposite directions: x under y as well.
    db.with_connection(|c| {
        c.execute(
            "UPDATE haex_passwords_pass_details SET parent_id = ?2 WHERE item_id = ?1",
            [&x.header.id, &y.header.id],
        )?;
        Ok(())
    })
    .expect("a stored loop");

    let z = pass(&service, "z", None).await;
    // z under x walks x, y, x, y ... and has to end, not hang.
    let under_loop = service
        .pass_set_parent(
            &Caller::User,
            z.header.id.clone(),
            token(&z),
            Some(x.header.id.clone()),
        )
        .await
        .expect("z may join the passes that loop");
    assert_eq!(
        under_loop.header.parent_id.as_deref(),
        Some(x.header.id.as_str())
    );
    db.write(|tx| {
        store::ensure_no_parent_loop(tx, "someone-else", &x.header.id).map_err(Into::into)
    })
    .expect("the walk ends");

    // The loop can be repaired by clearing one link.
    let fresh = service
        .get_item(&Caller::User, x.header.id.clone())
        .await
        .expect("x");
    let repaired = service
        .pass_set_parent(&Caller::User, x.header.id.clone(), token(&fresh), None)
        .await
        .expect("clear the link of x");
    assert_eq!(repaired.header.parent_id, None);
}

#[tokio::test]
async fn setting_the_parent_needs_the_current_token_and_a_pass() {
    let (_dir, _db, service) = setup();
    let a = pass(&service, "a", None).await;
    let stale = service
        .pass_set_parent(
            &Caller::User,
            a.header.id.clone(),
            "stale".to_owned(),
            Some("p".to_owned()),
        )
        .await;
    assert!(
        matches!(&stale, Err(HolziError::PasswordsConflict { reason }) if reason == "changed"),
        "{stale:?}"
    );
    let login = service
        .create_item(&Caller::User, &[], ItemInput::default(), None)
        .await
        .expect("login");
    let not_a_pass = service
        .pass_set_parent(&Caller::User, login, "x".to_owned(), Some("p".to_owned()))
        .await;
    assert_eq!(refusal(&not_a_pass), Some("not_a_pass"), "{not_a_pass:?}");
}

#[tokio::test]
async fn trashing_the_parent_changes_nothing_in_its_pass() {
    let (_dir, db, service) = setup();
    let parent = pass(&service, "Card", None).await;
    let child = pass(&service, "Voucher", Some(&parent.header.id)).await;
    let before = service
        .get_item(&Caller::User, child.header.id.clone())
        .await
        .expect("child");

    service
        .trash_targets(
            &Caller::User,
            vec![Target {
                kind: TargetKind::Item,
                id: parent.header.id.clone(),
            }],
        )
        .await
        .expect("trash the parent");

    let after = service
        .get_item(&Caller::User, child.header.id.clone())
        .await
        .expect("child");
    assert_eq!(after.header.updated_at, before.header.updated_at);
    assert_eq!(after.header.group_id, before.header.group_id);
    assert_eq!(
        parent_of(&db, &child.header.id).as_deref(),
        Some(parent.header.id.as_str())
    );
}
