//! Extensions stay apart (spec 057 T027, FR-004, FR-005, SC-008): a channel reaches only the frames
//! of its own extension, and a message naming another extension finds nothing.

use std::time::Duration;

use super::channel::MESSAGE;
use super::test_support::{exchange_cards, install, open_test_channel, Admit, Vault, KEY, NAME};
use super::wire::{self, Message, ADMISSION_FRAME_LIMIT, VERSION};
use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_channel_delivers_only_to_the_frames_of_its_extension() {
    let (a, b) = (
        Vault::start(Admit::AtOnce).await,
        Vault::start(Admit::AtOnce).await,
    );
    let other = install(&b.db, "other-probe");
    let other_frame = b.open_frame(other);
    let own_frames: Vec<String> = b
        .host
        .frames
        .of_extension(b.extension_id)
        .iter()
        .map(|session| session.frame.clone())
        .collect();
    let (near, _far) = open_test_channel(&a, &b).await;

    near.send_reliable(b"for the probe only".to_vec())
        .expect("queue");
    near.send_fast(b"fast too").expect("send");
    b.window
        .wait_until(|w| !w.frame_events(MESSAGE).is_empty())
        .await;
    near.send_reliable(b"after".to_vec()).expect("queue");
    b.window
        .wait_until(|w| w.frame_events(MESSAGE).len() >= 2)
        .await;

    for (_, frame, _) in b.window.frame_events(MESSAGE) {
        assert!(own_frames.contains(&frame), "{frame} is not the probe's");
        assert_ne!(frame, other_frame);
    }
    a.shutdown().await;
    b.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_extension_on_the_wire_resolves_only_with_its_key_and_name() {
    let vault = Vault::start(Admit::AtOnce).await;
    let device = vault.db.device_id();
    let resolve = |public_key: &str, name: &str| {
        let wanted = ExtensionRef {
            public_key: public_key.to_owned(),
            name: name.to_owned(),
        };
        vault
            .db
            .read_blocking(move |q| wanted.resolve(q, device).map_err(Into::into))
            .expect("read")
    };
    assert_eq!(resolve(KEY, NAME), Some(vault.extension_id));
    assert_eq!(resolve(KEY, "other-probe"), None);
    assert_eq!(resolve(&"cd".repeat(32), NAME), None);
    assert_eq!(resolve(&KEY.to_ascii_uppercase(), NAME), None);
    assert_eq!(resolve("not a key", NAME), None);

    let id = vault.extension_id.to_string();
    vault
        .db
        .write_blocking(move |tx| {
            tx.execute(
                "UPDATE extensions SET enabled = 0 WHERE id = ?1",
                haex_crdt::rusqlite::params![id],
            )?;
            Ok(())
        })
        .expect("disable");
    assert_eq!(
        resolve(KEY, NAME),
        None,
        "a disabled extension is not there"
    );
    vault.shutdown().await;
}

/// No lobby is open and no extension of the host is invited: whatever extension a contact names
/// in an `Invite` or a `Join`, the connection ends and no frame hears anything. (An `Invite` for
/// an extension the host runs is asked about in holzi's prompt, which the invitation tests cover.)
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_invite_or_join_reaches_no_frame() {
    let host = Vault::start(Admit::Real(Duration::from_secs(30))).await;
    let guest = Vault::start(Admit::AtOnce).await;
    exchange_cards(&host, &guest).await;
    for extension in [
        ExtensionRef {
            public_key: KEY.to_owned(),
            name: NAME.to_owned(),
        },
        ExtensionRef {
            public_key: KEY.to_owned(),
            name: "other-probe".to_owned(),
        },
    ] {
        for message in [
            Message::Invite {
                v: VERSION,
                invite: [1; 16],
                extension: extension.clone(),
                payload: b"{}".to_vec(),
                expires_at: u64::MAX,
            },
            Message::Join {
                v: VERSION,
                lobby: [2; 16],
                proof: [3; 32],
                extension: extension.clone(),
                display_name: "Gast".to_owned(),
                payload: b"{}".to_vec(),
            },
        ] {
            if matches!(message, Message::Invite { .. }) && extension.name == NAME {
                continue;
            }
            let (connection, mut send, _recv, verified) = guest.dial(&host).await;
            verified.expect("a contact passes the exchange");
            wire::write(&mut send, &message, ADMISSION_FRAME_LIMIT)
                .await
                .expect("write");
            tokio::time::timeout(test_support::LIMIT, connection.closed())
                .await
                .expect("the host ends the connection");
        }
    }
    assert_eq!(host.window.all_frame_events(), 0);
    assert!(host.host.peers.extensions().is_empty());
    host.shutdown().await;
    guest.shutdown().await;
}
