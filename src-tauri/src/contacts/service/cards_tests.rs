use super::*;
use crate::contacts::model::OriginKind;
use crate::contacts::service::test_support::{card_vault, person, value, TestVault};

/// Ben's card, renewed once more each call (a later `issued_at`).
async fn bens_card(ben: &TestVault) -> String {
    ben.db
        .write_blocking(|tx| own_card::refresh(tx).map_err(haex_crdt::Error::from))
        .expect("refresh");
    ben.service
        .own_card(&Caller::User)
        .await
        .expect("own card")
        .text
        .expect("a card")
}

async fn ben_with_values() -> (TestVault, String) {
    let ben = card_vault();
    let own = ben
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
    let email = own.values[0].id.clone();
    ben.service
        .own_card_set(&Caller::User, Some(own.id), vec![email])
        .await
        .expect("set");
    let text = bens_card(&ben).await;
    (ben, text)
}

#[tokio::test]
async fn a_card_becomes_a_new_contact_with_confirmed_addresses() {
    let (_ben, text) = ben_with_values().await;
    let mara = card_vault();
    let preview = mara
        .service
        .card_preview(&Caller::User, text.clone())
        .await
        .expect("preview");
    assert_eq!(preview.name, "Ben Krüger");
    assert_eq!(preview.devices, 1);
    assert!(preview.npub.starts_with("npub1"));
    assert!(preview.existing_contact.is_none());
    assert!(
        mara.service
            .list(&Caller::User, &[])
            .await
            .expect("list")
            .iter()
            .all(|header| header.is_self),
        "a preview stores nothing (the own contact is always there)"
    );

    let contact = mara
        .service
        .card_accept(&Caller::User, text, CardInto::New)
        .await
        .expect("accept");
    assert_eq!(contact.name, "Ben Krüger");
    assert_eq!(contact.origin.kind, OriginKind::Card);
    assert!(contact.targets.is_empty(), "no phone target");
    assert_eq!(contact.values.len(), 1);
    let card = contact.card.expect("confirmed addresses");
    assert_eq!(card.devices.len(), 1);
    assert_eq!(card.npub, preview.npub);
}

#[tokio::test]
async fn a_card_goes_into_an_existing_contact() {
    let (_ben, text) = ben_with_values().await;
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
        .card_accept(
            &Caller::User,
            text,
            CardInto::Contact {
                contact_id: known.id.clone(),
            },
        )
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
    let result = ben.service.card_preview(&Caller::User, text).await;
    assert!(matches!(result, Err(HolziError::CardOwn)));
}

#[tokio::test]
async fn a_changed_card_is_refused_and_nothing_is_stored() {
    let (_ben, text) = ben_with_values().await;
    let mara = card_vault();
    let mut chars: Vec<char> = text.chars().collect();
    let index = chars.len() - 10;
    chars[index] = if chars[index] == 'A' { 'B' } else { 'A' };
    let changed: String = chars.into_iter().collect();
    let result = mara
        .service
        .card_accept(&Caller::User, changed, CardInto::New)
        .await;
    assert!(matches!(result, Err(HolziError::CardInvalid { .. })));
    assert!(mara
        .service
        .list(&Caller::User, &[])
        .await
        .expect("list")
        .iter()
        .all(|header| header.is_self));
}

#[tokio::test]
async fn a_newer_card_replaces_the_addresses_and_an_older_one_changes_nothing() {
    let (ben, older) = ben_with_values().await;
    let newer = bens_card(&ben).await;
    let mara = card_vault();
    let contact = mara
        .service
        .card_accept(&Caller::User, older.clone(), CardInto::New)
        .await
        .expect("accept");
    let first = contact.card.expect("card").issued_at;

    let preview = mara
        .service
        .card_preview(&Caller::User, newer.clone())
        .await
        .expect("preview");
    assert_eq!(
        preview.existing_contact.as_ref().map(|c| c.id.as_str()),
        Some(contact.id.as_str()),
        "the vault's contact is offered"
    );
    let refused = mara
        .service
        .card_accept(&Caller::User, newer.clone(), CardInto::New)
        .await;
    assert!(
        matches!(refused, Err(HolziError::ContactsInvalid { .. })),
        "one vault, one contact"
    );
    let updated = mara
        .service
        .card_accept(
            &Caller::User,
            newer,
            CardInto::Contact {
                contact_id: contact.id.clone(),
            },
        )
        .await
        .expect("newer");
    assert!(updated.card.expect("card").issued_at > first);

    let old = mara
        .service
        .card_accept(
            &Caller::User,
            older,
            CardInto::Contact {
                contact_id: contact.id,
            },
        )
        .await;
    assert!(matches!(old, Err(HolziError::CardOlder)));
}

#[tokio::test]
async fn vaults_are_blocked_and_unblocked() {
    let (_ben, text) = ben_with_values().await;
    let mara = card_vault();
    let preview = mara
        .service
        .card_preview(&Caller::User, text.clone())
        .await
        .expect("preview");
    mara.service
        .set_blocked(&Caller::User, preview.vault_pubkey.clone(), true)
        .await
        .expect("block");
    mara.service
        .set_blocked(&Caller::User, preview.vault_pubkey.clone(), true)
        .await
        .expect("twice changes nothing");
    let blocked = mara.service.blocked(&Caller::User).await.expect("blocked");
    assert_eq!(blocked.len(), 1);
    assert_eq!(blocked[0].npub, preview.npub);
    assert!(
        mara.service
            .card_preview(&Caller::User, text)
            .await
            .expect("preview")
            .blocked
    );
    mara.service
        .set_blocked(&Caller::User, preview.vault_pubkey, false)
        .await
        .expect("unblock");
    assert!(mara
        .service
        .blocked(&Caller::User)
        .await
        .expect("blocked")
        .is_empty());
    assert!(mara
        .service
        .set_blocked(&Caller::User, "not hex".into(), true)
        .await
        .is_err());
}

#[tokio::test]
async fn only_the_user_reads_cards_in() {
    let mara = card_vault();
    assert!(mara
        .service
        .card_preview(&Caller::BuiltinAgent, "holzi:card:x".into())
        .await
        .is_err());
    assert!(mara.service.own_card(&Caller::BuiltinAgent).await.is_err());
}

/// Spec 057 T013: holzi's handshakes find a vault's card, a blocked vault is told apart from an
/// unknown one, and nobody else may ask.
#[tokio::test]
async fn a_vault_is_looked_up_by_its_identity_only_for_holzis_handshakes() {
    let (_ben, text) = ben_with_values().await;
    let mara = card_vault();
    let ben_vault = format::parse(&text).expect("card").card.vault;
    let peers = Caller::Internal {
        feature: "extensionPeers",
    };
    assert_eq!(
        mara.service
            .card_of_vault(&peers, ben_vault)
            .await
            .expect("lookup"),
        VaultCard::Unknown
    );

    let contact = mara
        .service
        .card_accept(&Caller::User, text.clone(), CardInto::New)
        .await
        .expect("accept");
    for feature in ["extensionPeers", "fileShares"] {
        match mara
            .service
            .card_of_vault(&Caller::Internal { feature }, ben_vault)
            .await
            .expect("lookup")
        {
            VaultCard::Stored { contact_id, card } => {
                assert_eq!(contact_id, contact.id);
                assert_eq!(card.card.vault, ben_vault);
            }
            other => panic!("expected the stored card, got {other:?}"),
        }
    }

    mara.service
        .set_blocked(&Caller::User, keys::hex(&ben_vault), true)
        .await
        .expect("block");
    assert_eq!(
        mara.service
            .card_of_vault(&peers, ben_vault)
            .await
            .expect("lookup"),
        VaultCard::Blocked
    );

    for caller in [
        Caller::User,
        Caller::BuiltinAgent,
        Caller::Extension { id: "x".into() },
        Caller::Internal { feature: "phone" },
    ] {
        assert!(
            matches!(
                mara.service.card_of_vault(&caller, ben_vault).await,
                Err(HolziError::ContactsForbidden)
            ),
            "{caller:?}"
        );
    }
}
