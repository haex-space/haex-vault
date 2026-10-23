//! Acceptance checks for the measurable criteria of spec 051 (T095): SC-001 (the first run with
//! 2,000 contacts in under 60 s), SC-002 and SC-003 (a change of the phone reaches the address book
//! and one from the vault reaches the phone in under 5 s), SC-005 (two phones with the same 500
//! contacts give 500 contacts) and SC-007 (the search rule under 200 ms per input). They
//! run against the fake phone, so they measure holzi's own work; the phone's Contacts Provider is
//! checked by hand (quickstart M1, M2, M6). The times hold in a debug build, so a release build on a
//! phone has room to spare.

use std::time::{Duration, Instant};

use serde_json::json;

use super::test_rig::Rig;
use super::test_support::{email, name, phone, FakePhoneBook, GOOGLE};
use super::*;
use crate::contacts::model::{ContactPatch, ValueUpsert, ValuesPatch};
use crate::contacts::search;
use crate::contacts::service::test_support::{account, add_phone};
use crate::contacts::service::Caller;

/// Fills `book` with `count` people in the Google account, each with a phone number and an
/// e-mail address; the same `count` gives the same people.
fn fill(book: &FakePhoneBook, count: usize) -> Vec<i64> {
    (0..count)
        .map(|n| {
            book.add(
                GOOGLE,
                vec![
                    name(&format!("Vorname{n}"), &format!("Nachname{n}")),
                    phone(&format!("0170 {n:07}"), 2),
                    email(&format!("person{n}@example.org")),
                ],
            )
        })
        .collect()
}

fn within(started: Instant, limit: Duration, what: &str) {
    let took = started.elapsed();
    assert!(took < limit, "{what} took {took:?}, more than {limit:?}");
}

/// A phone with 2,000 contacts after its first run, which must take under 60 s (SC-001).
fn synced_with_2000() -> (Rig, Vec<i64>) {
    let rig = Rig::new();
    let raws = fill(&rig.book, 2_000);
    rig.enable();
    let started = Instant::now();
    rig.run(Mode::First(&[])).expect("the first run");
    within(
        started,
        Duration::from_secs(60),
        "the first run of 2,000 contacts",
    );
    assert_eq!(rig.headers().len(), 2_000);
    (rig, raws)
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
async fn two_thousand_contacts_sync_in_time_both_ways() {
    let (rig, raws) = synced_with_2000();

    // SC-002: a change on the phone, a run when holzi comes back to the foreground.
    let raw = raws[1_234];
    rig.book.edit(
        raw,
        data_id(&rig, raw, "Phone"),
        json!({ "value": "0170 9999999" }),
    );
    let started = Instant::now();
    let summary = rig.run(Mode::Normal).expect("run");
    within(
        started,
        Duration::from_secs(5),
        "taking over a phone change",
    );
    assert_eq!(summary.counts.updated, 1);
    let changed = rig.named("Vorname1234 Nachname1234");
    assert!(
        changed
            .values
            .iter()
            .any(|value| value.value == "0170 9999999"),
        "{:?}",
        changed.values
    );

    // SC-003: a change that came from another device through the vault sync.
    let other = rig.named("Vorname42 Nachname42");
    let patch = ContactPatch {
        values: Some(ValuesPatch {
            upsert: vec![ValueUpsert {
                id: None,
                kind: "email".to_owned(),
                label: "work".to_owned(),
                value: "neu42@example.org".to_owned(),
                position: None,
            }],
            remove: vec![],
        }),
        ..ContactPatch::default()
    };
    rig.vault
        .service
        .update(&Caller::User, &[], &other.id, patch)
        .await
        .expect("update");
    let started = Instant::now();
    let summary = rig.run(Mode::Changes).expect("run");
    within(
        started,
        Duration::from_secs(5),
        "writing a vault change back",
    );
    assert_eq!(summary.counts.written, 1);
    assert!(rig
        .book
        .values(raws[42], "Email")
        .contains(&"neu42@example.org".to_owned()));
}

#[tokio::test]
async fn two_phones_with_the_same_500_contacts_give_500_contacts() {
    let rig = Rig::new();
    fill(&rig.book, 500);
    rig.start();
    assert_eq!(rig.headers().len(), 500);

    // The second phone of the same person: its own device, the same Google account.
    let second = FakePhoneBook::new();
    fill(&second, 500);
    let device = add_phone(
        &rig.vault,
        "Zweites Telefon",
        true,
        vec![account(GOOGLE.0, GOOGLE.1)],
    );
    let summary = run(
        &rig.vault.db,
        &second,
        &device,
        Mode::First(&[]),
        &crate::passwords::clock::now(),
    )
    .expect("the second phone's first run");
    assert_eq!(rig.headers().len(), 500, "no duplicates: {summary:?}");
    assert_eq!(summary.counts.imported, 0);
    rig.run(Mode::Normal).expect("the first phone again");
    assert_eq!(rig.headers().len(), 500);
}

#[test]
fn the_search_rule_answers_each_input_over_five_thousand_contacts_in_time() {
    let rig = Rig::new();
    fill(&rig.book, 5_000);
    rig.start();
    // SC-007: the rule the window runs on the headers it holds (`src/lib/contacts/search.ts`,
    // checked for speed in `scripts/check-contacts-search.ts`) and the service runs for agents and
    // extensions after reading the headers.
    let headers = rig.headers();
    assert_eq!(headers.len(), 5_000);
    let haystacks: Vec<_> = headers.iter().map(search::haystack).collect();
    for query in [
        "v",
        "vorname4",
        "vorname4999",
        "nachname12",
        "0170 0004",
        "example.org",
    ] {
        let started = Instant::now();
        let found = haystacks
            .iter()
            .filter(|haystack| search::matches(haystack, query))
            .count();
        within(
            started,
            Duration::from_millis(200),
            &format!("searching {query:?}"),
        );
        assert!(found > 0, "{query:?}");
    }
}
