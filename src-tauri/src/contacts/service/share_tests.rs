//! Reading in a Visitenkarte text of spec 051 (`holzi:card:`) through the share path (spec 060
//! FR-020): the same previews and choices, with the old card's name and details as an envelope.

use super::*;
use crate::contacts::card::{format, own_card};
use crate::contacts::model::OriginKind;
use crate::contacts::service::test_support::{card_vault, person, value, TestVault};
use crate::contacts::share::model::{ContactShareChoice, ContactShareItemKind};
use crate::error::HolziError;
use crate::sync::keys;

/// Ben's card, renewed once more each call (a later `issued_at`).
async fn bens_card(ben: &TestVault) -> String {
    ben.db
        .write_blocking(|tx| own_card::refresh(tx).map_err(haex_crdt::Error::from))
        .expect("refresh");
    ben.db
        .read_blocking(|q| own_card::stored_text(q).map_err(haex_crdt::Error::from))
        .expect("read")
        .expect("a card")
}

/// Ben's card with a name and an e-mail address of his own, signed by his vault: what an older
/// version of holzi handed out.
async fn bens_old_card(ben: &TestVault) -> String {
    let text = bens_card(ben).await;
    let mut card = format::parse(&text).expect("a card").card;
    card.name = "Ben Krüger".to_owned();
    card.values = vec![format::CardValue {
        kind: "email".to_owned(),
        label: "work".to_owned(),
        value: "ben@example.org".to_owned(),
    }];
    let secret = ben
        .db
        .read_blocking(|q| keys::vault_secret(q))
        .expect("read")
        .expect("a main device");
    format::sign(card, &secret).expect("sign").0.text()
}

fn contacts_but_own(headers: Vec<crate::contacts::model::ContactHeader>) -> usize {
    headers.iter().filter(|header| !header.is_self).count()
}

#[tokio::test]
async fn an_old_card_becomes_a_new_contact_with_confirmed_addresses() {
    let ben = card_vault();
    let text = bens_old_card(&ben).await;
    let mara = card_vault();
    let preview = mara
        .service
        .share_preview(&Caller::User, text.clone(), None)
        .await
        .expect("preview");
    assert_eq!(preview.name, "Ben Krüger");
    assert!(preview.legacy_card);
    assert!(preview
        .npub
        .as_deref()
        .is_some_and(|npub| npub.starts_with("npub1")));
    assert_eq!(preview.target, ContactShareTarget::New);
    assert_eq!(
        contacts_but_own(mara.service.list(&Caller::User, &[]).await.expect("list")),
        0,
        "a preview stores nothing"
    );

    let contact = mara
        .service
        .accept_defaults(text, None)
        .await
        .expect("accept");
    assert_eq!(contact.name, "Ben Krüger");
    assert_eq!(contact.origin.kind, OriginKind::Card);
    assert!(contact.targets.is_empty(), "no phone target");
    assert_eq!(contact.values.len(), 1);
    let card = contact.card.expect("confirmed addresses");
    assert_eq!(card.devices.len(), 1);
    assert_eq!(Some(card.npub), preview.npub);
}

#[tokio::test]
async fn an_old_card_goes_into_an_existing_contact_without_duplicating_what_is_there() {
    let ben = card_vault();
    let text = bens_old_card(&ben).await;
    let mara = card_vault();
    let known = mara
        .service
        .create(
            &Caller::User,
            &[],
            person(
                "Ben",
                "Krüger",
                vec![value("email", "work", "ben@example.org")],
            ),
        )
        .await
        .expect("create");
    let contact = mara
        .service
        .accept_defaults(text, Some(known.id.clone()))
        .await
        .expect("accept");
    assert_eq!(contact.id, known.id);
    assert_eq!(
        contact.values.len(),
        1,
        "the same address is not added twice"
    );
    assert!(contact.card.is_some());
    assert_eq!(contact.origin.kind, OriginKind::Holzi, "the origin stays");
}

#[tokio::test]
async fn the_own_card_is_refused() {
    let ben = card_vault();
    let text = bens_card(&ben).await;
    let result = ben.service.share_preview(&Caller::User, text, None).await;
    assert!(matches!(result, Err(HolziError::CardOwn)));
}

#[tokio::test]
async fn a_changed_card_is_refused_and_nothing_is_stored() {
    let ben = card_vault();
    let text = bens_old_card(&ben).await;
    let mara = card_vault();
    let mut chars: Vec<char> = text.chars().collect();
    let index = chars.len() - 10;
    chars[index] = if chars[index] == 'A' { 'B' } else { 'A' };
    let changed: String = chars.into_iter().collect();
    let result = mara.service.accept_defaults(changed, None).await;
    assert!(matches!(result, Err(HolziError::CardInvalid { .. })));
    assert_eq!(
        contacts_but_own(mara.service.list(&Caller::User, &[]).await.expect("list")),
        0
    );
}

#[tokio::test]
async fn a_newer_old_card_replaces_only_when_chosen_and_an_older_one_is_refused() {
    let ben = card_vault();
    let older = bens_old_card(&ben).await;
    let newer = bens_card(&ben).await;
    let mara = card_vault();
    let contact = mara
        .service
        .accept_defaults(older.clone(), None)
        .await
        .expect("accept");
    let first = contact.card.expect("card").issued_at;

    // The vault is on the contact: no other target, and the default only adds.
    let preview = mara
        .service
        .share_preview(&Caller::User, newer.clone(), None)
        .await
        .expect("preview");
    assert_eq!(
        preview.target,
        ContactShareTarget::Contact {
            id: contact.id.clone()
        }
    );
    assert!(matches!(
        mara.service
            .share_preview(&Caller::User, newer.clone(), Some(ContactShareTarget::New))
            .await,
        Err(HolziError::ShareInvalid { .. })
    ));
    let holzi = preview
        .items
        .iter()
        .find(|item| item.item == ContactShareItemKind::HolziAddresses)
        .expect("addresses");
    assert!(holzi.choices.contains(&ContactShareChoice::Replace));
    let target = ContactShareTarget::Contact {
        id: contact.id.clone(),
    };
    let updated = mara
        .service
        .share_accept(
            &Caller::User,
            newer,
            target.clone(),
            vec![ContactShareDecision {
                item: holzi.id,
                choice: ContactShareChoice::Replace,
                value_id: None,
            }],
            preview.target_revision,
        )
        .await
        .expect("replace");
    assert!(updated.card.expect("card").issued_at > first);

    // The older card text, chosen to replace, is refused.
    let seen = mara
        .service
        .share_preview(&Caller::User, older.clone(), Some(target.clone()))
        .await
        .expect("preview");
    let holzi = seen
        .items
        .iter()
        .find(|item| item.item == ContactShareItemKind::HolziAddresses);
    // Nothing new in it, so there is no choice to make; forcing one is refused.
    let forced = mara
        .service
        .share_accept(
            &Caller::User,
            older,
            target,
            vec![ContactShareDecision {
                item: holzi.map_or(0, |item| item.id),
                choice: ContactShareChoice::Replace,
                value_id: None,
            }],
            seen.target_revision,
        )
        .await;
    assert!(forced.is_err());
}

#[tokio::test]
async fn only_the_user_reads_in_and_shares() {
    let mara = card_vault();
    assert!(mara
        .service
        .share_preview(&Caller::BuiltinAgent, "holzi:card:x".into(), None)
        .await
        .is_err());
    assert!(mara
        .service
        .share_accept(
            &Caller::BuiltinAgent,
            "holzi:card:x".into(),
            ContactShareTarget::New,
            vec![],
            None
        )
        .await
        .is_err());
    assert!(mara
        .service
        .share_options(
            &Caller::BuiltinAgent,
            crate::contacts::ids::SELF_CONTACT_ID.to_owned()
        )
        .await
        .is_err());
}
