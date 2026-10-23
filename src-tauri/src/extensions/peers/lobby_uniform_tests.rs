//! One answer for every join that cannot enter (spec 057 T048, FR-025, SC-007): an unknown lobby,
//! a closed lobby, a wrong secret, another extension and a blocked vault all end as
//! `lobby_closed`, and the host takes as long for each.

use std::hint::black_box;
use std::time::{Duration, Instant};

use super::*;
use crate::contacts::service::{Caller, ContactsService};
use crate::extensions::peers::test_support::{exchange_cards, probe, Admit, Vault, KEY, LIMIT};
use crate::extensions::peers::wire::{join_proof, read, write};
use crate::sync::foreign_auth::Nonces;
use crate::sync::keys;

/// Runs per case of the timing comparison.
const RUNS: usize = 1000;

fn other_extension() -> ExtensionRef {
    ExtensionRef {
        public_key: KEY.to_owned(),
        name: "other-probe".to_owned(),
    }
}

/// A code of the host's endpoint for a lobby it never opened.
fn unknown_code(host: &Vault) -> JoinCode {
    JoinCode {
        v: VERSION,
        endpoint_id: *host.node.addr().id.as_bytes(),
        relay: None,
        lobby: *random_bytes::<16>(),
        secret: random_bytes::<16>(),
    }
}

/// `guest` sends a join for `code` as `extension` on its own connection; the host's answer.
async fn answer(
    guest: &Vault,
    host: &Vault,
    code: &JoinCode,
    extension: ExtensionRef,
) -> Option<Message> {
    let (connection, mut send, mut recv, verified) = guest.dial(host).await;
    let verified = verified.expect("a contact");
    let join = Message::Join {
        v: VERSION,
        lobby: code.lobby,
        proof: join_proof(&code.secret, &verified.nonces, &code.lobby),
        extension,
        display_name: "Gast".to_owned(),
        payload: b"{}".to_vec(),
    };
    write(&mut send, &join, ADMISSION_FRAME_LIMIT)
        .await
        .expect("write");
    let reply = tokio::time::timeout(LIMIT, read(&mut recv, ADMISSION_FRAME_LIMIT))
        .await
        .expect("an answer in time")
        .ok()
        .flatten();
    connection.close(0u32.into(), b"");
    reply
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn every_join_that_cannot_enter_gets_the_same_answer() {
    let host = Vault::start(Admit::Real(Duration::from_secs(5))).await;
    let open_code = JoinCode::parse(&host.open_lobby("open", Visibility::Public)).expect("a code");
    let closed_code =
        JoinCode::parse(&host.open_lobby("gone", Visibility::Contacts)).expect("a code");
    close(&host.host.peers, host.extension_id, "gone");
    let mut wrong_secret = open_code.clone();
    wrong_secret.secret = random_bytes::<16>();
    let cases = [
        ("unknown lobby", unknown_code(&host), probe()),
        ("closed lobby", closed_code, probe()),
        ("wrong secret", wrong_secret, probe()),
        ("other extension", open_code.clone(), other_extension()),
    ];

    let closed = Some(Message::JoinReply {
        v: VERSION,
        status: JoinStatus::Closed,
    });
    for (case, code, extension) in cases {
        // A guest of its own per case, so the throttle's count of failed joins stays out of it.
        let guest = Vault::start(Admit::AtOnce).await;
        guest.learn(&host).await;
        exchange_cards(&host, &guest).await;
        assert_eq!(
            answer(&guest, &host, &code, extension.clone()).await,
            closed,
            "{case}"
        );
        let joined = crate::extensions::peers::join::join(
            &guest.host.peers,
            guest.extension_id,
            extension,
            &code.text(),
            "Gast",
            &json!({}),
        )
        .await;
        assert_eq!(joined.err(), Some(PeerError::LobbyClosed), "{case}");
        guest.shutdown().await;
    }

    // A blocked vault is refused within the exchange, before any join; its extension hears the
    // same `lobby_closed`.
    let blocked = Vault::start(Admit::AtOnce).await;
    blocked.learn(&host).await;
    exchange_cards(&host, &blocked).await;
    ContactsService::new(host.db.clone())
        .set_blocked(&Caller::User, keys::hex(&blocked.vault()), true)
        .await
        .expect("block");
    assert_eq!(
        blocked.join(&open_code.text(), "Gast").await.err(),
        Some(PeerError::LobbyClosed)
    );
    assert!(host.window.frame_events(JOINED).is_empty());
    blocked.shutdown().await;
    host.shutdown().await;
}

/// SC-007: over [`RUNS`] decisions per case, the medians of the host's handling time differ by
/// less than a millisecond. Measured on the decision itself, the one place every case passes
/// through, without the network.
#[test]
fn the_host_takes_as_long_for_every_join_that_cannot_enter() {
    let (mut lobbies, extension_id) = (Lobbies::default(), Uuid::new_v4());
    let (open_key, open_secret) = lobbies
        .open(extension_id, probe(), "open", Visibility::Contacts)
        .expect("open");
    let (closed_key, closed_secret) = lobbies
        .open(extension_id, probe(), "gone", Visibility::Contacts)
        .expect("open");
    lobbies.close(extension_id, "gone");
    let nonces = Nonces {
        dial: [1; 32],
        accept: [2; 32],
    };
    let remote = Verified {
        device_pubkey: [3; 32],
        vault: [4; 32],
        endpoint_id: [5; 32],
        trust: Trust::Contact {
            contact_id: "c".to_owned(),
            card_name: "Ben".to_owned(),
        },
        nonces,
    };
    let request = |lobby: [u8; 16], secret: &[u8; 16], extension: ExtensionRef| JoinRequest {
        lobby,
        proof: join_proof(secret, &nonces, &lobby),
        extension,
        display_name: "Gast".to_owned(),
        payload: b"{}".to_vec(),
    };
    let cases = [
        request(*random_bytes::<16>(), &[9; 16], probe()),
        request(closed_key, &closed_secret, probe()),
        request(open_key, &[9; 16], probe()),
        request(open_key, &open_secret, other_extension()),
    ];

    let mut times: Vec<Vec<Duration>> = vec![Vec::with_capacity(RUNS); cases.len()];
    for _ in 0..RUNS {
        // Interleaved, so a slow moment of the machine hits every case alike.
        for (case, join) in cases.iter().enumerate() {
            let started = Instant::now();
            let decision = black_box(lobbies.decide(black_box(join), black_box(&remote)));
            times[case].push(started.elapsed());
            assert_eq!(decision, Decision::Closed, "case {case}");
        }
    }
    let medians: Vec<Duration> = times
        .into_iter()
        .map(|mut case| {
            case.sort_unstable();
            case[case.len() / 2]
        })
        .collect();
    let (fastest, slowest) = (
        medians.iter().min().copied().unwrap_or_default(),
        medians.iter().max().copied().unwrap_or_default(),
    );
    assert!(
        slowest - fastest < Duration::from_millis(1),
        "medians {medians:?}"
    );
}
