//! The exchange of spec 057 research R4 between real vaults (`Member::genesis`) over in-memory
//! streams: who is trusted how, and every reason to refuse.

use std::collections::BTreeMap;

use tokio::io::{duplex, split};

use super::*;
use crate::contacts::card::own_card;
use crate::contacts::service::CardInto;
use crate::sync::test_support::Member;
use crate::sync::wire::EXT_PEER_ALPN;

const PEERS: Purpose = Purpose {
    alpn: EXT_PEER_ALPN,
    domain: Domain::ExtPeerAuth,
};

/// One device of a vault with what the exchange reads.
struct Side {
    member: Member,
    db: VaultDb,
    seen: SeenGenerations,
}

impl Side {
    fn new(member: Member) -> Self {
        let db = member.device.vault_db();
        Self {
            member,
            db,
            seen: SeenGenerations::default(),
        }
    }

    fn local(&self) -> Local<'_> {
        Local {
            keys: &self.member.keys,
            vault: self.member.vault,
            db: &self.db,
            feature: "extensionPeers",
            seen: &self.seen,
        }
    }

    fn endpoint(&self) -> [u8; 32] {
        self.member.keys.endpoint_id
    }
}

/// `of`'s card, made now, stored on a new contact in `on`; returns the contact's id.
async fn store_card(of: &Side, on: &Side) -> String {
    of.db
        .write_blocking(|tx| own_card::refresh(tx).map_err(haex_crdt::Error::from))
        .expect("refresh the card");
    let text = of
        .db
        .read_blocking(|q| own_card::stored_text(q).map_err(haex_crdt::Error::from))
        .expect("read the card")
        .expect("a main device has a card");
    ContactsService::new(on.db.clone())
        .card_accept(&Caller::User, text, CardInto::New)
        .await
        .expect("accept the card")
        .id
}

async fn block(vault: [u8; 32], on: &Side) {
    ContactsService::new(on.db.clone())
        .set_blocked(&Caller::User, keys::hex(&vault), true)
        .await
        .expect("block");
}

/// The effective list `member` holds now.
fn effective(member: &Member) -> SignedList {
    let rows = crate::storage::query::read(member.device.db(), |r| device_list::load_all(r))
        .expect("read lists");
    let valid: BTreeMap<_, _> = device_list::valid_lists(&rows, &member.vault);
    device_list::effective(&valid).expect("a list").clone()
}

type Outcome = Result<Verified, ForeignAuthError>;

/// `dialer` dials `acceptor`; the acceptor sees the connection come from `seen_from`. Each side
/// drops its streams when done, so a refusal ends the other side at once.
async fn exchange_from(
    dialer: &Side,
    acceptor: &Side,
    purpose: Purpose,
    seen_from: [u8; 32],
) -> (Outcome, Outcome) {
    let (near, far) = duplex(256 * 1024);
    let (mut near_recv, mut near_send) = split(near);
    let (mut far_recv, mut far_send) = split(far);
    let dialing = async move {
        dial(
            &mut near_send,
            &mut near_recv,
            &dialer.local(),
            purpose,
            acceptor.endpoint(),
        )
        .await
    };
    let accepting = async move {
        accept(
            &mut far_send,
            &mut far_recv,
            &acceptor.local(),
            PEERS,
            seen_from,
        )
        .await
    };
    tokio::join!(dialing, accepting)
}

async fn exchange(dialer: &Side, acceptor: &Side) -> (Outcome, Outcome) {
    exchange_from(dialer, acceptor, PEERS, dialer.endpoint()).await
}

/// Dials `acceptor` by hand as the device of `keys`, presenting `list` of `vault`, with `sig` as
/// the proof instead of a real one when given. Returns what the acceptor decided.
async fn rogue(
    acceptor: &Side,
    keys: &DeviceKeys,
    vault: [u8; 32],
    list: &SignedList,
    sig: Option<[u8; 64]>,
) -> Outcome {
    let (near, far) = duplex(256 * 1024);
    let (mut near_recv, mut near_send) = split(near);
    let (mut far_recv, mut far_send) = split(far);
    let endpoint = keys.endpoint_id;
    let accepting = async move {
        accept(
            &mut far_send,
            &mut far_recv,
            &acceptor.local(),
            PEERS,
            endpoint,
        )
        .await
    };
    let dialing = async move {
        let hello = Hello {
            v: VERSION,
            vault,
            device_pubkey: keys.device_pubkey,
            list_payload: list.payload.clone(),
            list_sig: Signature(list.signature),
            nonce: [9; 32],
        };
        write_value(&mut near_send, &hello, HANDSHAKE_FRAME_LIMIT)
            .await
            .expect("hello");
        let theirs: Hello = expect_value(&mut near_recv, HANDSHAKE_FRAME_LIMIT)
            .await
            .expect("their hello");
        let nonces = Nonces {
            dial: hello.nonce,
            accept: theirs.nonce,
        };
        let sig = sig.unwrap_or_else(|| {
            let signed = transcript(
                &nonces,
                &keys.endpoint_id,
                &acceptor.endpoint(),
                &vault,
                EXT_PEER_ALPN,
            );
            signing::sign(Domain::ExtPeerAuth, &signed, &keys.device_secret).expect("sign")
        });
        let proof = Proof {
            v: VERSION,
            sig: Signature(sig),
        };
        write_value(&mut near_send, &proof, HANDSHAKE_FRAME_LIMIT)
            .await
            .expect("proof");
        // Stays until the acceptor answered or gave up, as a real dialer would.
        let _ = expect_value::<_, Proof>(&mut near_recv, HANDSHAKE_FRAME_LIMIT).await;
    };
    tokio::join!(accepting, dialing).0
}

fn refused(outcome: &Outcome, why: &str) {
    assert!(
        matches!(outcome, Err(ForeignAuthError::Refused(reason)) if reason.contains(why)),
        "expected a refusal for {why}, got {outcome:?}"
    );
}

fn trust(outcome: Outcome) -> Trust {
    outcome.expect("verified").trust
}

/// A main device and a linked one of a second vault, the linked one on list generation 2.
fn vault_with_linked() -> (Side, Member) {
    let main = Member::genesis();
    let linked = Member::join(&main);
    main.add(&linked);
    (Side::new(main), linked)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_linked_device_of_the_own_vault_is_own_vault_on_both_sides() {
    let main = Member::genesis();
    let linked = Member::join(&main);
    main.add(&linked);
    let (main, linked) = (Side::new(main), Side::new(linked));

    let (dialed, accepted) = exchange(&linked, &main).await;
    let (dialed, accepted) = (dialed.expect("verified"), accepted.expect("verified"));
    assert_eq!(accepted.trust, Trust::OwnVault);
    assert_eq!(accepted.device_pubkey, linked.member.keys.device_pubkey);
    assert_eq!(dialed.trust, Trust::OwnVault);
    assert_eq!(dialed.device_pubkey, main.member.keys.device_pubkey);
    assert_eq!(dialed.endpoint_id, main.endpoint());
    // Both sides agree on the nonces a protocol binds further proofs to.
    assert_eq!(dialed.nonces, accepted.nonces);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_vault_with_a_stored_card_is_a_contact_and_one_without_is_not_verified() {
    let (ana, ben) = (Side::new(Member::genesis()), Side::new(Member::genesis()));
    let ben_on_ana = store_card(&ben, &ana).await;

    let (dialed, accepted) = exchange(&ben, &ana).await;
    match trust(accepted) {
        Trust::Contact { contact_id, .. } => assert_eq!(contact_id, ben_on_ana),
        other => panic!("expected a contact, got {other:?}"),
    }
    // Ben holds no card of Ana: she is only verified as a device of her vault.
    let dialed = dialed.expect("verified");
    assert_eq!(dialed.trust, Trust::Unverified);
    assert_eq!(dialed.vault, ana.member.vault);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_blocked_vault_is_refused_although_its_card_is_stored() {
    let (ana, ben) = (Side::new(Member::genesis()), Side::new(Member::genesis()));
    store_card(&ben, &ana).await;
    block(ben.member.vault, &ana).await;

    let (dialed, accepted) = exchange(&ben, &ana).await;
    refused(&accepted, "blocked");
    // The refused side learns nothing but that the exchange ended.
    assert!(
        matches!(dialed, Err(ForeignAuthError::Wire(_))),
        "{dialed:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_device_its_vault_does_not_list_is_refused() {
    let ana = Side::new(Member::genesis());
    let ben = Member::genesis();
    let unlisted = Side::new(Member::join(&ben));
    store_card(&Side::new(ben), &ana).await;

    let (_, accepted) = exchange(&unlisted, &ana).await;
    refused(&accepted, "not on its vault's device list");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_removed_device_is_refused_with_the_list_that_removes_it() {
    let ana = Side::new(Member::genesis());
    let (ben, linked) = vault_with_linked();
    store_card(&ben, &ana).await;
    let listed = effective(&ben.member);
    assert!(matches!(
        trust(rogue(&ana, &linked.keys, ben.member.vault, &listed, None).await),
        Trust::Contact { .. }
    ));

    crate::sync::removal::remove_device(
        &ben.member.device.replica,
        &ben.member.keys,
        &linked.keys.device_pubkey,
        5_000,
    )
    .expect("remove");
    let removing = effective(&ben.member);
    let outcome = rogue(&ana, &linked.keys, ben.member.vault, &removing, None).await;
    refused(&outcome, "removed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_connection_from_another_endpoint_than_the_list_names_is_refused() {
    let (ana, ben) = (Side::new(Member::genesis()), Side::new(Member::genesis()));
    let elsewhere = crate::sync::keys::DeviceKeys::generate().endpoint_id;

    let (_, accepted) = exchange_from(&ben, &ana, PEERS, elsewhere).await;
    refused(&accepted, "another endpoint");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_proof_that_does_not_verify_is_refused() {
    let ana = Side::new(Member::genesis());
    let ben = Member::genesis();
    let list = effective(&ben);
    let outcome = rogue(&ana, &ben.keys, ben.vault, &list, Some([7; 64])).await;
    refused(&outcome, "proof");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_list_older_than_the_stored_card_is_refused() {
    let ana = Side::new(Member::genesis());
    let (ben, linked) = vault_with_linked();
    let generation_2 = effective(&ben.member);
    ben.member.issue_list(|list| list);
    let generation_3 = effective(&ben.member);
    // The card is made now, from generation 3.
    store_card(&ben, &ana).await;

    let old = rogue(&ana, &linked.keys, ben.member.vault, &generation_2, None).await;
    refused(&old, "older than the stored card");
    let current = rogue(&ana, &linked.keys, ben.member.vault, &generation_3, None).await;
    assert!(matches!(trust(current), Trust::Contact { .. }));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_list_older_than_one_seen_in_this_session_is_refused() {
    let ana = Side::new(Member::genesis());
    let (ben, linked) = vault_with_linked();
    store_card(&ben, &ana).await;
    let generation_2 = effective(&ben.member);
    ben.member.issue_list(|list| list);
    let generation_3 = effective(&ben.member);

    let newer = rogue(&ana, &linked.keys, ben.member.vault, &generation_3, None).await;
    assert!(matches!(trust(newer), Trust::Contact { .. }));
    let older = rogue(&ana, &linked.keys, ben.member.vault, &generation_2, None).await;
    refused(&older, "seen before");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_proof_for_another_protocol_or_domain_does_not_verify() {
    let (ana, ben) = (Side::new(Member::genesis()), Side::new(Member::genesis()));
    for other in [
        Purpose {
            alpn: b"holzi-files/1",
            domain: Domain::ExtPeerAuth,
        },
        Purpose {
            alpn: EXT_PEER_ALPN,
            domain: Domain::DeviceAuth,
        },
    ] {
        let (_, accepted) = exchange_from(&ben, &ana, other, ben.endpoint()).await;
        refused(&accepted, "proof");
    }
}

/// The clock is paused: it jumps to the limit as soon as nothing else is left to run.
#[tokio::test(start_paused = true)]
async fn a_dialer_that_says_nothing_is_dropped_after_the_limit() {
    let ana = Side::new(Member::genesis());
    let (_silent, far) = duplex(1024);
    let (mut far_recv, mut far_send) = split(far);
    let outcome = accept(&mut far_send, &mut far_recv, &ana.local(), PEERS, [1; 32]).await;
    assert!(
        matches!(outcome, Err(ForeignAuthError::TimedOut)),
        "{outcome:?}"
    );
}

#[test]
fn a_seen_generation_only_rises() {
    let seen = SeenGenerations::default();
    assert!(seen.note([1; 32], 3));
    assert!(seen.note([1; 32], 3));
    assert!(!seen.note([1; 32], 2));
    assert!(seen.note([1; 32], 4));
    assert!(seen.note([2; 32], 1), "another vault has its own floor");
    seen.clear();
    assert!(seen.note([1; 32], 1));
}
