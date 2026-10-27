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

/// Spec 057 T013: holzi's handshakes find a vault's card, a blocked vault is told apart from an
/// unknown one, and nobody else may ask.
#[tokio::test]
async fn a_vault_is_looked_up_by_its_identity_only_for_holzis_handshakes() {
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
                Err(HolziError::ContactsForbidden)
            ),
            "{caller:?}"
        );
    }
}
