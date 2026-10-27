use super::*;
use crate::contacts::access::Caller;
use crate::contacts::model::{ContactPatch, ValueUpsert, ValuesPatch};
use crate::contacts::service::test_support::{
    card_vault, person, test_vault, value as input, TestVault,
};
use crate::contacts::share::envelope::ShareEnvelope;
use crate::contacts::share::model::{
    ContactShareChoice as Choice, ContactShareItemKind, ContactSharePreview,
};
use crate::contacts::share::test_support::{card, envelope, text, value, BEN, MARA};
use crate::storage::query::Query;

fn decision(item: u32, choice: Choice, value_id: Option<&str>) -> ContactShareDecision {
    ContactShareDecision {
        item,
        choice,
        value_id: value_id.map(str::to_owned),
    }
}

async fn contact(vault: &TestVault, id: &str) -> Contact {
    vault
        .service
        .get(&Caller::User, &[], id)
        .await
        .expect("get")
}

async fn anna(vault: &TestVault) -> String {
    vault
        .service
        .create(
            &Caller::User,
            &[],
            person("Anna", "Beck", vec![input("phone", "mobile", "0170 1111")]),
        )
        .await
        .expect("create")
        .id
}

/// Previews into `id` and accepts with `decisions`.
async fn read_in(
    vault: &TestVault,
    share: &str,
    id: &str,
    decisions: Vec<ContactShareDecision>,
) -> Result<Contact> {
    let target = ContactShareTarget::Contact { id: id.to_owned() };
    let seen = vault
        .service
        .share_preview(&Caller::User, share.to_owned(), Some(target.clone()))
        .await?;
    vault
        .service
        .share_accept(
            &Caller::User,
            share.to_owned(),
            target,
            decisions,
            seen.target_revision,
        )
        .await
}

async fn preview_into(vault: &TestVault, share: &str, id: &str) -> ContactSharePreview {
    vault
        .service
        .share_preview(
            &Caller::User,
            share.to_owned(),
            Some(ContactShareTarget::Contact { id: id.to_owned() }),
        )
        .await
        .expect("preview")
}

fn values_of(contact: &Contact) -> Vec<(String, String)> {
    contact
        .values
        .iter()
        .map(|v| (v.kind.clone(), v.value.clone()))
        .collect()
}

fn card_rows(vault: &TestVault) -> i64 {
    vault
        .db
        .read_blocking(|q| {
            Ok(
                q.query_row("SELECT COUNT(*) FROM haex_contacts_cards", &[], |r| {
                    r.get::<_, i64>(0)
                })?
                .unwrap_or(0),
            )
        })
        .expect("count")
}

#[tokio::test]
async fn a_mobile_number_stays_when_a_share_with_only_an_email_is_added() {
    let vault = test_vault();
    let id = anna(&vault).await;
    let share = text(&envelope(
        vec![value("email", "work", "anna@example.org")],
        None,
    ));
    let after = read_in(&vault, &share, &id, vec![]).await.expect("read in");
    let values = values_of(&after);
    assert!(values.contains(&("phone".to_owned(), "0170 1111".to_owned())));
    assert!(values.contains(&("email".to_owned(), "anna@example.org".to_owned())));
    assert_eq!(after.name, "Anna Beck", "a filled name stays");
}

#[tokio::test]
async fn add_is_the_default_update_replaces_only_that_value_and_skip_leaves_it() {
    let vault = test_vault();
    let id = vault
        .service
        .create(
            &Caller::User,
            &[],
            person(
                "Anna",
                "Beck",
                vec![
                    input("email", "work", "old@example.org"),
                    input("email", "home", "keep@example.org"),
                ],
            ),
        )
        .await
        .expect("create")
        .id;
    let before = contact(&vault, &id).await;
    let old = before
        .values
        .iter()
        .find(|v| v.value == "old@example.org")
        .expect("old")
        .id
        .clone();
    let share = text(&envelope(
        vec![value("email", "work", "new@example.org")],
        None,
    ));
    let preview = preview_into(&vault, &share, &id).await;
    let item = preview
        .items
        .iter()
        .find(|item| item.value == "new@example.org")
        .expect("item")
        .id;

    let updated = read_in(
        &vault,
        &share,
        &id,
        vec![decision(item, Choice::Update, Some(&old))],
    )
    .await
    .expect("update");
    let emails: Vec<_> = values_of(&updated).into_iter().map(|(_, v)| v).collect();
    assert!(emails.contains(&"new@example.org".to_owned()));
    assert!(
        !emails.contains(&"old@example.org".to_owned()),
        "that one was replaced"
    );
    assert!(
        emails.contains(&"keep@example.org".to_owned()),
        "the other stays"
    );

    let vault2 = test_vault();
    let id2 = anna(&vault2).await;
    let preview2 = preview_into(&vault2, &share, &id2).await;
    let item2 = preview2
        .items
        .iter()
        .find(|item| item.value == "new@example.org")
        .expect("item")
        .id;
    let skipped = read_in(
        &vault2,
        &share,
        &id2,
        vec![decision(item2, Choice::Skip, None)],
    )
    .await
    .expect("skip");
    assert!(!values_of(&skipped)
        .iter()
        .any(|(_, v)| v == "new@example.org"));
}

#[tokio::test]
async fn two_updates_cannot_replace_the_same_value() {
    let vault = test_vault();
    let id = vault
        .service
        .create(
            &Caller::User,
            &[],
            person(
                "Anna",
                "Beck",
                vec![input("email", "work", "old@example.org")],
            ),
        )
        .await
        .expect("create")
        .id;
    let before = contact(&vault, &id).await;
    let old = before
        .values
        .iter()
        .find(|value| value.value == "old@example.org")
        .expect("old")
        .id
        .clone();
    let share = text(&envelope(
        vec![
            value("email", "work", "first@example.org"),
            value("email", "home", "second@example.org"),
        ],
        None,
    ));
    let preview = preview_into(&vault, &share, &id).await;
    let items: Vec<_> = preview
        .items
        .iter()
        .filter(|item| item.value.ends_with("@example.org"))
        .collect();
    assert_eq!(items.len(), 2);
    let error = read_in(
        &vault,
        &share,
        &id,
        items
            .iter()
            .map(|item| decision(item.id, Choice::Update, Some(&old)))
            .collect(),
    )
    .await
    .expect_err("duplicate updates must be rejected");
    assert!(matches!(error, HolziError::ShareInvalid { .. }));
    let after = contact(&vault, &id).await;
    assert_eq!(
        values_of(&after),
        vec![("email".to_owned(), "old@example.org".to_owned())]
    );
}

#[tokio::test]
async fn an_identical_value_is_never_duplicated_and_two_shares_lose_nothing() {
    let vault = test_vault();
    let id = anna(&vault).await;
    let first = text(&envelope(
        vec![
            value("phone", "mobile", "0170 1111"),
            value("email", "work", "a@example.org"),
        ],
        None,
    ));
    let second = text(&envelope(
        vec![value("email", "home", "b@example.org")],
        None,
    ));
    read_in(&vault, &first, &id, vec![]).await.expect("first");
    let after = read_in(&vault, &second, &id, vec![]).await.expect("second");
    let values = values_of(&after);
    assert_eq!(
        values
            .iter()
            .filter(|(k, v)| k == "phone" && v == "0170 1111")
            .count(),
        1
    );
    assert!(values.contains(&("email".to_owned(), "a@example.org".to_owned())));
    assert!(values.contains(&("email".to_owned(), "b@example.org".to_owned())));
}

#[tokio::test]
async fn empty_single_fields_fill_and_filled_ones_are_kept_unless_updated() {
    let vault = test_vault();
    let id = anna(&vault).await;
    let mut shared = envelope(vec![], None);
    shared.head.organization = Some("Beck GmbH".to_owned());
    let share = text(&shared);
    let after = read_in(&vault, &share, &id, vec![]).await.expect("read in");
    assert_eq!(
        after.head.organization.as_deref(),
        Some("Beck GmbH"),
        "the empty field fills"
    );
    assert_eq!(
        after.head.family_name.as_deref(),
        Some("Beck"),
        "Krüger does not replace Beck"
    );

    let preview = preview_into(&vault, &share, &id).await;
    let family = preview
        .items
        .iter()
        .find(|item| {
            item.item
                == ContactShareItemKind::NamePart {
                    field: "family".to_owned(),
                }
        })
        .expect("family")
        .id;
    let changed = read_in(
        &vault,
        &share,
        &id,
        vec![decision(family, Choice::Update, None)],
    )
    .await
    .expect("update");
    assert_eq!(changed.head.family_name.as_deref(), Some("Krüger"));
    assert_eq!(
        changed.head.given_name.as_deref(),
        Some("Anna"),
        "Ben does not replace Anna"
    );
}

#[tokio::test]
async fn holzi_addresses_are_added_next_to_the_old_ones_and_skip_leaves_them() {
    let vault = test_vault();
    let first = card(&BEN, &[1], &["wss://one.example.org"], 1_760_000_000);
    let id = vault
        .service
        .accept_defaults(text(&envelope(vec![], Some(&first))), None)
        .await
        .expect("accept")
        .id;
    assert_eq!(card_rows(&vault), 1);

    let second = card(&BEN, &[3], &["wss://two.example.org"], 1_760_000_100);
    let share = text(&envelope(vec![], Some(&second)));
    let preview = preview_into(&vault, &share, &id).await;
    let holzi = preview
        .items
        .iter()
        .find(|item| item.item == ContactShareItemKind::HolziAddresses)
        .expect("holzi")
        .id;
    let skipped = read_in(
        &vault,
        &share,
        &id,
        vec![decision(holzi, Choice::Skip, None)],
    )
    .await
    .expect("skip");
    assert_eq!(card_rows(&vault), 1);
    assert_eq!(skipped.card.expect("card").devices.len(), 1);

    let added = read_in(&vault, &share, &id, vec![]).await.expect("add");
    assert_eq!(card_rows(&vault), 2, "the old card stays");
    let shown = added.card.expect("card");
    assert_eq!(
        shown.devices.len(),
        2,
        "both devices are the contact's addresses"
    );
    assert_eq!(shown.nostr_relays.len(), 2);

    // The same share again brings nothing and adds nothing.
    read_in(&vault, &share, &id, vec![]).await.expect("again");
    assert_eq!(card_rows(&vault), 2);
}

#[tokio::test]
async fn an_older_card_in_a_share_only_adds_what_is_new() {
    let vault = test_vault();
    let newer = card(&BEN, &[1], &[], 1_760_000_100);
    let id = vault
        .service
        .accept_defaults(text(&envelope(vec![], Some(&newer))), None)
        .await
        .expect("accept")
        .id;
    let older = card(&BEN, &[1], &[], 1_760_000_000);
    let share = text(&envelope(vec![], Some(&older)));
    read_in(&vault, &share, &id, vec![]).await.expect("read in");
    assert_eq!(
        card_rows(&vault),
        1,
        "an older card with no new address changes nothing"
    );
}

#[tokio::test]
async fn replace_works_only_for_an_old_card_text_and_removes_only_that_vaults_older_cards() {
    let vault = test_vault();
    let first = card(&BEN, &[1], &[], 1_760_000_000);
    let id = vault
        .service
        .accept_defaults(first.text(), None)
        .await
        .expect("accept")
        .id;
    let other = card(&MARA, &[2], &[], 1_760_000_000);
    vault
        .service
        .accept_defaults(other.text(), None)
        .await
        .expect("accept");
    assert_eq!(card_rows(&vault), 2);

    // In a share there is no replace.
    let newer = card(&BEN, &[5], &[], 1_760_000_200);
    let share = text(&envelope(vec![], Some(&newer)));
    let preview = preview_into(&vault, &share, &id).await;
    let holzi = preview
        .items
        .iter()
        .find(|item| item.item == ContactShareItemKind::HolziAddresses)
        .expect("holzi")
        .id;
    assert!(!preview
        .items
        .iter()
        .find(|item| item.id == holzi)
        .expect("holzi")
        .choices
        .contains(&Choice::Replace));
    assert!(matches!(
        read_in(
            &vault,
            &share,
            &id,
            vec![decision(holzi, Choice::Replace, None)]
        )
        .await,
        Err(HolziError::ShareInvalid { .. })
    ));

    // An old card text replaces, when it is newer.
    let legacy = newer.text();
    let preview = preview_into(&vault, &legacy, &id).await;
    let holzi = preview
        .items
        .iter()
        .find(|item| item.item == ContactShareItemKind::HolziAddresses)
        .expect("holzi")
        .id;
    let after = read_in(
        &vault,
        &legacy,
        &id,
        vec![decision(holzi, Choice::Replace, None)],
    )
    .await
    .expect("replace");
    assert_eq!(card_rows(&vault), 2, "Ben's old card went, Mara's stays");
    assert_eq!(after.card.expect("card").devices.len(), 1);

    // An older old-card text is refused.
    let stale = card(&BEN, &[6], &[], 1_760_000_050).text();
    let preview = preview_into(&vault, &stale, &id).await;
    let holzi = preview
        .items
        .iter()
        .find(|item| item.item == ContactShareItemKind::HolziAddresses)
        .expect("holzi")
        .id;
    assert!(matches!(
        read_in(
            &vault,
            &stale,
            &id,
            vec![decision(holzi, Choice::Replace, None)]
        )
        .await,
        Err(HolziError::ShareInvalid { .. })
    ));
    assert_eq!(card_rows(&vault), 2);
}

#[tokio::test]
async fn an_update_pointing_at_another_contacts_value_fails_and_writes_nothing() {
    let vault = test_vault();
    let id = vault
        .service
        .create(
            &Caller::User,
            &[],
            person(
                "Anna",
                "Beck",
                vec![input("email", "work", "anna@example.org")],
            ),
        )
        .await
        .expect("create")
        .id;
    let other = vault
        .service
        .create(
            &Caller::User,
            &[],
            person(
                "Bodo",
                "Frei",
                vec![input("email", "work", "bodo@example.org")],
            ),
        )
        .await
        .expect("create");
    let share = text(&envelope(
        vec![value("email", "home", "new@example.org")],
        None,
    ));
    let preview = preview_into(&vault, &share, &id).await;
    let item = preview
        .items
        .iter()
        .find(|item| item.value == "new@example.org")
        .expect("item")
        .id;
    let before = contact(&vault, &id).await;
    let refused = read_in(
        &vault,
        &share,
        &id,
        vec![decision(item, Choice::Update, Some(&other.values[0].id))],
    )
    .await;
    assert!(matches!(refused, Err(HolziError::ShareInvalid { .. })));
    assert_eq!(contact(&vault, &id).await, before);
    assert_eq!(
        contact(&vault, &other.id).await.values[0].value,
        "bodo@example.org"
    );
}

#[tokio::test]
async fn a_choice_the_item_does_not_offer_refuses_the_share() {
    let vault = test_vault();
    let id = anna(&vault).await;
    let share = text(&envelope(vec![value("phone", "mobile", "0170 1111")], None));
    // The number is there already: nothing to choose.
    let preview = preview_into(&vault, &share, &id).await;
    let item = preview
        .items
        .iter()
        .find(|item| item.value == "0170 1111")
        .expect("item")
        .id;
    assert!(matches!(
        read_in(&vault, &share, &id, vec![decision(item, Choice::Add, None)]).await,
        Err(HolziError::ShareInvalid { .. })
    ));
    assert!(matches!(
        read_in(&vault, &share, &id, vec![decision(99, Choice::Skip, None)]).await,
        Err(HolziError::ShareInvalid { .. })
    ));
    assert!(matches!(
        read_in(
            &vault,
            &share,
            &id,
            vec![
                decision(item, Choice::Skip, None),
                decision(item, Choice::Skip, None)
            ]
        )
        .await,
        Err(HolziError::ShareInvalid { .. })
    ));
}

#[tokio::test]
async fn a_changed_target_returns_stale_and_writes_nothing() {
    let vault = test_vault();
    let id = anna(&vault).await;
    let share = text(&envelope(
        vec![value("email", "work", "anna@example.org")],
        None,
    ));
    let target = ContactShareTarget::Contact { id: id.clone() };
    let seen = vault
        .service
        .share_preview(&Caller::User, share.clone(), Some(target.clone()))
        .await
        .expect("preview");
    vault
        .service
        .update(
            &Caller::User,
            &[],
            &id,
            ContactPatch {
                values: Some(ValuesPatch {
                    upsert: vec![ValueUpsert {
                        id: None,
                        kind: "phone".to_owned(),
                        label: "home".to_owned(),
                        value: "030 5550101".to_owned(),
                        position: None,
                    }],
                    remove: vec![],
                }),
                ..Default::default()
            },
        )
        .await
        .expect("a change after the preview");
    let before = contact(&vault, &id).await;
    let stale = vault
        .service
        .share_accept(
            &Caller::User,
            share.clone(),
            target.clone(),
            vec![],
            seen.target_revision,
        )
        .await;
    assert!(matches!(stale, Err(HolziError::SharePreviewStale)));
    assert_eq!(contact(&vault, &id).await, before);

    // Without a revision an existing target is stale too; a fresh preview works.
    let none = vault
        .service
        .share_accept(&Caller::User, share.clone(), target, vec![], None)
        .await;
    assert!(matches!(none, Err(HolziError::SharePreviewStale)));
    read_in(&vault, &share, &id, vec![])
        .await
        .expect("with a fresh preview");
}

#[tokio::test]
async fn a_new_contact_takes_name_parts_details_and_the_confirmed_addresses() {
    let vault = test_vault();
    let card = card(&BEN, &[1], &["wss://one.example.org"], 1_760_000_000);
    let share = text(&envelope(
        vec![value("email", "work", "ben@example.org")],
        Some(&card),
    ));
    let made = vault
        .service
        .accept_defaults(share, None)
        .await
        .expect("accept");
    assert_eq!(made.name, "Ben Krüger");
    assert_eq!(made.origin.kind, crate::contacts::model::OriginKind::Card);
    assert_eq!(
        values_of(&made),
        [("email".to_owned(), "ben@example.org".to_owned())]
    );
    assert_eq!(made.card.expect("confirmed addresses").devices.len(), 1);

    // A skipped detail is not written, and a nameless share takes the npub as display name.
    let vault = test_vault();
    let nameless = card_without_name();
    let made = vault
        .service
        .share_accept(
            &Caller::User,
            nameless,
            ContactShareTarget::New,
            vec![decision(0, Choice::Skip, None)],
            None,
        )
        .await
        .expect("accept");
    assert!(made.values.is_empty());
    assert!(made.name.starts_with("npub1"), "{}", made.name);
}

fn card_without_name() -> String {
    let card = card(&BEN, &[1], &[], 1_760_000_000);
    let mut shared = ShareEnvelope::new(crate::contacts::card::received::npub(&card.card.vault));
    shared.values = vec![value("email", "work", "ben@example.org")];
    shared.card = Some(card.bytes);
    text(&shared)
}

#[tokio::test]
async fn a_broken_card_the_own_vault_and_a_blocked_vault_stop_the_whole_share() {
    let vault = card_vault();
    let good = card(&BEN, &[1], &[], 1_760_000_000);
    let mut broken = envelope(vec![value("email", "work", "ben@example.org")], Some(&good));
    let bytes = broken.card.as_mut().expect("a card");
    let middle = bytes.len() / 2;
    bytes[middle] ^= 1;
    let rows_before = vault
        .service
        .list(&Caller::User, &[])
        .await
        .expect("list")
        .len();
    assert!(matches!(
        vault.service.accept_defaults(text(&broken), None).await,
        Err(HolziError::CardInvalid { .. })
    ));

    let own = vault
        .db
        .read_blocking(|q| {
            crate::contacts::card::own_card::build_for_share(q, &[], &[])
                .map_err(haex_crdt::Error::from)
        })
        .expect("build")
        .expect("a card");
    assert!(matches!(
        vault
            .service
            .accept_defaults(text(&envelope(vec![], Some(&own))), None)
            .await,
        Err(HolziError::CardOwn)
    ));

    vault
        .service
        .set_blocked(
            &Caller::User,
            crate::sync::keys::hex(&good.card.vault),
            true,
        )
        .await
        .expect("block");
    assert!(matches!(
        vault
            .service
            .accept_defaults(
                text(&envelope(
                    vec![value("email", "work", "ben@example.org")],
                    Some(&good)
                )),
                None
            )
            .await,
        Err(HolziError::ShareInvalid { .. })
    ));
    assert_eq!(
        vault
            .service
            .list(&Caller::User, &[])
            .await
            .expect("list")
            .len(),
        rows_before,
        "nothing was written"
    );
}

#[tokio::test]
async fn the_own_contact_and_a_mismatching_identity_cannot_take_a_share() {
    let vault = test_vault();
    let own = crate::contacts::ids::SELF_CONTACT_ID;
    let share = text(&envelope(
        vec![value("email", "work", "ben@example.org")],
        None,
    ));
    assert!(matches!(
        vault
            .service
            .share_accept(
                &Caller::User,
                share,
                ContactShareTarget::Contact { id: own.to_owned() },
                vec![],
                None
            )
            .await,
        Err(HolziError::ShareInvalid { .. })
    ));

    let mara = vault
        .service
        .accept_defaults(
            text(&envelope(
                vec![],
                Some(&card(&MARA, &[2], &[], 1_760_000_000)),
            )),
            None,
        )
        .await
        .expect("accept")
        .id;
    let ben_share = text(&envelope(
        vec![value("email", "work", "ben@example.org")],
        Some(&card(&BEN, &[1], &[], 1_760_000_000)),
    ));
    let before = contact(&vault, &mara).await;
    assert!(matches!(
        read_in(&vault, &ben_share, &mara, vec![]).await,
        Err(HolziError::ShareInvalid { .. })
    ));
    assert_eq!(
        contact(&vault, &mara).await,
        before,
        "its vault identity stays"
    );

    // A new contact takes it instead.
    let new = vault
        .service
        .accept_defaults(ben_share, None)
        .await
        .expect("new contact");
    assert_ne!(new.id, mara);
}

#[tokio::test]
async fn a_contact_without_addresses_takes_a_share_with_a_card_and_keeps_its_data() {
    let vault = test_vault();
    let id = anna(&vault).await;
    let card = card(&BEN, &[1], &[], 1_760_000_000);
    let share = text(&envelope(vec![], Some(&card)));
    let after = read_in(&vault, &share, &id, vec![]).await.expect("read in");
    assert!(values_of(&after).contains(&("phone".to_owned(), "0170 1111".to_owned())));
    assert_eq!(after.card.expect("confirmed").devices.len(), 1);
}
