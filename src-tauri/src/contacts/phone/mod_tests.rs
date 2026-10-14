//! Whole runs against the fake phone (spec 051 T033, T036): the state machine, the first run,
//! taking over and merging, aggregated contacts and paging.

use serde_json::json;

use super::state::PhoneState;
use super::test_rig::Rig;
use super::test_support::{email, name, phone, row, GOOGLE, LOCAL};
use super::*;
use crate::contacts::model::OriginKind;
use crate::contacts::service::test_support::{person, value};
use crate::contacts::service::Caller;
use crate::error::HolziError;

#[tokio::test]
async fn no_run_while_off_and_none_before_the_overview_is_confirmed() {
    let rig = Rig::new();
    rig.book.add(GOOGLE, vec![name("Anna", "Beck")]);
    assert!(
        matches!(
            rig.run(Mode::Normal),
            Err(HolziError::ContactsInvalid { .. })
        ),
        "off: no row"
    );
    rig.enable();
    assert_eq!(
        rig.run(Mode::Normal).expect("run"),
        RunSummary::default(),
        "the overview waits"
    );
    assert!(rig.headers().is_empty());
}

#[tokio::test]
async fn the_first_run_takes_over_every_account_and_goes_active() {
    let rig = Rig::new();
    let anna = rig.book.add(
        GOOGLE,
        vec![
            name("Anna", "Beck"),
            phone("+49 170 1234567", 2),
            email("anna@example.org"),
            row(
                "Organization",
                json!({ "company": "Beck GmbH", "title": "Chefin" }),
            ),
            row(
                "StructuredPostal",
                json!({ "street": "Markt 1", "city": "Leipzig", "type": "1" }),
            ),
            row("Event", json!({ "startDate": "1990-05-17", "type": "3" })),
            row("vnd.com.whatsapp.profile", json!({ "data1": "x" })),
        ],
    );
    rig.book.add(
        LOCAL,
        vec![name("Bernd", "Clausen"), phone("0171 7654321", 1)],
    );
    rig.enable();
    let review = rig.review();
    assert_eq!(
        (review.to_address_book, review.merged, review.to_phone),
        (2, 0, 0)
    );

    let summary = rig.run(Mode::First(&[])).expect("first run");
    assert_eq!(summary.counts.imported, 2);
    assert_eq!(rig.status().state, PhoneState::Active);

    let contact = rig.named("Anna Beck");
    assert_eq!(contact.head.organization.as_deref(), Some("Beck GmbH"));
    assert_eq!(contact.head.job_title.as_deref(), Some("Chefin"));
    let kinds: Vec<&str> = contact
        .values
        .iter()
        .map(|value| value.kind.as_str())
        .collect();
    assert_eq!(kinds, vec!["address", "email", "event", "phone"]);
    assert_eq!(contact.origin.kind, OriginKind::Phone);
    assert_eq!(
        contact.origin.account.as_deref(),
        Some("com.google/mara@example.org")
    );
    assert_eq!(contact.origin.device, rig.device);

    let bernd = rig.named("Bernd Clausen");
    assert_eq!(
        bernd.origin.account.as_deref(),
        Some(""),
        "the local account"
    );
    let targets = rig.targets();
    assert_eq!(
        targets.len(),
        2,
        "each contact has its phone as a target (FR-018b)"
    );
    let anna_target = targets
        .iter()
        .find(|target| target.contact_id == contact.id)
        .expect("a target");
    assert_eq!(
        (
            anna_target.account_type.as_str(),
            anna_target.account_name.as_str()
        ),
        GOOGLE
    );
    assert!(rig.links().iter().any(|link| link.raw_contact_id == anna));
    assert!(
        rig.book.ops().is_empty(),
        "taking over writes nothing to the phone"
    );
}

#[tokio::test]
async fn raw_contacts_of_one_person_become_one_contact_with_every_value_once() {
    let rig = Rig::new();
    let google = rig
        .book
        .add(GOOGLE, vec![name("Anna", "Beck"), phone("0170 1234567", 2)]);
    let local = rig.book.add(
        LOCAL,
        vec![
            name("Anna", "Beck"),
            phone("0170 123 4567", 2),
            email("anna@example.org"),
        ],
    );
    rig.book.join(local, google);
    rig.start();
    let headers = rig.headers();
    assert_eq!(headers.len(), 1);
    let contact = rig.contact(&headers[0].id);
    assert_eq!(
        contact
            .values
            .iter()
            .filter(|value| value.kind == "phone")
            .count(),
        1
    );
    assert_eq!(
        contact
            .values
            .iter()
            .filter(|value| value.kind == "email")
            .count(),
        1
    );
    assert_eq!(
        rig.links().len(),
        2,
        "each raw contact has its own link (research R7)"
    );
}

#[tokio::test]
async fn a_clear_match_merges_and_keeps_the_older_origin() {
    let rig = Rig::new();
    let holzi = rig
        .vault
        .service
        .create(
            &Caller::User,
            &[],
            person(
                "Beck",
                "Anna",
                vec![
                    value("phone", "mobile", "0170 1234567"),
                    value("website", "home", "https://anna.example"),
                    value("custom", "custom:Kundennummer", "4711"),
                ],
            ),
        )
        .await
        .expect("create");
    let raw = rig.book.add(
        GOOGLE,
        vec![
            name("Anna", "Beck"),
            phone("+49 170 1234567", 2),
            email("anna@example.org"),
        ],
    );
    rig.enable();
    assert_eq!(rig.review().merged, 1);
    let summary = rig.run(Mode::First(&[])).expect("first run");
    assert_eq!(summary.counts.merged, 1);
    assert_eq!(rig.headers().len(), 1);
    let contact = rig.contact(&holzi.id);
    assert_eq!(contact.origin.kind, OriginKind::Holzi, "FR-018a");
    assert!(contact
        .values
        .iter()
        .any(|value| value.value == "anna@example.org"));
    assert_eq!(
        contact
            .values
            .iter()
            .filter(|value| value.kind == "phone")
            .count(),
        1,
        "one number in two forms"
    );
    let target = rig
        .targets()
        .into_iter()
        .find(|target| target.contact_id == holzi.id)
        .expect("FR-018b");
    assert_eq!(target.account_type, GOOGLE.0);
    assert_eq!(
        rig.book.values(raw, "Website"),
        vec!["https://anna.example"],
        "holzi's value went to the phone"
    );
    let raw_now = rig.book.raw(raw).expect("raw");
    assert!(
        raw_now
            .rows
            .iter()
            .all(|row| row.fields.get("value") != Some(&json!("4711"))),
        "never a custom value"
    );
}

#[tokio::test]
async fn an_uncertain_pair_stays_apart_unless_the_person_merges_it() {
    let rig = Rig::new();
    let holzi = rig
        .vault
        .service
        .create(
            &Caller::User,
            &[],
            person("Anna", "Beck", vec![value("phone", "home", "030 5550101")]),
        )
        .await
        .expect("create");
    rig.book
        .add(GOOGLE, vec![name("Anna", "Beck"), phone("0170 1234567", 2)]);
    rig.enable();
    let review = rig.review();
    assert_eq!(review.uncertain.len(), 1);
    assert_eq!(review.uncertain[0].contact_id, holzi.id);

    let decided = [PairDecision {
        pair_id: review.uncertain[0].id.clone(),
        merge: true,
    }];
    rig.run(Mode::First(&decided)).expect("first run");
    assert_eq!(rig.headers().len(), 1, "merged as the person decided");
}

#[tokio::test]
async fn an_uncertain_pair_left_alone_becomes_a_new_contact() {
    let rig = Rig::new();
    rig.vault
        .service
        .create(
            &Caller::User,
            &[],
            person("Anna", "Beck", vec![value("phone", "home", "030 5550101")]),
        )
        .await
        .expect("create");
    rig.book
        .add(GOOGLE, vec![name("Anna", "Beck"), phone("0170 1234567", 2)]);
    rig.start();
    assert_eq!(rig.headers().len(), 2, "the default is to keep them apart");
}

#[tokio::test]
async fn switching_off_keeps_everything_and_switching_on_again_adds_no_duplicates() {
    let rig = Rig::new();
    let raw = rig
        .book
        .add(GOOGLE, vec![name("Anna", "Beck"), phone("0170 1234567", 2)]);
    rig.start();
    rig.disable();
    assert_eq!(rig.headers().len(), 1, "FR-026: nothing deleted");
    assert!(rig.book.raw(raw).is_some());
    assert_eq!(rig.links().len(), 1, "the links stay");
    rig.enable();
    assert_eq!(rig.status().state, PhoneState::Review);
    rig.run(Mode::First(&[])).expect("again");
    assert_eq!(rig.headers().len(), 1);
}

#[tokio::test]
async fn many_contacts_go_in_pages() {
    let rig = Rig::new();
    for n in 0..520 {
        rig.book.add(
            LOCAL,
            vec![
                name("Person", &format!("{n:03}")),
                phone(&format!("0170 {n:07}"), 2),
            ],
        );
    }
    let summary = rig.start();
    assert_eq!(
        summary.counts.imported, 520,
        "the index pages by 500, the reads by 200"
    );
    assert_eq!(rig.headers().len(), 520);
}

#[tokio::test]
async fn a_photo_of_the_phone_comes_along() {
    let rig = Rig::new();
    let raw = rig.book.add(GOOGLE, vec![name("Anna", "Beck")]);
    let mut png = Vec::new();
    image::DynamicImage::new_rgb8(32, 32)
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .expect("a png");
    rig.book.set_photo(raw, png);
    rig.start();
    let contact = rig.named("Anna Beck");
    assert!(contact.photo.is_some());
    assert_eq!(rig.links()[0].photo_base, contact.photo);
}
