//! Runs after the first one against the fake phone (spec 051 T034, T035): changes both ways,
//! deletions and the guard, accounts that leave, the permission and failures.

use serde_json::json;

use super::*;
use crate::contacts::model::{ContactPatch, ValueUpsert, ValuesPatch};
use crate::contacts::phone::state::PhoneState;
use crate::contacts::phone::test_rig::Rig;
use crate::contacts::phone::test_support::{email, name, phone, GOOGLE, LOCAL};
use crate::contacts::phone::{Counts, IssueReason, MassDecision, Paused};
use crate::contacts::service::Caller;

/// A phone with Anna in the Google account, synced.
fn synced() -> (Rig, i64, String) {
    let rig = Rig::new();
    let raw = rig
        .book
        .add(GOOGLE, vec![name("Anna", "Beck"), phone("0170 1234567", 2)]);
    rig.start();
    let id = rig.named("Anna Beck").id;
    (rig, raw, id)
}

fn data_id(rig: &Rig, raw: i64, mimetype: &str) -> i64 {
    rig.book
        .raw(raw)
        .expect("raw")
        .rows
        .iter()
        .find(|row| row.mimetype == mimetype)
        .expect("row")
        .data_id
}

#[tokio::test]
async fn a_change_on_the_phone_reaches_holzi() {
    let (rig, raw, id) = synced();
    rig.book.edit(
        raw,
        data_id(&rig, raw, "Phone"),
        json!({ "value": "0170 7654321" }),
    );
    let summary = rig.run(Mode::Normal).expect("run");
    assert_eq!(summary.counts.updated, 1);
    let phones: Vec<String> = rig
        .contact(&id)
        .values
        .into_iter()
        .map(|value| value.value)
        .collect();
    assert_eq!(phones, vec!["0170 7654321"]);
    assert!(rig.book.ops().is_empty());
}

#[tokio::test]
async fn a_change_in_holzi_reaches_the_raw_contact_in_its_account() {
    let (rig, raw, id) = synced();
    let patch = ContactPatch {
        values: Some(ValuesPatch {
            upsert: vec![ValueUpsert {
                id: None,
                kind: "email".to_owned(),
                label: "home".to_owned(),
                value: "anna@example.org".to_owned(),
                position: None,
            }],
            remove: vec![],
        }),
        ..ContactPatch::default()
    };
    rig.vault
        .service
        .update(&Caller::User, &[], &id, patch)
        .await
        .expect("update");
    let summary = rig.run(Mode::Normal).expect("run");
    assert_eq!(summary.counts.written, 1);
    assert_eq!(rig.book.values(raw, "Email"), vec!["anna@example.org"]);

    // holzi's own write does not come back as a change of the phone.
    let ops = rig.book.ops().len();
    let again = rig.run(Mode::Normal).expect("again");
    assert_eq!(again.counts, Counts::default());
    assert_eq!(rig.book.ops().len(), ops);
}

#[tokio::test]
async fn values_added_and_removed_on_the_phone_reach_holzi() {
    let (rig, raw, id) = synced();
    rig.book.add_row(raw, email("anna@example.org"));
    rig.book.remove_row(raw, data_id(&rig, raw, "Phone"));
    rig.run(Mode::Normal).expect("run");
    let values: Vec<(String, String)> = rig
        .contact(&id)
        .values
        .into_iter()
        .map(|value| (value.kind, value.value))
        .collect();
    assert_eq!(
        values,
        vec![("email".to_owned(), "anna@example.org".to_owned())]
    );
}

#[tokio::test]
async fn a_read_only_raw_contact_is_only_a_source() {
    let (rig, raw, id) = synced();
    rig.book.set_read_only(raw);
    rig.book.edit(
        raw,
        data_id(&rig, raw, "Phone"),
        json!({ "value": "0170 7654321" }),
    );
    rig.run(Mode::Normal).expect("run");
    assert_eq!(
        rig.contact(&id).values[0].value,
        "0170 7654321",
        "read from it"
    );
    let patch = ContactPatch {
        values: Some(ValuesPatch {
            upsert: vec![ValueUpsert {
                id: None,
                kind: "email".to_owned(),
                label: "home".to_owned(),
                value: "anna@example.org".to_owned(),
                position: None,
            }],
            remove: vec![],
        }),
        ..ContactPatch::default()
    };
    rig.vault
        .service
        .update(&Caller::User, &[], &id, patch)
        .await
        .expect("update");
    rig.run(Mode::Normal).expect("run");
    assert!(rig.book.values(raw, "Email").is_empty(), "never written");
    assert_eq!(rig.status().issues[0].reason, IssueReason::ReadOnly);
}

#[tokio::test]
async fn a_contact_deleted_on_the_phone_goes_from_holzi() {
    let (rig, raw, id) = synced();
    rig.book.delete(raw);
    let summary = rig.run(Mode::Normal).expect("run");
    assert_eq!(summary.counts.deleted, 1);
    assert!(rig.headers().iter().all(|header| header.id != id));
}

#[tokio::test]
async fn a_contact_deleted_in_holzi_goes_from_the_phone() {
    let (rig, raw, id) = synced();
    rig.vault
        .service
        .delete(&Caller::User, &[], vec![id])
        .await
        .expect("delete");
    let summary = rig.run(Mode::Normal).expect("run");
    assert_eq!(summary.counts.deleted, 1);
    assert!(rig.book.raw(raw).is_none());
    // The next run takes nothing over again.
    rig.run(Mode::Normal).expect("again");
    assert!(rig.headers().is_empty());
}

#[tokio::test]
async fn a_new_contact_on_the_phone_comes_to_holzi() {
    let (rig, _, _) = synced();
    rig.book.add(
        LOCAL,
        vec![name("Bernd", "Clausen"), email("bernd@example.org")],
    );
    let summary = rig.run(Mode::Normal).expect("run");
    assert_eq!(summary.counts.imported, 1);
    assert_eq!(rig.headers().len(), 2);
}

fn many(rig: &Rig, count: usize) -> Vec<i64> {
    (0..count)
        .map(|n| {
            rig.book.add(
                LOCAL,
                vec![
                    name("Person", &format!("{n:03}")),
                    phone(&format!("0170 {n:07}"), 2),
                ],
            )
        })
        .collect()
}

#[tokio::test]
async fn deleting_many_on_the_phone_waits_for_the_person() {
    let rig = Rig::new();
    let raws = many(&rig, 100);
    rig.start();
    for raw in &raws[..11] {
        rig.book.delete(*raw);
    }
    let summary = rig.run(Mode::Normal).expect("run");
    assert_eq!(summary.paused, Some(Paused::MassDelete));
    assert_eq!(
        summary.mass_delete.as_ref().map(|held| held.from_phone),
        Some(11)
    );
    assert_eq!(rig.headers().len(), 100, "nothing deleted yet");
    let status = rig.status();
    assert_eq!(status.state, PhoneState::Paused);
    assert_eq!(status.mass_delete.map(|held| held.from_phone), Some(11));
    assert_eq!(
        rig.run(Mode::Normal).expect("still paused").paused,
        Some(Paused::MassDelete)
    );

    // "Keep": the contacts stay in holzi and go to the phone again.
    let summary = rig.run(Mode::Decided(MassDecision::Keep)).expect("keep");
    assert_eq!(summary.paused, None);
    assert_eq!(rig.headers().len(), 100);
    assert_eq!(rig.book.live().len(), 100, "re-created on the phone");
    assert_eq!(rig.status().state, PhoneState::Active);
}

#[tokio::test]
async fn deleting_many_goes_ahead_when_the_person_says_so() {
    let rig = Rig::new();
    let raws = many(&rig, 100);
    rig.start();
    for raw in &raws[..11] {
        rig.book.delete(*raw);
    }
    rig.run(Mode::Normal).expect("paused");
    let summary = rig
        .run(Mode::Decided(MassDecision::Delete))
        .expect("delete");
    assert_eq!(summary.counts.deleted, 11);
    assert_eq!(rig.headers().len(), 89);
}

#[tokio::test]
async fn ten_deletions_go_ahead_without_asking() {
    let rig = Rig::new();
    let raws = many(&rig, 50);
    rig.start();
    for raw in &raws[..10] {
        rig.book.delete(*raw);
    }
    let summary = rig.run(Mode::Normal).expect("run");
    assert_eq!(summary.paused, None);
    assert_eq!(rig.headers().len(), 40);
}

#[tokio::test]
async fn an_account_removed_from_the_phone_deletes_nothing() {
    let rig = Rig::new();
    let work = ("com.work", "mara@work.example");
    rig.book.add_account(work, true);
    for n in 0..20 {
        rig.book.add(
            work,
            vec![
                name("Kollege", &format!("{n:02}")),
                phone(&format!("0170 {n:07}"), 3),
            ],
        );
    }
    rig.start();
    rig.book.remove_account(work);
    let summary = rig.run(Mode::Normal).expect("run");
    assert_eq!(summary.paused, None, "no mass deletion either (FR-026)");
    assert_eq!(rig.headers().len(), 20);
    assert!(rig.links().is_empty(), "the links went");
    let issues = rig.status().issues;
    assert_eq!(issues.len(), 20, "their target account is gone");
    assert!(issues
        .iter()
        .all(|issue| issue.reason == IssueReason::AccountRemoved));
}

#[tokio::test]
async fn a_missing_permission_pauses_without_changing_anything() {
    let (rig, raw, _) = synced();
    rig.book.set_permitted(false);
    rig.book.delete(raw);
    let summary = rig.run(Mode::Normal).expect("run");
    assert_eq!(summary.paused, Some(Paused::Permission));
    assert_eq!(rig.status().state, PhoneState::Paused);
    assert_eq!(rig.headers().len(), 1);
    rig.book.set_permitted(true);
    rig.run(Mode::Normal).expect("resumed");
    assert_eq!(rig.status().state, PhoneState::Active);
}

#[tokio::test]
async fn a_change_a_read_only_account_cannot_take_stays_and_is_shown() {
    let rig = Rig::new();
    let messenger = ("com.messenger", "anna");
    rig.book.add_account(messenger, false);
    let raw = rig.book.add(
        messenger,
        vec![name("Anna", "Beck"), phone("0170 1234567", 2)],
    );
    rig.start();
    let id = rig.named("Anna Beck").id;
    let contact = rig.contact(&id);
    let number = contact.values[0].clone();
    let patch = ContactPatch {
        values: Some(ValuesPatch {
            upsert: vec![ValueUpsert {
                id: Some(number.id),
                kind: "phone".to_owned(),
                label: "mobile".to_owned(),
                value: "0170 7654321".to_owned(),
                position: None,
            }],
            remove: vec![],
        }),
        ..ContactPatch::default()
    };
    rig.vault
        .service
        .update(&Caller::User, &[], &id, patch)
        .await
        .expect("update");
    rig.run(Mode::Normal).expect("run");
    assert_eq!(
        rig.book.values(raw, "Phone"),
        vec!["0170 1234567"],
        "the phone keeps its number"
    );
    assert_eq!(
        rig.contact(&id).values[0].value,
        "0170 7654321",
        "holzi keeps its change"
    );
    assert_eq!(rig.status().issues[0].reason, IssueReason::ReadOnly);
}

#[tokio::test]
async fn a_refused_batch_is_shown_and_tried_again() {
    let (rig, raw, id) = synced();
    let patch = ContactPatch {
        values: Some(ValuesPatch {
            upsert: vec![ValueUpsert {
                id: None,
                kind: "email".to_owned(),
                label: "home".to_owned(),
                value: "anna@example.org".to_owned(),
                position: None,
            }],
            remove: vec![],
        }),
        ..ContactPatch::default()
    };
    rig.vault
        .service
        .update(&Caller::User, &[], &id, patch)
        .await
        .expect("update");
    rig.book.fail_next_apply();
    rig.run(Mode::Normal).expect("run");
    assert_eq!(rig.status().issues[0].reason, IssueReason::Failed);
    assert!(rig.book.values(raw, "Email").is_empty());
    rig.run(Mode::Normal).expect("again");
    assert_eq!(rig.book.values(raw, "Email"), vec!["anna@example.org"]);
    assert!(
        rig.status().issues.is_empty(),
        "the issue clears once the change arrived"
    );
}
