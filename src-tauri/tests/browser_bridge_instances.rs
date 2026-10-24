//! Spec 049, one approval per instance and the proof of its key (FR-047 to FR-049, SC-011, SC-012,
//! research R22): only the holder of an instance's private key gets through as that instance, an
//! approval never carries over to another key, origin or device, and protocol 2 is refused.

#[path = "common/bridge_fixture.rs"]
mod bridge_fixture;

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use serde_json::{json, Value};
use uuid::Uuid;

use bridge_fixture::*;
use holzi_lib::browser_bridge::envelope::seal_with;
use holzi_lib::browser_bridge::prompts::BridgeQuestion;
use holzi_lib::browser_bridge::runtime::Notice;

/// The approved, remembered instance every test starts from, and its last connection time.
async fn approved_instance(fx: &mut Fixture, port: u16, seed: u8) -> (TestClient, Option<i64>) {
    let client = TestClient::new(seed);
    drop(approved(fx, port, &client).await);
    let seen = fx
        .stored(&client.id)
        .await
        .expect("remembered")
        .last_seen_at;
    (client, seen)
}

/// A handshake sent raw; the next frame, which for protocol 3 is the challenge.
async fn raw_handshake(socket: &mut Socket, handshake: Value) -> Value {
    send(socket, handshake).await;
    recv_frame(socket).await.expect("a frame")
}

/// A proof of `bytes`, sealed to the bridge key named in `challenge`, by someone without the key.
fn forged_proof(challenge: &Value, bytes: &[u8]) -> Value {
    let server: [u8; 32] = STANDARD
        .decode(challenge["serverPublicKey"].as_str().expect("server key"))
        .expect("b64")
        .try_into()
        .expect("32 bytes");
    let plain = json!({ "challenge": STANDARD.encode(bytes) }).to_string();
    let sealed = seal_with([7u8; 32], [9u8; 12], &server, plain.as_bytes()).expect("seal");
    json!({ "type": "proof", "publicKey": sealed.public_key, "iv": sealed.iv, "message": sealed.message })
}

fn refusal(frame: &Value) -> &str {
    assert_eq!(frame["type"], "error", "{frame}");
    frame["code"].as_str().expect("code")
}

fn outdated_notices(fx: &Fixture) -> usize {
    fx.notices
        .lock()
        .expect("notices")
        .iter()
        .filter(|n| **n == Notice::OutdatedClient)
        .count()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn protocol_2_is_refused_with_an_update_hint_and_the_window_hears_it_once() {
    let mut fx = fixture();
    let bridge = fx.bridge().await;
    let client = TestClient::new(80);
    let passwords =
        json!({ "core": { "passwords": [{ "target": "*", "operation": "readWrite" }] } });
    for _ in 0..2 {
        let mut socket = extension(bridge.port()).await;
        let frame = raw_handshake(&mut socket, client.handshake_with(2, passwords.clone())).await;
        assert_eq!(refusal(&frame), "PROTOCOL_UPDATE_REQUIRED");
        assert!(frame["message"]
            .as_str()
            .is_some_and(|m| m.contains("update haex-pass")));
        closed(&mut socket).await;
    }
    fx.no_question().await;
    assert_eq!(outdated_notices(&fx), 1, "once per run of the bridge");
    assert!(
        bridge.core().outdated_client_seen(),
        "the settings show it while the bridge runs"
    );
    assert!(fx.stored(&client.id).await.is_none());
    bridge.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_challenge_opens_only_with_the_key_and_the_right_proof_leads_to_the_question() {
    let mut fx = fixture();
    let bridge = fx.bridge().await;
    let client = TestClient::new(81);
    let stranger = TestClient::new(82);
    let mut socket = extension(bridge.port()).await;

    let challenge = raw_handshake(&mut socket, client.handshake()).await;
    assert_eq!(challenge["type"], "challenge", "{challenge}");
    assert!(challenge["serverPublicKey"].is_string());
    assert!(
        stranger.prove(&challenge).is_none(),
        "sealed to the client's key only"
    );
    fx.no_question().await;

    send(&mut socket, client.prove(&challenge).expect("opens")).await;
    let answer = recv_frame(&mut socket).await.expect("answer");
    assert_eq!(handshake_answer(&answer), (false, true));
    let (_, question) = fx.next_question().await;
    let BridgeQuestion::Approval(question) = question else {
        panic!("an approval question");
    };
    assert_eq!(question.client.client_id, client.id);
    bridge.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_approved_instance_cannot_be_impersonated_without_its_key() {
    let mut fx = fixture();
    let bridge = fx.bridge().await;
    let (client, seen) = approved_instance(&mut fx, bridge.port(), 83).await;
    let port = bridge.port();

    // No proof before the deadline (400 ms here).
    let mut silent = extension(port).await;
    raw_handshake(&mut silent, client.handshake()).await;
    assert_eq!(
        refusal(&recv_frame(&mut silent).await.expect("refusal")),
        "PROOF_FAILED"
    );
    closed(&mut silent).await;

    // A wrong proof.
    let mut wrong = extension(port).await;
    let challenge = raw_handshake(&mut wrong, client.handshake()).await;
    send(&mut wrong, forged_proof(&challenge, &[0u8; 32])).await;
    assert_eq!(
        refusal(&recv_frame(&mut wrong).await.expect("refusal")),
        "PROOF_FAILED"
    );
    closed(&mut wrong).await;

    // The real instance's proof from its own connection, replayed on another one.
    let mut real = extension(port).await;
    let real_challenge = raw_handshake(&mut real, client.handshake()).await;
    let real_proof = client.prove(&real_challenge).expect("opens");
    let mut relay = extension(port).await;
    raw_handshake(&mut relay, client.handshake()).await;
    send(&mut relay, real_proof).await;
    assert_eq!(
        refusal(&recv_frame(&mut relay).await.expect("refusal")),
        "PROOF_FAILED"
    );
    closed(&mut relay).await;
    drop(real);

    // A request before the proof is not answered with data.
    let mut early = extension(port).await;
    raw_handshake(&mut early, client.handshake()).await;
    send(
        &mut early,
        client.request(
            &STANDARD.encode([1u8; 32]),
            "get-items",
            json!({ "requestId": "r1" }),
        ),
    )
    .await;
    assert_eq!(
        refusal(&recv_frame(&mut early).await.expect("refusal")),
        "UNAUTHORIZED"
    );

    fx.no_question().await;
    assert_eq!(
        fx.stored(&client.id)
            .await
            .expect("still remembered")
            .last_seen_at,
        seen,
        "no failed attempt counts as a connection of the instance"
    );
    bridge.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_proof_counts_once_and_a_repeated_handshake_needs_none() {
    let mut fx = fixture();
    let bridge = fx.bridge().await;
    let (client, _) = approved_instance(&mut fx, bridge.port(), 84).await;
    let mut socket = extension(bridge.port()).await;
    let challenge = raw_handshake(&mut socket, client.handshake()).await;
    let proof = client.prove(&challenge).expect("opens");
    send(&mut socket, proof.clone()).await;
    assert_eq!(
        handshake_answer(&recv_frame(&mut socket).await.expect("answer")),
        (true, false)
    );

    // The extension repeats its handshake: the current state, no new challenge.
    let again = raw_handshake(&mut socket, client.handshake()).await;
    assert_eq!(handshake_answer(&again), (true, false));

    // A second proof ends the connection.
    send(&mut socket, proof).await;
    assert_eq!(
        refusal(&recv_frame(&mut socket).await.expect("refusal")),
        "PROOF_FAILED"
    );
    closed(&mut socket).await;
    bridge.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn another_key_or_another_origin_is_another_instance_and_is_asked() {
    let mut fx = fixture();
    let bridge = fx.bridge().await;
    let (client, _) = approved_instance(&mut fx, bridge.port(), 85).await;

    // Another browser on the same device: another key, the same kind of origin.
    let other = TestClient::new(86);
    let mut socket = extension(bridge.port()).await;
    send(&mut socket, other.handshake()).await;
    assert_eq!(
        handshake_answer(&recv(&mut socket).await.expect("answer")),
        (false, true)
    );
    fx.next_question().await;

    // The same key from another origin.
    let mut moved = connect(bridge.port(), Some("moz-extension://another-profile"))
        .await
        .expect("connects");
    send(&mut moved, client.handshake()).await;
    assert_eq!(
        handshake_answer(&recv(&mut moved).await.expect("answer")),
        (false, true)
    );
    fx.next_question().await;
    bridge.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_approval_on_one_device_is_asked_again_on_another() {
    let mut fx = fixture();
    let bridge = fx.bridge().await;
    let (client, _) = approved_instance(&mut fx, bridge.port(), 87).await;
    bridge.stop().await;

    // The same vault, opened on a second device of the user.
    let other_device = Uuid::new_v4();
    fx.db
        .write(move |tx| {
            tx.execute(
                "INSERT INTO known_devices (installation_uuid, vault_device_uuid, alias, first_seen) \
                 VALUES (?1, ?2, 'laptop', 1)",
                &[
                    &Uuid::new_v4().to_string() as &dyn haex_crdt::rusqlite::ToSql,
                    &other_device.to_string(),
                ],
            )
            .map(drop)
        })
        .await
        .expect("second device");
    let first_device = std::mem::replace(&mut fx.device, other_device);

    let bridge = fx.bridge().await;
    let mut socket = extension(bridge.port()).await;
    send(&mut socket, client.handshake()).await;
    assert_eq!(
        handshake_answer(&recv(&mut socket).await.expect("answer")),
        (false, true),
        "the same key and origin are asked again on the second device"
    );
    fx.next_question().await;
    bridge.stop().await;

    fx.device = first_device;
    assert!(
        fx.stored(&client.id).await.is_some(),
        "the first device keeps its approval"
    );
}
