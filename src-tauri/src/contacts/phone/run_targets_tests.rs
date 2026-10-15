//! Runs with sync targets against the fake phone (spec 051 T052, US3): contacts made elsewhere
//! go to the phone only with a target, into the target's account; a removed target deletes the
//! contact on the phone or lets it go; a target set again finds what the phone kept.

use tauri_plugin_holzi_android::contacts::Op;

use super::*;
use crate::contacts::model::{ContactInput, ContactPatch, Head, TargetInput, ValueInput};
use crate::contacts::phone::test_rig::Rig;
use crate::contacts::phone::test_support::{name, phone, GOOGLE};
use crate::contacts::phone::Account;
use crate::contacts::service::{Caller, OnPhone, Selection};

const WORK: (&str, &str) = ("com.example.work", "firma");

fn account((kind, name): (&str, &str)) -> Account {
    Account {
        account_type: kind.to_owned(),
        name: name.to_owned(),
        writable: true,
        is_default: false,
    }
}

fn value(kind: &str, label: &str, value: &str) -> ValueInput {
    ValueInput {
        kind: kind.to_owned(),
        label: label.to_owned(),
        value: value.to_owned(),
    }
}

/// The sync on and the first run done, with an empty phone or what the test added before.
fn active() -> Rig {
    let rig = Rig::new();
    rig.start();
    rig
}

async fn create(rig: &Rig, given: &str, values: Vec<ValueInput>, to: Option<Account>) -> String {
    let input = ContactInput {
        head: Head {
            given_name: Some(given.to_owned()),
            family_name: Some("Beck".to_owned()),
            ..Head::default()
        },
        values,
        groups: vec![],
        targets: to
            .map(|account| TargetInput {
                device: rig.device.clone(),
                account: Some(account),
            })
            .into_iter()
            .collect(),
    };
    rig.vault
        .service
        .create(&Caller::User, &[], input)
        .await
        .expect("create")
        .id
}

async fn remove(rig: &Rig, id: &str, on_phone: OnPhone) {
    rig.vault
        .service
        .targets_remove(
            &Caller::User,
            vec![id.to_owned()],
            rig.device.clone(),
            on_phone,
        )
        .await
        .expect("remove the target");
}

fn inserts(ops: &[Op]) -> usize {
    ops.iter()
        .filter(|op| matches!(op, Op::InsertRaw { .. }))
        .count()
}

#[tokio::test]
async fn a_contact_made_in_holzi_reaches_the_phone_only_with_a_target() {
    let rig = active();
    create(
        &rig,
        "Anna",
        vec![
            value("phone", "mobile", "0170 1234567"),
            value("email", "work", "anna@example.org"),
            value("website", "home", "https://anna.example.org"),
            value(
                "address",
                "home",
                r#"{"street":"Hauptstr. 1","city":"Berlin"}"#,
            ),
            value("event", "birthday", r#"{"date":"1990-05-04"}"#),
            value("custom", "custom:Kundennummer", "4711"),
        ],
        Some(account(GOOGLE)),
    )
    .await;
    create(
        &rig,
        "Ben",
        vec![value("phone", "mobile", "0170 7654321")],
        None,
    )
    .await;

    let summary = rig.run(Mode::Normal).expect("run");
    assert_eq!(summary.counts.written, 1);
    let live = rig.book.live();
    assert_eq!(live.len(), 1, "Ben stays only in holzi");
    let anna = &live[0];
    assert_eq!(
        (
            anna.index.account_type.as_str(),
            anna.index.account_name.as_str()
        ),
        GOOGLE
    );
    let mimetypes: Vec<&str> = anna.rows.iter().map(|row| row.mimetype.as_str()).collect();
    for mimetype in [
        "StructuredName",
        "Phone",
        "Email",
        "Website",
        "StructuredPostal",
        "Event",
    ] {
        assert!(mimetypes.contains(&mimetype), "{mimetype} in {mimetypes:?}");
    }
    assert_eq!(mimetypes.len(), 6, "the custom value stays in holzi");
}

#[tokio::test]
async fn a_change_goes_to_the_account_the_contact_lies_in() {
    let rig = Rig::new();
    rig.book.add_account(WORK, true);
    let raw = rig
        .book
        .add(WORK, vec![name("Carla", "Beck"), phone("030 5550101", 3)]);
    rig.start();
    let id = rig.named("Carla Beck").id;
    // New contacts go into Google from now on; Carla stays where she is.
    let device = rig.device.clone();
    rig.vault
        .db
        .write_blocking(move |tx| {
            crate::contacts::phone::state::set_target(tx, &device, &account(GOOGLE))
                .map_err(haex_crdt::Error::from)
        })
        .expect("target account");
    let contact = rig.contact(&id);
    let patch = ContactPatch {
        values: Some(crate::contacts::model::ValuesPatch {
            upsert: vec![crate::contacts::model::ValueUpsert {
                id: Some(contact.values[0].id.clone()),
                kind: "phone".into(),
                label: "work".into(),
                value: "030 5550202".into(),
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
    assert_eq!(rig.book.values(raw, "Phone"), ["030 5550202"]);
    assert_eq!(
        inserts(&rig.book.ops()),
        0,
        "nothing new in the Google account"
    );
}

#[tokio::test]
async fn a_target_removed_with_delete_deletes_the_contact_on_the_phone_only() {
    let rig = Rig::new();
    let raw = rig.book.add(GOOGLE, vec![name("Anna", "Beck")]);
    rig.start();
    let id = rig.named("Anna Beck").id;
    remove(&rig, &id, OnPhone::Delete).await;

    let summary = rig.run(Mode::Normal).expect("run");
    assert_eq!(summary.counts.deleted, 1);
    assert!(summary.paused.is_none(), "the user decided, no guard");
    assert!(rig.book.raw(raw).is_none_or(|raw| raw.index.deleted));
    assert_eq!(rig.contact(&id).name, "Anna Beck", "it stays in holzi");
    assert!(rig.links().is_empty());

    rig.run(Mode::Normal).expect("again");
    assert_eq!(rig.headers().len(), 1, "nothing comes back");
}

#[tokio::test]
async fn many_targets_removed_with_delete_are_not_held_back() {
    let rig = Rig::new();
    for n in 0..15 {
        rig.book
            .add(GOOGLE, vec![name(&format!("Kontakt{n}"), "Beck")]);
    }
    rig.start();
    let ids: Vec<String> = rig.headers().into_iter().map(|header| header.id).collect();
    rig.vault
        .service
        .targets_remove(&Caller::User, ids, rig.device.clone(), OnPhone::Delete)
        .await
        .expect("remove");
    let summary = rig.run(Mode::Normal).expect("run");
    assert!(summary.paused.is_none());
    assert!(rig.book.live().is_empty());
    assert_eq!(rig.headers().len(), 15);
}

#[tokio::test]
async fn a_target_removed_with_keep_leaves_the_phone_alone_until_it_is_set_again() {
    let rig = Rig::new();
    let raw = rig
        .book
        .add(GOOGLE, vec![name("Anna", "Beck"), phone("0170 1234567", 2)]);
    rig.start();
    let id = rig.named("Anna Beck").id;
    remove(&rig, &id, OnPhone::Keep).await;
    let before = rig.book.ops().len();
    rig.run(Mode::Normal).expect("run");
    assert_eq!(rig.book.ops().len(), before, "the phone is not touched");
    assert!(rig.book.raw(raw).is_some_and(|raw| !raw.index.deleted));
    assert_eq!(rig.headers().len(), 1, "not taken over a second time");

    // A change on the phone no longer reaches holzi.
    rig.book.add_row(raw, phone("030 5550101", 1));
    rig.run(Mode::Normal).expect("run");
    assert_eq!(rig.contact(&id).values.len(), 1);

    // Set again, the contact finds the raw contact the phone kept.
    rig.vault
        .service
        .targets_set(
            &Caller::User,
            Selection::Contacts(vec![id.clone()]),
            rig.device.clone(),
            Some(account(GOOGLE)),
        )
        .await
        .expect("target again");
    rig.run(Mode::Normal).expect("run");
    assert_eq!(inserts(&rig.book.ops()), 0, "no second raw contact");
    assert_eq!(
        rig.links()
            .iter()
            .map(|link| link.raw_contact_id)
            .collect::<Vec<_>>(),
        [raw]
    );
    assert_eq!(
        rig.contact(&id).values.len(),
        2,
        "the phone's new number came along"
    );
}

#[tokio::test]
async fn forty_contacts_with_a_new_target_are_all_written() {
    let rig = active();
    let mut ids = Vec::new();
    for n in 0..40 {
        ids.push(create(&rig, &format!("Kontakt{n}"), vec![], None).await);
    }
    rig.vault
        .service
        .targets_set(
            &Caller::User,
            Selection::Contacts(ids),
            rig.device.clone(),
            Some(account(GOOGLE)),
        )
        .await
        .expect("set");
    let summary = rig.run(Mode::Normal).expect("run");
    assert_eq!(summary.counts.written, 40);
    assert_eq!(rig.book.live().len(), 40);
}

#[tokio::test]
async fn a_read_only_raw_contact_of_a_removed_target_is_only_let_go() {
    let rig = Rig::new();
    let raw = rig.book.add(GOOGLE, vec![name("Anna", "Beck")]);
    rig.book.set_read_only(raw);
    rig.start();
    let id = rig.named("Anna Beck").id;
    remove(&rig, &id, OnPhone::Delete).await;
    rig.run(Mode::Normal).expect("run");
    assert!(rig.book.raw(raw).is_some_and(|raw| !raw.index.deleted));
    assert!(rig.links().is_empty());
    rig.run(Mode::Normal).expect("again");
    assert_eq!(rig.headers().len(), 1, "not taken over again");
}

#[tokio::test]
async fn a_run_tells_the_other_devices_the_phones_accounts() {
    let rig = active();
    let phones = rig
        .vault
        .service
        .phones(&Caller::User)
        .await
        .expect("phones");
    assert_eq!(phones.len(), 1);
    let accounts: Vec<(&str, &str)> = phones[0]
        .accounts
        .iter()
        .map(|account| (account.account_type.as_str(), account.name.as_str()))
        .collect();
    assert_eq!(accounts, [("", ""), GOOGLE]);
}

#[tokio::test]
async fn a_run_for_changes_in_the_vault_leaves_the_phones_deletions_to_a_full_run() {
    let rig = Rig::new();
    let raws: Vec<i64> = (0..12)
        .map(|n| {
            rig.book
                .add(GOOGLE, vec![name(&format!("Kontakt{n}"), "Beck")])
        })
        .collect();
    rig.start();
    // The phone deletes one by one; runs for changes in the vault come in between.
    for raw in &raws[..11] {
        rig.book.delete(*raw);
        let summary = rig.run(Mode::Changes).expect("a run for changes");
        assert_eq!(summary.counts.deleted, 0);
    }
    assert_eq!(rig.headers().len(), 12, "nothing deleted piece by piece");
    let summary = rig.run(Mode::Normal).expect("a full run");
    assert!(
        summary.paused.is_some(),
        "the full run sees all eleven at once and asks"
    );
}
