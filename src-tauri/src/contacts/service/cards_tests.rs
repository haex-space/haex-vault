use super::*;
use crate::contacts::card::own_card;
use crate::contacts::service::test_support::{card_vault, TestVault};
use crate::contacts::share::model::ContactShareTarget;
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

#[tokio::test]
async fn vaults_are_blocked_and_unblocked() {
    let ben = card_vault();
    let text = bens_card(&ben).await;
    let mara = card_vault();
    let preview = mara
        .service
        .share_preview(&Caller::User, text.clone(), None)
        .await
        .expect("preview");
    let vault = ben
        .db
        .read_blocking(|q| keys::vault_pubkey(q))
        .expect("read")
        .expect("an identity");
    let vault_hex = keys::hex(&vault);
    mara.service
        .set_blocked(&Caller::User, vault_hex.clone(), true)
        .await
        .expect("block");
    mara.service
        .set_blocked(&Caller::User, vault_hex.clone(), true)
        .await
        .expect("twice changes nothing");
    let blocked = mara.service.blocked(&Caller::User).await.expect("blocked");
    assert_eq!(blocked.len(), 1);
    assert_eq!(Some(blocked[0].npub.clone()), preview.npub);
    assert!(
        mara.service
            .share_preview(&Caller::User, text, None)
            .await
            .expect("preview")
            .blocked
    );
    mara.service
        .set_blocked(&Caller::User, vault_hex, false)
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

/// Spec 057 T013: haex-vault's handshakes find a vault's card, a blocked vault is told apart from an
/// unknown one, and nobody else may ask.
#[tokio::test]
async fn a_vault_is_looked_up_by_its_identity_only_for_haexs_handshakes() {
    let ben = card_vault();
    let text = bens_card(&ben).await;
    let mara = card_vault();
    let ben_vault = crate::contacts::card::format::parse(&text)
        .expect("card")
        .card
        .vault;
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
        .share_accept(
            &Caller::User,
            text.clone(),
            ContactShareTarget::New,
            vec![],
            None,
        )
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
                Err(HaexError::ContactsForbidden)
            ),
            "{caller:?}"
        );
    }
}

/// The noted answers of `vault`, by vault (hex).
fn notices(vault: &TestVault) -> Vec<(String, crate::chat::peer::envelope::Answer)> {
    vault
        .db
        .read_blocking(|q| crate::chat::peer::gate::notices(q).map_err(haex_crdt::Error::from))
        .expect("notices")
        .into_iter()
        .map(|(vault, answer, _, _)| (vault, answer))
        .collect()
}

fn vault_of(text: &str) -> [u8; 32] {
    crate::contacts::card::format::parse(text)
        .expect("card")
        .card
        .vault
}

/// Spec 099 T033 (research R13, R14): blocking a contact blocks every vault of it with a blocked
/// answer each; unblocking one of them removes every entry of the contact with an accepted answer
/// each, and the contact keeps its values.
#[tokio::test]
async fn blocking_a_contact_blocks_all_its_vaults_and_unblocking_restores_them() {
    use crate::chat::peer::envelope::Answer;
    let (ben, clara) = (card_vault(), card_vault());
    let (ben_text, clara_text) = (bens_card(&ben).await, bens_card(&clara).await);
    let (ben_vault, clara_vault) = (
        keys::hex(&vault_of(&ben_text)),
        keys::hex(&vault_of(&clara_text)),
    );
    let mara = card_vault();
    let contact = mara
        .service
        .share_accept(
            &Caller::User,
            ben_text,
            ContactShareTarget::New,
            vec![],
            None,
        )
        .await
        .expect("accept");
    // A second vault on the same contact, as a person with two vaults.
    let (id, card) = (
        contact.id.clone(),
        crate::contacts::card::format::parse(&clara_text).expect("card"),
    );
    mara.db
        .write_blocking(move |tx| Ok(received::save(tx, &id, &card).map(|_| ())?))
        .expect("save");

    mara.service
        .block_contact(&Caller::User, contact.id.clone())
        .await
        .expect("block");
    let mut blocked: Vec<_> = mara
        .service
        .blocked(&Caller::User)
        .await
        .expect("blocked")
        .into_iter()
        .map(|entry| {
            (
                entry.vault_pubkey,
                entry.contact_id,
                entry.name,
                entry.thread_id.is_some(),
            )
        })
        .collect();
    blocked.sort();
    let mut expected = vec![
        (
            ben_vault.clone(),
            Some(contact.id.clone()),
            Some(contact.name.clone()),
            true,
        ),
        (
            clara_vault.clone(),
            Some(contact.id.clone()),
            Some(contact.name.clone()),
            true,
        ),
    ];
    expected.sort();
    assert_eq!(blocked, expected);
    let mut sent = notices(&mara);
    sent.sort_by(|a, b| a.0.cmp(&b.0));
    let mut answers = vec![
        (ben_vault.clone(), Answer::Blocked),
        (clara_vault.clone(), Answer::Blocked),
    ];
    answers.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(sent, answers);

    mara.service
        .set_blocked(&Caller::User, clara_vault.clone(), false)
        .await
        .expect("unblock");
    assert!(mara
        .service
        .blocked(&Caller::User)
        .await
        .expect("blocked")
        .is_empty());
    let mut sent = notices(&mara);
    sent.sort_by(|a, b| a.0.cmp(&b.0));
    let mut answers = vec![
        (ben_vault, Answer::Accepted),
        (clara_vault, Answer::Accepted),
    ];
    answers.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(sent, answers);
    let back = mara
        .service
        .get(&Caller::User, &[], &contact.id)
        .await
        .expect("visible again");
    assert_eq!(back.values, contact.values);
}

/// Spec 099 T033: blocking a vault a contact holds blocks that contact with every vault of it.
#[tokio::test]
async fn blocking_a_vault_of_a_contact_blocks_the_contact() {
    let (ben, clara) = (card_vault(), card_vault());
    let (ben_text, clara_text) = (bens_card(&ben).await, bens_card(&clara).await);
    let clara_vault = keys::hex(&vault_of(&clara_text));
    let mara = card_vault();
    let contact = mara
        .service
        .share_accept(
            &Caller::User,
            ben_text.clone(),
            ContactShareTarget::New,
            vec![],
            None,
        )
        .await
        .expect("accept");
    let (id, card) = (
        contact.id.clone(),
        crate::contacts::card::format::parse(&clara_text).expect("card"),
    );
    mara.db
        .write_blocking(move |tx| Ok(received::save(tx, &id, &card).map(|_| ())?))
        .expect("save");
    mara.service
        .set_blocked(&Caller::User, keys::hex(&vault_of(&ben_text)), true)
        .await
        .expect("block");
    let blocked = mara.service.blocked(&Caller::User).await.expect("blocked");
    assert_eq!(blocked.len(), 2);
    assert!(blocked
        .iter()
        .any(|entry| entry.vault_pubkey == clara_vault));
}

/// Spec 099 T033 (quickstart Q6): unblocking a declined stranger makes a contact from the card its
/// entry kept and answers "accepted"; an entry without card only goes.
#[tokio::test]
async fn unblocking_a_declined_stranger_makes_it_a_contact() {
    use crate::chat::peer::envelope::Answer;
    let ben = card_vault();
    let text = bens_card(&ben).await;
    let ben_vault = vault_of(&text);
    let ben_hex = keys::hex(&ben_vault);
    let mara = card_vault();
    let (key, card) = (
        ben_hex.clone(),
        crate::contacts::card::format::parse(&text).expect("card"),
    );
    mara.db
        .write_blocking(move |tx| Ok(received::block_with_card(tx, &key, Some(&card))?))
        .expect("decline");
    let entry = mara.service.blocked(&Caller::User).await.expect("blocked");
    assert_eq!(entry[0].contact_id, None);
    assert_eq!(
        entry[0].name.as_deref(),
        Some(
            crate::contacts::card::format::parse(&text)
                .expect("card")
                .card
                .name
                .as_str()
        )
    );

    mara.service
        .set_blocked(&Caller::User, ben_hex.clone(), false)
        .await
        .expect("unblock");
    assert!(mara
        .service
        .blocked(&Caller::User)
        .await
        .expect("blocked")
        .is_empty());
    let stored = mara
        .db
        .read_blocking(move |q| Ok(received::stored(q, &ben_vault)?))
        .expect("read");
    let (contact_id, _) = stored.expect("a contact from the kept card");
    assert!(mara
        .service
        .list(&Caller::User, &[])
        .await
        .expect("list")
        .iter()
        .any(|header| header.id == contact_id));
    assert_eq!(notices(&mara), vec![(ben_hex, Answer::Accepted)]);

    let contacts = mara
        .service
        .list(&Caller::User, &[])
        .await
        .expect("list")
        .len();
    let stranger = "ab".repeat(32);
    mara.service
        .set_blocked(&Caller::User, stranger.clone(), true)
        .await
        .expect("block");
    mara.service
        .set_blocked(&Caller::User, stranger, false)
        .await
        .expect("unblock");
    assert!(mara
        .service
        .blocked(&Caller::User)
        .await
        .expect("blocked")
        .is_empty());
    assert_eq!(
        mara.service
            .list(&Caller::User, &[])
            .await
            .expect("list")
            .len(),
        contacts,
        "no contact for an entry without card"
    );
}
