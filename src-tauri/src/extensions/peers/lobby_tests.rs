//! Lobbies for contacts between three vaults (spec 057 T047, US4, FR-020–FR-027): A hosts, B is
//! a contact, D is not.

use std::time::Duration;

use super::*;
use crate::extensions::peers::channel::{CHANNEL_CLOSED, MESSAGE};
use crate::extensions::peers::join::Joined;
use crate::extensions::peers::test_support::{
    exchange_cards, install, probe, Admit, Vault, Window, KEY, LIMIT,
};
use crate::extensions::peers::wire::{join_proof, read, write};
use crate::sync::test_support::Member;

/// The host answers for real; the others only dial.
async fn host() -> Vault {
    Vault::start(Admit::Real(Duration::from_secs(5))).await
}

/// A vault that knows where `host` listens.
async fn guest_of(host: &Vault) -> Vault {
    let guest = Vault::start(Admit::AtOnce).await;
    guest.learn(host).await;
    guest
}

fn joined(window: &Window) -> Vec<Value> {
    window
        .frame_events(JOINED)
        .into_iter()
        .map(|(_, _, data)| data)
        .collect()
}

fn ends(window: &Window, channel: &Channel) -> Vec<String> {
    window
        .frame_events(CHANNEL_CLOSED)
        .into_iter()
        .filter(|(_, _, data)| data["channelId"] == channel.id.to_string())
        .map(|(_, _, data)| data["reason"].as_str().unwrap_or_default().to_owned())
        .collect()
}

async fn joins(guest: &Vault, code: &str, name: &str) -> Joined {
    guest.join(code, name).await.expect("joined")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_contact_joins_a_lobby_for_contacts_and_becomes_a_channel() {
    let a = host().await;
    let b = guest_of(&a).await;
    exchange_cards(&a, &b).await;
    let code = a.open_lobby("deathmatch", Visibility::Contacts);

    let joined_b = joins(&b, &code, "Ben").await;
    assert_eq!(joined_b.lobby_id, "deathmatch");
    assert_eq!(
        joined_b.channel.origin,
        Origin::Joined {
            lobby_id: "deathmatch".to_owned()
        }
    );
    a.window.wait_until(|w| !joined(w).is_empty()).await;
    let event = joined(&a.window).remove(0);
    let channel_id = event["channelId"]
        .as_str()
        .expect("a channel id")
        .to_owned();
    assert_eq!(event["lobbyId"], "deathmatch");
    assert_eq!(event["displayName"], "Ben");
    assert_eq!(event["verified"], true);
    assert_eq!(event["payload"], json!({ "skin": "Ben" }));
    assert!(
        event.get("contactId").is_none(),
        "no `contacts` permission, no contact: {event}"
    );

    // The connection is the channel, both ways.
    joined_b
        .channel
        .send_reliable(b"ready".to_vec())
        .expect("send");
    a.window
        .wait_until(|w| !w.frame_events(MESSAGE).is_empty())
        .await;
    let (_, _, message) = a.window.frame_events(MESSAGE).remove(0);
    assert_eq!(message["channelId"], channel_id);
    let near = a
        .host
        .peers
        .channel(a.extension_id, Uuid::parse_str(&channel_id).expect("uuid"))
        .expect("A has the channel");
    near.send_reliable(b"go".to_vec()).expect("send");
    b.window
        .wait_until(|w| !w.frame_events(MESSAGE).is_empty())
        .await;
    a.shutdown().await;
    b.shutdown().await;
}

/// `peers:joined` names the contact only when the extension's `contacts` permission covers it
/// (research R12).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_joined_contact_is_named_only_within_the_contacts_permission() {
    let a = host().await;
    let b = guest_of(&a).await;
    let (b_on_a, _) = exchange_cards(&a, &b).await;
    a.grant("contacts", "read", "granted");
    let code = a.open_lobby("coop", Visibility::Contacts);

    joins(&b, &code, "Ben").await;
    a.window.wait_until(|w| !joined(w).is_empty()).await;
    assert_eq!(joined(&a.window)[0]["contactId"], b_on_a);
    a.shutdown().await;
    b.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_own_linked_device_joins_as_verified() {
    let main = Member::genesis();
    let linked = Member::join(&main);
    main.add(&linked);
    let a = Vault::of(main, Admit::Real(Duration::from_secs(5))).await;
    let a2 = Vault::of(linked, Admit::AtOnce).await;
    a2.learn(&a).await;
    let code = a.open_lobby("coop", Visibility::Contacts);

    joins(&a2, &code, "Laptop").await;
    a.window.wait_until(|w| !joined(w).is_empty()).await;
    let event = joined(&a.window).remove(0);
    assert_eq!(event["verified"], true);
    assert!(event.get("contactId").is_none());
    a.shutdown().await;
    a2.shutdown().await;
}

/// FR-027: the removed device and every device of its vault stay out until the lobby closes.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_removed_device_and_its_vault_stay_out_until_the_lobby_closes() {
    let a = host().await;
    let main = Member::genesis();
    let linked = Member::join(&main);
    main.add(&linked);
    // The linked device presents the list that names it.
    linked.device.pull_from(&main.device);
    let b = Vault::of(main, Admit::AtOnce).await;
    let b2 = Vault::of(linked, Admit::AtOnce).await;
    exchange_cards(&a, &b).await;
    b.learn(&a).await;
    b2.learn(&a).await;
    let code = a.open_lobby("coop", Visibility::Contacts);

    let far = joins(&b, &code, "Ben").await.channel;
    a.window.wait_until(|w| !joined(w).is_empty()).await;
    let near =
        Uuid::parse_str(joined(&a.window)[0]["channelId"].as_str().expect("id")).expect("uuid");
    let near = a.host.peers.channel(a.extension_id, near).expect("channel");
    remove(&a.host.peers, a.extension_id, near.id).expect("remove");
    b.window.wait_until(|w| !ends(w, &far).is_empty()).await;
    assert_eq!(ends(&b.window, &far), vec!["closed"]);
    assert_eq!(ends(&a.window, &near), vec!["closed"]);

    assert_eq!(
        b.join(&code, "Ben").await.err(),
        Some(PeerError::NotAdmitted)
    );
    assert_eq!(
        b2.join(&code, "Ben 2").await.err(),
        Some(PeerError::NotAdmitted),
        "the vault is banned with the device"
    );

    close(&a.host.peers, a.extension_id, "coop");
    let reopened = a.open_lobby("coop", Visibility::Contacts);
    joins(&b2, &reopened, "Ben 2").await;
    a.shutdown().await;
    b.shutdown().await;
    b2.shutdown().await;
}

/// FR-022: closing a lobby kills its code at once and keeps its channels.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_closed_lobby_answers_lobby_closed_and_keeps_its_channels() {
    let a = host().await;
    let b = guest_of(&a).await;
    let c = guest_of(&a).await;
    exchange_cards(&a, &b).await;
    exchange_cards(&a, &c).await;
    let code = a.open_lobby("coop", Visibility::Contacts);
    let far = joins(&b, &code, "Ben").await.channel;

    close(&a.host.peers, a.extension_id, "coop");
    close(&a.host.peers, a.extension_id, "coop");
    assert_eq!(
        c.join(&code, "Carla").await.err(),
        Some(PeerError::LobbyClosed)
    );
    far.send_reliable(b"still here".to_vec()).expect("send");
    a.window
        .wait_until(|w| !w.frame_events(MESSAGE).is_empty())
        .await;
    assert!(ends(&b.window, &far).is_empty());
    a.shutdown().await;
    b.shutdown().await;
    c.shutdown().await;
}

/// Everything of an extension ends with it, its lobbies included.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ending_an_extension_closes_its_lobbies() {
    let a = host().await;
    let b = guest_of(&a).await;
    exchange_cards(&a, &b).await;
    let code = a.open_lobby("coop", Visibility::Public);
    assert_eq!(a.host.peers.openness(), Openness::Public);
    assert!(a.host.peers.extensions().contains(&a.extension_id));

    a.host.peers.end_all(a.extension_id);
    assert_eq!(a.host.peers.openness(), Openness::None);
    assert_eq!(
        b.join(&code, "Ben").await.err(),
        Some(PeerError::LobbyClosed)
    );
    a.shutdown().await;
    b.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn lobby_ids_are_checked_and_unique_per_extension() {
    let a = Vault::start(Admit::AtOnce).await;
    let other = install(&a.db, "other-probe");
    let other_ref = ExtensionRef {
        public_key: KEY.to_owned(),
        name: "other-probe".to_owned(),
    };
    let open_as = |extension_id, extension, lobby_id: &str| {
        open(
            &a.host.peers,
            extension_id,
            extension,
            lobby_id,
            Visibility::Contacts,
        )
    };
    for bad in ["", &"x".repeat(65), "with space", "ümlaut", "a/b"] {
        assert_eq!(
            open_as(a.extension_id, probe(), bad).err(),
            Some(PeerError::InvalidInput),
            "{bad:?}"
        );
    }
    open_as(a.extension_id, probe(), &"x".repeat(64)).expect("64 characters");
    open_as(a.extension_id, probe(), "A-z_0.9").expect("the allowed characters");
    assert_eq!(
        open_as(a.extension_id, probe(), "A-z_0.9").err(),
        Some(PeerError::InvalidInput),
        "a second open lobby with the same id"
    );
    open_as(other, other_ref, "A-z_0.9").expect("another extension's own id");
    open_as(a.extension_id, probe(), "third").expect("third");
    open_as(a.extension_id, probe(), "fourth").expect("fourth");
    assert_eq!(
        open_as(a.extension_id, probe(), "fifth").err(),
        Some(PeerError::Limit {
            limit: LOBBIES_PER_EXTENSION
        })
    );
    close(&a.host.peers, a.extension_id, "third");
    open_as(a.extension_id, probe(), "fifth").expect("room again");
    a.shutdown().await;
}

/// The code names the host's endpoint and nothing of its vault (FR-021); without a session there
/// is no code.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_code_names_the_host_endpoint_and_needs_a_session() {
    let a = Vault::start(Admit::AtOnce).await;
    let code = JoinCode::parse(&a.open_lobby("coop", Visibility::Contacts)).expect("a code");
    assert_eq!(code.endpoint_id, *a.node.addr().id.as_bytes());
    assert_eq!(code.relay, None, "no relays in the tests");
    assert_ne!(code.endpoint_id, a.vault());

    a.host.peers.clear();
    assert_eq!(
        open(
            &a.host.peers,
            a.extension_id,
            probe(),
            "coop",
            Visibility::Contacts
        )
        .err(),
        Some(PeerError::NetworkUnavailable)
    );
    a.shutdown().await;
}

/// `extension_peers_remove` only takes channels of an own lobby.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn only_a_channel_of_an_own_lobby_can_be_removed() {
    let a = host().await;
    let b = guest_of(&a).await;
    exchange_cards(&a, &b).await;
    let code = a.open_lobby("coop", Visibility::Contacts);
    let far = joins(&b, &code, "Ben").await.channel;

    assert_eq!(
        remove(&b.host.peers, b.extension_id, far.id),
        Err(PeerError::ChannelUnknown),
        "the joining side hosts no lobby"
    );
    let other = install(&b.db, "other-probe");
    assert_eq!(
        remove(&b.host.peers, other, far.id),
        Err(PeerError::ChannelUnknown)
    );
    assert!(ends(&b.window, &far).is_empty());
    a.shutdown().await;
    b.shutdown().await;
}

/// A join with a valid proof whose display name or payload breaks the limits is refused like
/// a wrong code: the joining side checks them first, so only a changed client gets here.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_join_beyond_the_limits_is_closed_by_the_host() {
    let a = host().await;
    let b = guest_of(&a).await;
    exchange_cards(&a, &b).await;
    let code = JoinCode::parse(&a.open_lobby("coop", Visibility::Contacts)).expect("code");
    let cases = [
        (String::new(), b"{}".to_vec()),
        ("x".repeat(65), b"{}".to_vec()),
        ("Ben".to_owned(), b"not json".to_vec()),
        (
            "Ben".to_owned(),
            format!("\"{}\"", "x".repeat(PAYLOAD_BYTES - 1)).into_bytes(),
        ),
    ];
    for (display_name, payload) in cases {
        let (connection, mut send, mut recv, verified) = b.dial(&a).await;
        let verified = verified.expect("a contact");
        let join = Message::Join {
            v: VERSION,
            lobby: code.lobby,
            proof: join_proof(&code.secret, &verified.nonces, &code.lobby),
            extension: probe(),
            display_name,
            payload,
        };
        write(&mut send, &join, ADMISSION_FRAME_LIMIT)
            .await
            .expect("write");
        let reply = tokio::time::timeout(LIMIT, read(&mut recv, ADMISSION_FRAME_LIMIT))
            .await
            .expect("an answer in time")
            .expect("read");
        assert_eq!(
            reply,
            Some(Message::JoinReply {
                v: VERSION,
                status: JoinStatus::Closed
            })
        );
        connection.close(0u32.into(), b"");
    }
    assert!(joined(&a.window).is_empty());
    a.shutdown().await;
    b.shutdown().await;
}
