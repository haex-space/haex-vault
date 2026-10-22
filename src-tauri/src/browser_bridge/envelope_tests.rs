//! Tests for the envelopes (spec 049, T007): X25519 against RFC 7748, the AES-GCM round trip in the
//! client's layout, the fingerprint rule and the refusals.

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use curve25519_dalek::montgomery::MontgomeryPoint;

use super::envelope::{
    agree, client_id_of, decode_key, seal_response, seal_with, EnvelopeError, ServerKeys, IV_LEN,
    KEY_LEN,
};

fn hex32(hex: &str) -> [u8; 32] {
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("hex"))
        .collect();
    bytes.try_into().expect("32 bytes")
}

#[test]
fn x25519_matches_rfc_7748_section_5_2() {
    let scalar = hex32("a546e36bf0527c9d3b16154b82465edd62144c0ac1fc5a18506a2244ba449ac4");
    let u = hex32("e6db6867583030db3594c1a424b15f7c726624ec26b3353b10a903a6d0ab1c4c");
    let out = agree(&scalar, &u).expect("agree");
    assert_eq!(
        *out,
        hex32("c3da55379de9c6908e94ea4df28d084f32eccf03491c71f754b4075577a28552")
    );
}

#[test]
fn x25519_key_agreement_matches_rfc_7748_section_6_1() {
    let alice = hex32("77076d0a7318a57d3c16c17251b26645df4c2f87ebc0992ab177fba51db92c2a");
    let bob = hex32("5dab087e624a8a4b79e17f8b83800ee66f3bb1292618b6fd1c2f8b27ff88e0eb");
    let alice_public = MontgomeryPoint::mul_base_clamped(alice).to_bytes();
    let bob_public = MontgomeryPoint::mul_base_clamped(bob).to_bytes();
    assert_eq!(
        alice_public,
        hex32("8520f0098930a754748b7ddcb43ef75a0dbf3a0d26381af4eba4a98eaa9b4e6a")
    );
    assert_eq!(
        bob_public,
        hex32("de9edb7d7b7dc1b4d35b61c2ece435373f8343c85b78674dadfc7e146f882b4f")
    );
    let shared = hex32("4a5d9d5ba4ce2de1728e3bf480350f25e07e21c947d19e3376f09b3c1e161742");
    assert_eq!(*agree(&alice, &bob_public).expect("alice"), shared);
    assert_eq!(*agree(&bob, &alice_public).expect("bob"), shared);
}

#[test]
fn a_low_order_public_key_is_refused() {
    let secret = [7u8; KEY_LEN];
    assert_eq!(
        agree(&secret, &[0u8; KEY_LEN]).err(),
        Some(EnvelopeError::LowOrder)
    );
}

/// The client seals a request the way `background/crypto.ts` does: a fresh key, the shared secret
/// as AES key, a random 12-byte IV, the tag after the ciphertext.
fn client_seals(server_public: &[u8; KEY_LEN], plaintext: &[u8]) -> (String, String, String) {
    let sealed = seal_with([3u8; KEY_LEN], [9u8; IV_LEN], server_public, plaintext).expect("seal");
    (sealed.public_key, sealed.iv, sealed.message)
}

#[test]
fn a_request_sealed_to_the_server_key_opens() {
    let server = ServerKeys::from_secret([5u8; KEY_LEN]);
    let server_public = decode_key(&server.public_b64()).expect("public");
    let (ephemeral, iv, message) = client_seals(&server_public, br#"{"requestId":"r1"}"#);
    // The tag sits after the ciphertext: 16 bytes more than the plaintext.
    assert_eq!(
        STANDARD.decode(&message).expect("b64").len(),
        br#"{"requestId":"r1"}"#.len() + 16
    );
    let plain = server
        .open_request(&ephemeral, &iv, &message)
        .expect("open");
    assert_eq!(plain, br#"{"requestId":"r1"}"#);
}

#[test]
fn a_response_opens_with_the_client_key_and_uses_a_fresh_key_each_time() {
    let client_secret = [11u8; KEY_LEN];
    let client_public = MontgomeryPoint::mul_base_clamped(client_secret).to_bytes();
    let first = seal_response(&client_public, b"hello").expect("seal");
    let second = seal_response(&client_public, b"hello").expect("seal");
    assert_ne!(first.public_key, second.public_key);
    assert_ne!(first.iv, second.iv);
    // The client opens with its own secret and the fresh key, exactly like a server opens.
    let client = ServerKeys::from_secret(client_secret);
    let plain = client
        .open_request(&first.public_key, &first.iv, &first.message)
        .expect("open");
    assert_eq!(plain, b"hello");
}

#[test]
fn a_wrong_key_or_iv_fails_without_panicking() {
    let server = ServerKeys::from_secret([5u8; KEY_LEN]);
    let other = ServerKeys::from_secret([6u8; KEY_LEN]);
    let server_public = decode_key(&server.public_b64()).expect("public");
    let (ephemeral, iv, message) = client_seals(&server_public, b"x");
    assert_eq!(
        other.open_request(&ephemeral, &iv, &message),
        Err(EnvelopeError::Decrypt)
    );
    assert_eq!(
        server.open_request(&ephemeral, &STANDARD.encode([0u8; 8]), &message),
        Err(EnvelopeError::IvLength)
    );
    assert_eq!(
        server.open_request(&ephemeral, &iv, "not base64!"),
        Err(EnvelopeError::Base64)
    );
    assert_eq!(
        server.open_request(&STANDARD.encode([1u8; 31]), &iv, &message),
        Err(EnvelopeError::KeyLength)
    );
}

#[test]
fn the_fingerprint_is_the_first_half_of_sha256_in_lowercase_hex() {
    // SHA-256 of 32 zero bytes is 66687aadf862bd776c8fc18b8e9f8e20089714856ee233b3902a591d0d5f2925.
    assert_eq!(
        client_id_of(&[0u8; 32]).expect("id"),
        "66687aadf862bd776c8fc18b8e9f8e20"
    );
    assert_eq!(client_id_of(&[0u8; 31]), Err(EnvelopeError::KeyLength));
}

/// The envelopes WebCrypto made with the calls of `background/crypto.ts` (research R18), written by
/// `scripts/fixtures/browser-bridge-envelope.ts`; `pnpm check:browser-bridge` opens the response.
const WEBCRYPTO: &str = include_str!("../../tests/fixtures/browser_bridge/webcrypto.json");

fn fixture_key(fixture: &serde_json::Value, pointer: &str) -> [u8; KEY_LEN] {
    decode_key(
        fixture
            .pointer(pointer)
            .and_then(|v| v.as_str())
            .expect(pointer),
    )
    .expect(pointer)
}

fn fixture_text<'a>(fixture: &'a serde_json::Value, pointer: &str) -> &'a str {
    fixture
        .pointer(pointer)
        .and_then(|v| v.as_str())
        .expect(pointer)
}

#[test]
fn webcrypto_envelopes_open_and_a_response_seals_byte_for_byte() {
    let fixture: serde_json::Value = serde_json::from_str(WEBCRYPTO).expect("fixture");
    let client_secret = fixture_key(&fixture, "/client/secret");
    let client_public = fixture_key(&fixture, "/client/publicKey");
    let server = ServerKeys::from_secret(fixture_key(&fixture, "/server/secret"));
    assert_eq!(
        MontgomeryPoint::mul_base_clamped(client_secret).to_bytes(),
        client_public,
        "the public key WebCrypto derived"
    );
    assert_eq!(
        server.public_b64(),
        fixture_text(&fixture, "/server/publicKey")
    );
    assert_eq!(
        client_id_of(&client_public).expect("id"),
        fixture_text(&fixture, "/client/clientId")
    );

    let request = server
        .open_request(
            fixture_text(&fixture, "/request/publicKey"),
            fixture_text(&fixture, "/request/iv"),
            fixture_text(&fixture, "/request/message"),
        )
        .expect("the request opens");
    assert_eq!(
        request,
        fixture_text(&fixture, "/request/plaintext").as_bytes()
    );

    let iv: [u8; IV_LEN] = STANDARD
        .decode(fixture_text(&fixture, "/response/iv"))
        .expect("b64")
        .try_into()
        .expect("12 bytes");
    let sealed = seal_with(
        fixture_key(&fixture, "/response/ephemeralSecret"),
        iv,
        &client_public,
        fixture_text(&fixture, "/response/plaintext").as_bytes(),
    )
    .expect("seal");
    assert_eq!(
        sealed.public_key,
        fixture_text(&fixture, "/response/publicKey")
    );
    assert_eq!(sealed.iv, fixture_text(&fixture, "/response/iv"));
    assert_eq!(sealed.message, fixture_text(&fixture, "/response/message"));
}
