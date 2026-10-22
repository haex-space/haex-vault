//! Tests for the WebAuthn building blocks (spec 036, T057, research R8, R9): the origin check,
//! the user handle limit, the authenticator data, the COSE keys, the attestation object, the
//! client data and the signatures.

use ciborium::Value;
use p256::ecdsa::signature::Verifier as _;
use pkcs8::DecodePublicKey as _;
use sha2::{Digest, Sha256};

use super::model_passkeys::UserPresence;
use super::passkeys::{EDDSA, ES256, RS256};
use super::webauthn::{
    attestation_object, choose_algorithm, client_data_json, cose_public_key, create_auth_data,
    decode_cose_key, decode_user_handle, generate_key, get_auth_data, origin_matches, sign,
    Ceremony, CoseKey, WebauthnError, FLAG_AT, FLAG_BE, FLAG_BS, FLAG_UP, FLAG_UV,
};

#[test]
fn the_origin_matches_its_own_domain_and_subdomains() {
    assert!(origin_matches("https://example.com", "example.com"));
    assert!(origin_matches("https://login.example.com", "example.com"));
    assert!(origin_matches("https://example.com:8443", "example.com"));
    assert!(origin_matches("https://EXAMPLE.com", "Example.COM"));
}

#[test]
fn another_domain_or_a_lookalike_does_not_match() {
    assert!(!origin_matches("https://evil.com", "example.com"));
    assert!(!origin_matches(
        "https://example.com.evil.com",
        "example.com"
    ));
    assert!(!origin_matches("https://notexample.com", "example.com"));
    assert!(!origin_matches("https://example.com", "login.example.com"));
}

#[test]
fn a_public_suffix_is_no_relying_party() {
    assert!(!origin_matches("https://example.com", "com"));
    assert!(!origin_matches("https://example.co.uk", "co.uk"));
    assert!(origin_matches("https://example.co.uk", "example.co.uk"));
}

#[test]
fn plain_http_is_only_for_localhost() {
    assert!(origin_matches("http://localhost", "localhost"));
    assert!(origin_matches("http://localhost:3000", "localhost"));
    assert!(origin_matches("https://localhost", "localhost"));
    assert!(!origin_matches("http://example.com", "example.com"));
}

#[test]
fn an_ip_address_is_no_relying_party() {
    assert!(!origin_matches("https://127.0.0.1", "127.0.0.1"));
    assert!(!origin_matches("https://[::1]", "::1"));
    assert!(!origin_matches("https://127.0.0.1", "localhost"));
}

#[test]
fn idna_hosts_compare_in_their_ascii_form() {
    assert!(origin_matches("https://bücher.example", "bücher.example"));
    assert!(origin_matches(
        "https://xn--bcher-kva.example",
        "bücher.example"
    ));
    assert!(origin_matches(
        "https://bücher.example",
        "xn--bcher-kva.example"
    ));
    assert!(!origin_matches("https://bucher.example", "bücher.example"));
}

#[test]
fn an_origin_is_scheme_host_and_port_only() {
    assert!(!origin_matches("https://example.com/login", "example.com"));
    assert!(!origin_matches("https://user@example.com", "example.com"));
    assert!(!origin_matches("https://example.com?x=1", "example.com"));
    assert!(!origin_matches("ftp://example.com", "example.com"));
    assert!(!origin_matches("example.com", "example.com"));
    assert!(!origin_matches("https://example.com", "example.com:443"));
    assert!(!origin_matches("https://example.com", ""));
}

#[test]
fn a_user_handle_holds_at_most_64_bytes() {
    let ok = base64_url(&[7u8; 64]);
    assert_eq!(decode_user_handle(&ok).expect("64 bytes").len(), 64);
    let long = base64_url(&[7u8; 65]);
    assert_eq!(
        decode_user_handle(&long),
        Err(WebauthnError::InvalidInput("userHandle"))
    );
    assert_eq!(
        decode_user_handle(""),
        Err(WebauthnError::InvalidInput("userHandle"))
    );
    assert_eq!(
        decode_user_handle("not base64url!"),
        Err(WebauthnError::InvalidInput("userHandle"))
    );
}

#[test]
fn es256_comes_before_eddsa_and_nothing_else_is_chosen() {
    assert_eq!(choose_algorithm(&[EDDSA, ES256]), Some(ES256));
    assert_eq!(choose_algorithm(&[RS256, EDDSA]), Some(EDDSA));
    assert_eq!(choose_algorithm(&[RS256]), None);
    assert_eq!(choose_algorithm(&[]), None);
}

#[test]
fn the_authenticator_data_of_a_create_carries_the_attested_key() {
    let key = generate_key(ES256).expect("key");
    let cose = cose_public_key(ES256, &key.spki).expect("cose");
    let credential_id = [9u8; 32];
    let data = create_auth_data("example.com", &credential_id, &cose, UserPresence::None);

    assert_eq!(&data[..32], Sha256::digest(b"example.com").as_slice());
    assert_eq!(data[32], 0x58, "BE, BS and AT; no UP, no UV");
    assert_eq!(data[32], FLAG_BE | FLAG_BS | FLAG_AT);
    assert_eq!(data[32] & (FLAG_UP | FLAG_UV), 0);
    assert_eq!(&data[33..37], &[0, 0, 0, 0], "the counter is always 0");
    assert_eq!(&data[37..53], &[0u8; 16], "AAGUID of zeros");
    assert_eq!(&data[53..55], &[0, 32], "credential id length");
    assert_eq!(&data[55..87], &credential_id);
    assert_eq!(&data[87..], cose.as_slice());
}

#[test]
fn the_authenticator_data_of_a_get_has_no_key_and_no_presence() {
    let data = get_auth_data("example.com", UserPresence::None);
    assert_eq!(data.len(), 37);
    assert_eq!(&data[..32], Sha256::digest(b"example.com").as_slice());
    assert_eq!(data[32], 0x18, "BE and BS; no UP, no UV");
    assert_eq!(&data[33..], &[0, 0, 0, 0]);
}

#[test]
fn a_confirmation_sets_up_and_the_vault_password_also_uv() {
    let cose = [1u8; 4];
    let id = [2u8; 32];
    for (presence, flags) in [
        (UserPresence::None, 0),
        (UserPresence::Present, FLAG_UP),
        (UserPresence::Verified, FLAG_UP | FLAG_UV),
    ] {
        let created = create_auth_data("example.com", &id, &cose, presence);
        assert_eq!(
            created[32],
            FLAG_BE | FLAG_BS | FLAG_AT | flags,
            "{presence:?}"
        );
        let got = get_auth_data("example.com", presence);
        assert_eq!(got[32], FLAG_BE | FLAG_BS | flags, "{presence:?}");
    }
}

#[test]
fn the_relying_party_hash_uses_the_ascii_form() {
    assert_eq!(
        get_auth_data("Bücher.Example", UserPresence::None),
        get_auth_data("xn--bcher-kva.example", UserPresence::None)
    );
}

#[test]
fn cose_keys_round_trip_for_ec2_and_okp() {
    let es = generate_key(ES256).expect("es256");
    let cose = cose_public_key(ES256, &es.spki).expect("ec2");
    let CoseKey::Ec2 { x, y } = decode_cose_key(&cose).expect("decode ec2") else {
        panic!("an ES256 key is EC2");
    };
    let public = p256::PublicKey::from_public_key_der(&es.spki).expect("spki");
    let point = p256::elliptic_curve::sec1::ToSec1Point::to_sec1_point(&public, false);
    assert_eq!(&point.as_bytes()[1..33], x.as_slice());
    assert_eq!(&point.as_bytes()[33..], y.as_slice());
    // The map in CTAP2 canonical order: 1, 3, -1, -2, -3.
    let Value::Map(entries) = ciborium::from_reader::<Value, _>(cose.as_slice()).expect("cbor")
    else {
        panic!("a COSE key is a map");
    };
    let keys: Vec<i128> = entries
        .iter()
        .map(|(k, _)| k.as_integer().expect("int key").into())
        .collect();
    assert_eq!(keys, [1, 3, -1, -2, -3]);

    let ed = generate_key(EDDSA).expect("eddsa");
    let cose = cose_public_key(EDDSA, &ed.spki).expect("okp");
    let CoseKey::Okp { x } = decode_cose_key(&cose).expect("decode okp") else {
        panic!("an EdDSA key is OKP");
    };
    let public = ed25519_dalek::VerifyingKey::from_public_key_der(&ed.spki).expect("spki");
    assert_eq!(public.to_bytes().as_slice(), x.as_slice());
}

#[test]
fn rs256_gets_no_cose_key_and_no_new_key() {
    assert_eq!(
        generate_key(RS256).err(),
        Some(WebauthnError::UnsupportedAlgorithm)
    );
    assert_eq!(
        cose_public_key(RS256, &[]).err(),
        Some(WebauthnError::UnsupportedAlgorithm)
    );
}

#[test]
fn the_attestation_object_is_fmt_none() {
    let auth_data = get_auth_data("example.com", UserPresence::None);
    let object = attestation_object(&auth_data);
    let Value::Map(entries) = ciborium::from_reader::<Value, _>(object.as_slice()).expect("cbor")
    else {
        panic!("a map");
    };
    let keys: Vec<&str> = entries
        .iter()
        .map(|(k, _)| k.as_text().expect("text key"))
        .collect();
    assert_eq!(keys, ["fmt", "attStmt", "authData"]);
    assert_eq!(entries[0].1, Value::Text("none".into()));
    assert_eq!(entries[1].1, Value::Map(vec![]));
    assert_eq!(entries[2].1, Value::Bytes(auth_data));
}

#[test]
fn the_client_data_has_a_fixed_field_order() {
    let json = client_data_json(Ceremony::Create, b"abc", "https://example.com");
    assert_eq!(
        json,
        r#"{"type":"webauthn.create","challenge":"YWJj","origin":"https://example.com","crossOrigin":false}"#
    );
    let json = client_data_json(Ceremony::Get, b"abc", "https://ex\"ample.com");
    assert_eq!(
        json,
        r#"{"type":"webauthn.get","challenge":"YWJj","origin":"https://ex\"ample.com","crossOrigin":false}"#
    );
}

#[test]
fn signatures_verify_with_the_derived_public_key() {
    let auth_data = get_auth_data("example.com", UserPresence::None);
    let client = client_data_json(Ceremony::Get, b"challenge", "https://example.com");
    let mut signed = auth_data.clone();
    signed.extend_from_slice(&Sha256::digest(client.as_bytes()));

    let es = generate_key(ES256).expect("es256");
    let signature = sign(ES256, &es.pkcs8, &auth_data, client.as_bytes()).expect("sign");
    let verifying = p256::ecdsa::VerifyingKey::from_public_key_der(&es.spki).expect("spki");
    let parsed = p256::ecdsa::Signature::from_der(&signature).expect("DER");
    verifying.verify(&signed, &parsed).expect("ES256 verifies");
    assert_eq!(
        es.spki,
        super::passkeys::derive_public_key(ES256, &es.pkcs8).expect("derive"),
        "the stored public key is the one derived from the private key"
    );

    let ed = generate_key(EDDSA).expect("eddsa");
    let signature = sign(EDDSA, &ed.pkcs8, &auth_data, client.as_bytes()).expect("sign");
    assert_eq!(signature.len(), 64);
    let verifying = ed25519_dalek::VerifyingKey::from_public_key_der(&ed.spki).expect("spki");
    let parsed = ed25519_dalek::Signature::from_slice(&signature).expect("64 bytes");
    verifying
        .verify_strict(&signed, &parsed)
        .expect("EdDSA verifies");
}

#[test]
fn signing_with_rs256_or_a_broken_key_fails_without_key_material() {
    assert_eq!(
        sign(RS256, b"x", b"a", b"c").err(),
        Some(WebauthnError::UnsupportedAlgorithm)
    );
    let error = sign(ES256, b"not a key", b"a", b"c").expect_err("broken key");
    assert_eq!(error, WebauthnError::UnreadableKey);
}

#[test]
fn a_key_pair_prints_no_private_key() {
    let key = generate_key(ES256).expect("key");
    let printed = format!("{key:?}");
    assert!(printed.contains("redacted"), "{printed}");
    let hex: String = key.pkcs8.iter().map(|b| format!("{b:02x}")).collect();
    assert!(!printed.contains(&hex[hex.len() - 16..]));
}

fn base64_url(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}
