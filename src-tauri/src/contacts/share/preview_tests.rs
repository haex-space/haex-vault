use super::*;
use crate::contacts::access::Caller;
use crate::contacts::service::test_support::{person, test_vault, value as input, TestVault};
use crate::contacts::share::model::{ContactShareChoice as Choice, ContactShareState as State};
use crate::contacts::share::test_support::{card, envelope, text, value, BEN, MARA};

async fn preview_of(
    vault: &TestVault,
    text: String,
    target: Option<ContactShareTarget>,
) -> Result<ContactSharePreview> {
    vault
        .service
        .share_preview(&Caller::User, text, target)
        .await
}

async fn ben_with(vault: &TestVault, devices: &[u8]) -> (String, String) {
    let card = card(&BEN, devices, &["wss://one.example.org"], 1_760_000_000);
    let share = text(&envelope(
        vec![value("email", "work", "ben@example.org")],
        Some(&card),
    ));
    let contact = vault
        .service
        .accept_defaults(share.clone(), None)
        .await
        .expect("accept")
        .id;
    (contact, share)
}

fn rows(vault: &TestVault) -> i64 {
    vault
        .db
        .read_blocking(|q| {
            Ok(q.query_row(
                "SELECT (SELECT COUNT(*) FROM haex_contacts) + (SELECT COUNT(*) FROM haex_contacts_values) \
                 + (SELECT COUNT(*) FROM haex_contacts_cards)",
                &[],
                |r| r.get::<_, i64>(0),
            )?
            .unwrap_or(0))
        })
        .expect("count")
}

fn item<'a>(preview: &'a ContactSharePreview, kind: &ContactShareItemKind) -> &'a ContactShareItem {
    preview
        .items
        .iter()
        .find(|item| &item.item == kind)
        .expect("the item is there")
}

#[tokio::test]
async fn a_share_of_a_new_contact_offers_every_detail_with_add_as_the_default_and_writes_nothing() {
    let vault = test_vault();
    let card = card(&BEN, &[1], &["wss://one.example.org"], 1_760_000_000);
    let share = text(&envelope(
        vec![
            value("email", "work", "ben@example.org"),
            value("phone", "mobile", "0170 1111"),
        ],
        Some(&card),
    ));
    let before = rows(&vault);
    let preview = preview_of(&vault, share, None).await.expect("preview");
    assert_eq!(rows(&vault), before, "a preview writes nothing");
    assert_eq!(preview.target, ContactShareTarget::New);
    assert!(preview.target_revision.is_none());
    assert_eq!(preview.identity, ContactShareIdentity::New);
    assert!(preview.reachable);
    let email = preview
        .items
        .iter()
        .find(|item| item.value_kind.as_deref() == Some("email"))
        .expect("email");
    assert_eq!(email.state, State::New);
    assert_eq!(email.default, Some(Choice::Add));
    assert_eq!(email.choices, [Choice::Add, Choice::Skip]);
    let holzi = item(&preview, &ContactShareItemKind::HolziAddresses);
    assert_eq!(holzi.default, Some(Choice::Add));
    assert!(preview.npub.is_some());
    // The ids run through the items in order.
    assert!(preview
        .items
        .iter()
        .enumerate()
        .all(|(index, item)| usize::try_from(item.id).unwrap() == index));
}

#[tokio::test]
async fn the_own_contact_is_never_a_target_nor_a_hint() {
    let vault = test_vault();
    let own = crate::contacts::ids::SELF_CONTACT_ID.to_owned();
    vault
        .service
        .update(
            &Caller::User,
            &[],
            &own,
            crate::contacts::model::ContactPatch {
                values: Some(crate::contacts::model::ValuesPatch {
                    upsert: vec![crate::contacts::model::ValueUpsert {
                        id: None,
                        kind: "email".to_owned(),
                        label: "work".to_owned(),
                        value: "ben@example.org".to_owned(),
                        position: None,
                    }],
                    remove: vec![],
                }),
                ..Default::default()
            },
        )
        .await
        .expect("fill the own contact");
    let share = text(&envelope(
        vec![value("email", "work", "ben@example.org")],
        None,
    ));
    let refused = preview_of(
        &vault,
        share.clone(),
        Some(ContactShareTarget::Contact { id: own }),
    )
    .await;
    assert!(matches!(refused, Err(HolziError::ShareInvalid { .. })));
    let preview = preview_of(&vault, share, None).await.expect("preview");
    assert!(preview.candidates.is_empty(), "the own contact is no hint");
}

#[tokio::test]
async fn a_vault_on_a_contact_selects_it_and_no_other_target_is_allowed() {
    let vault = test_vault();
    let (ben, share) = ben_with(&vault, &[1]).await;
    let other = vault
        .service
        .create(
            &Caller::User,
            &[],
            person(
                "Cleo",
                "Frei",
                vec![input("phone", "mobile", "030 5550101")],
            ),
        )
        .await
        .expect("create")
        .id;

    let preview = preview_of(&vault, share.clone(), None)
        .await
        .expect("preview");
    assert_eq!(
        preview.target,
        ContactShareTarget::Contact { id: ben.clone() }
    );
    assert_eq!(preview.candidates.len(), 1);
    assert_eq!(preview.candidates[0].id, ben);
    assert_eq!(preview.candidates[0].reason, ContactShareReason::Vault);
    assert_eq!(preview.identity, ContactShareIdentity::Matches);

    for target in [
        ContactShareTarget::New,
        ContactShareTarget::Contact { id: other },
    ] {
        assert!(matches!(
            preview_of(&vault, share.clone(), Some(target)).await,
            Err(HolziError::ShareInvalid { .. })
        ));
    }
}

#[tokio::test]
async fn a_phone_number_or_an_address_in_common_is_only_a_hint() {
    let vault = test_vault();
    let by_phone = vault
        .service
        .create(
            &Caller::User,
            &[],
            person(
                "Anna",
                "Beck",
                vec![input("phone", "mobile", "+49 170 1111")],
            ),
        )
        .await
        .expect("create")
        .id;
    let by_mail = vault
        .service
        .create(
            &Caller::User,
            &[],
            person(
                "Bodo",
                "Frei",
                vec![input("email", "home", "BEN@example.org")],
            ),
        )
        .await
        .expect("create")
        .id;
    vault
        .service
        .create(
            &Caller::User,
            &[],
            person(
                "Cleo",
                "Stein",
                vec![input("phone", "mobile", "030 5550101")],
            ),
        )
        .await
        .expect("create");
    let share = text(&envelope(
        vec![
            value("phone", "mobile", "49 170-1111"),
            value("email", "work", "ben@example.org"),
        ],
        None,
    ));
    let preview = preview_of(&vault, share, None).await.expect("preview");
    assert_eq!(preview.target, ContactShareTarget::New, "never preselected");
    let mut hints: Vec<_> = preview.candidates.iter().map(|c| c.id.clone()).collect();
    hints.sort();
    let mut wanted = vec![by_phone, by_mail];
    wanted.sort();
    assert_eq!(hints, wanted);
    assert!(preview
        .candidates
        .iter()
        .all(|c| c.reason == ContactShareReason::Hint));
}

#[tokio::test]
async fn an_existing_target_gets_a_revision_that_follows_its_changes() {
    let vault = test_vault();
    let anna = vault
        .service
        .create(
            &Caller::User,
            &[],
            person("Anna", "Beck", vec![input("phone", "mobile", "0170 1111")]),
        )
        .await
        .expect("create")
        .id;
    let share = text(&envelope(
        vec![value("email", "work", "anna@example.org")],
        None,
    ));
    let target = Some(ContactShareTarget::Contact { id: anna.clone() });
    let first = preview_of(&vault, share.clone(), target.clone())
        .await
        .expect("preview");
    let again = preview_of(&vault, share.clone(), target.clone())
        .await
        .expect("preview");
    let revision = first.target_revision.clone().expect("a revision");
    assert_eq!(again.target_revision.as_deref(), Some(revision.as_str()));
    assert_eq!(first.target_name.as_deref(), Some("Anna Beck"));

    vault
        .service
        .update(
            &Caller::User,
            &[],
            &anna,
            crate::contacts::model::ContactPatch {
                note: crate::patch::Patch::Set("neu".to_owned()),
                ..Default::default()
            },
        )
        .await
        .expect("change");
    let changed = preview_of(&vault, share, target).await.expect("preview");
    assert_ne!(changed.target_revision.as_deref(), Some(revision.as_str()));
}

#[tokio::test]
async fn what_the_contact_has_already_carries_no_choice_and_a_filled_field_is_a_conflict() {
    let vault = test_vault();
    let anna = vault
        .service
        .create(
            &Caller::User,
            &[],
            person(
                "Ben",
                "Beck",
                vec![input("email", "work", "ben@example.org")],
            ),
        )
        .await
        .expect("create")
        .id;
    let share = text(&envelope(
        vec![
            value("email", "work", "ben@example.org"),
            value("email", "home", "ben@home.example.org"),
        ],
        None,
    ));
    let preview = preview_of(
        &vault,
        share,
        Some(ContactShareTarget::Contact { id: anna }),
    )
    .await
    .expect("preview");
    let same = preview
        .items
        .iter()
        .find(|item| item.value == "ben@example.org")
        .expect("the same");
    assert_eq!(same.state, State::Same);
    assert!(same.choices.is_empty() && same.default.is_none());

    let new = preview
        .items
        .iter()
        .find(|item| item.value == "ben@home.example.org")
        .expect("the new one");
    assert_eq!(new.choices, [Choice::Add, Choice::Update, Choice::Skip]);
    assert_eq!(new.existing.len(), 1, "the e-mail address to replace");

    let given = item(
        &preview,
        &ContactShareItemKind::NamePart {
            field: "given".to_owned(),
        },
    );
    assert_eq!(given.state, State::Same);
    let family = item(
        &preview,
        &ContactShareItemKind::NamePart {
            field: "family".to_owned(),
        },
    );
    assert_eq!(family.state, State::Conflict, "Krüger against Beck");
}

#[tokio::test]
async fn another_vault_identity_on_the_target_is_a_mismatch() {
    let vault = test_vault();
    let mara_card = card(&MARA, &[2], &[], 1_760_000_000);
    let mara = vault
        .service
        .accept_defaults(text(&envelope(vec![], Some(&mara_card))), None)
        .await
        .expect("accept")
        .id;
    let ben_card = card(&BEN, &[1], &[], 1_760_000_000);
    let share = text(&envelope(vec![], Some(&ben_card)));
    let preview = preview_of(
        &vault,
        share,
        Some(ContactShareTarget::Contact { id: mara }),
    )
    .await
    .expect("preview");
    assert!(matches!(
        preview.identity,
        ContactShareIdentity::Mismatch { .. }
    ));
}

#[tokio::test]
async fn a_share_without_a_card_is_not_reachable_and_not_an_error() {
    let vault = test_vault();
    let share = text(&envelope(
        vec![value("email", "work", "ben@example.org")],
        None,
    ));
    let preview = preview_of(&vault, share, None).await.expect("preview");
    assert!(!preview.reachable);
    assert_eq!(preview.identity, ContactShareIdentity::None);
    assert!(preview
        .items
        .iter()
        .all(|item| item.item != ContactShareItemKind::HolziAddresses));
}

#[tokio::test]
async fn addresses_that_bring_nothing_new_are_not_asked_about_and_an_old_card_may_replace() {
    let vault = test_vault();
    let (_ben, share) = ben_with(&vault, &[1]).await;
    let preview = preview_of(&vault, share, None).await.expect("preview");
    let holzi = item(&preview, &ContactShareItemKind::HolziAddresses);
    assert_eq!(holzi.state, State::Same);
    assert!(holzi.choices.is_empty());

    let more = card(
        &BEN,
        &[1, 3],
        &["wss://one.example.org", "wss://two.example.org"],
        1_760_000_100,
    );
    let preview = preview_of(&vault, text(&envelope(vec![], Some(&more))), None)
        .await
        .expect("preview");
    let holzi = item(&preview, &ContactShareItemKind::HolziAddresses);
    assert_eq!(holzi.state, State::New);
    let found = holzi.addresses.clone().expect("addresses");
    assert_eq!((found.new_devices, found.new_relays), (1, 1));
    assert_eq!(
        holzi.choices,
        [Choice::Add, Choice::Skip],
        "no replace in a share"
    );

    let legacy = preview_of(&vault, more.text(), None)
        .await
        .expect("preview");
    assert!(legacy.legacy_card);
    let holzi = item(&legacy, &ContactShareItemKind::HolziAddresses);
    assert!(holzi.choices.contains(&Choice::Replace));
}

#[tokio::test]
async fn a_blocked_vault_is_marked_and_a_card_of_the_own_vault_is_refused() {
    let vault = crate::contacts::service::test_support::card_vault();
    let ben_card = card(&BEN, &[1], &[], 1_760_000_000);
    vault
        .service
        .set_blocked(
            &Caller::User,
            crate::sync::keys::hex(&ben_card.card.vault),
            true,
        )
        .await
        .expect("block");
    let preview = preview_of(&vault, text(&envelope(vec![], Some(&ben_card))), None)
        .await
        .expect("preview");
    assert!(preview.blocked);

    let own_card = vault
        .db
        .read_blocking(|q| {
            crate::contacts::card::own_card::build_for_share(q, &[], &[])
                .map_err(haex_crdt::Error::from)
        })
        .expect("build")
        .expect("a card");
    assert!(matches!(
        preview_of(&vault, text(&envelope(vec![], Some(&own_card))), None).await,
        Err(HolziError::CardOwn)
    ));
}
