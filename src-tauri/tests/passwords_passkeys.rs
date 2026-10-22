//! The passkey service through the public `PasswordsService` (spec 036, US5, T060,
//! `contracts/passkey-service.md`): create and confirm give answers a relying party can verify,
//! with constant counters and without UP or UV; the origin, the algorithm, the user handle and
//! `excludeCredentials` are checked before anything is written or signed, and no answer, error or
//! `Debug` output carries a private key. The scope and the links: `passwords_passkeys_scope.rs`.

// These tests read raw vault state that the CRDT write path does not expose.
#![allow(clippy::disallowed_methods)]

#[path = "common/passkey_fixture.rs"]
mod passkey_fixture;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use ciborium::Value;
use p256::ecdsa::signature::Verifier as _;
use pkcs8::{DecodePrivateKey as _, DecodePublicKey as _};
use rsa::pkcs8::EncodePrivateKey as _;
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use holzi_lib::passwords::access::{Caller, GrantAction};
use holzi_lib::passwords::model_passkeys::{
    NewPasskeyEntry, PasskeyCreated, PasskeyListRequest, PasskeyTarget, UserPresence,
};
use holzi_lib::passwords::passkeys::{self, PasskeyInput};
use holzi_lib::HolziError;

use passkey_fixture::{
    b64, confirm_request, create_request, extension, fixture, unb64, Fixture, ORIGIN, RP,
};

/// An imported RS256 passkey with a real key, at `item_id`.
fn import_rs256(fx: &Fixture, item_id: &str) {
    let key = rsa_key();
    let input = PasskeyInput {
        item_id: Some(item_id.to_string()),
        credential_id: STANDARD.encode(b"rsa-credential"),
        relying_party_id: RP.to_string(),
        relying_party_name: None,
        user_name: Some("anna".to_string()),
        user_display_name: None,
        user_handle: b64(b"user-1"),
        private_key: Zeroizing::new(STANDARD.encode(key.as_slice())),
        public_key: String::new(),
        algorithm: passkeys::RS256,
        sign_count: 7,
        is_discoverable: true,
        icon: None,
        color: None,
        nickname: None,
        created_at: None,
        last_used_at: None,
    };
    fx.db
        .write(|tx| {
            passkeys::insert(tx, &input)
                .map(|_| ())
                .map_err(haex_crdt::Error::from)
        })
        .expect("import");
}

fn rsa_key() -> Zeroizing<Vec<u8>> {
    let mut rng = getrandom::rand_core::UnwrapErr(getrandom::SysRng);
    let key = rsa::RsaPrivateKey::new(&mut rng, 1024).expect("rsa");
    Zeroizing::new(key.to_pkcs8_der().expect("pkcs8").as_bytes().to_vec())
}

fn verify(algorithm: i64, spki: &[u8], auth_data: &[u8], client_data: &[u8], signature: &[u8]) {
    let mut message = auth_data.to_vec();
    message.extend_from_slice(&Sha256::digest(client_data));
    match algorithm {
        -7 => {
            let key = p256::ecdsa::VerifyingKey::from_public_key_der(spki).expect("spki");
            let sig = p256::ecdsa::Signature::from_der(signature).expect("DER");
            key.verify(&message, &sig).expect("ES256 verifies");
        }
        -8 => {
            let key = ed25519_dalek::VerifyingKey::from_public_key_der(spki).expect("spki");
            let sig = ed25519_dalek::Signature::from_slice(signature).expect("64 bytes");
            key.verify_strict(&message, &sig).expect("EdDSA verifies");
        }
        other => panic!("unexpected algorithm {other}"),
    }
}

#[tokio::test]
async fn create_then_confirm_verifies_with_counters_at_zero_and_no_presence_flags() {
    for algorithm in [-7, -8] {
        let fx = fixture();
        let item = fx.entry("Konto", &[]).await;
        let created = fx.create(&item, &[algorithm]).await;
        assert_eq!(created.algorithm, algorithm);
        assert_eq!(created.item_id, item);

        let Value::Map(object) =
            ciborium::from_reader::<Value, _>(unb64(&created.attestation_object).as_slice())
                .expect("attestation")
        else {
            panic!("a map");
        };
        assert_eq!(object[0].1, Value::Text("none".into()));
        let auth_data = object[2].1.as_bytes().expect("authData").clone();
        assert_eq!(auth_data[32], 0x58, "BE, BS, AT; no UP, no UV");
        assert_eq!(&auth_data[33..37], &[0, 0, 0, 0]);
        let client: serde_json::Value =
            serde_json::from_slice(&unb64(&created.client_data_json)).expect("client data");
        assert_eq!(client["type"], "webauthn.create");
        assert_eq!(client["origin"], ORIGIN);
        assert_eq!(client["challenge"], b64(b"create-challenge"));

        for _ in 0..2 {
            let assertion = fx
                .service
                .passkey_confirm(&Caller::User, &[], confirm_request(&[]))
                .await
                .expect("confirm");
            assert_eq!(assertion.credential_id, created.credential_id);
            assert_eq!(
                assertion.user_handle.as_deref(),
                Some(b64(b"user-1").as_str())
            );
            let auth = unb64(&assertion.authenticator_data);
            assert_eq!(auth.len(), 37);
            assert_eq!(auth[32], 0x18, "BE, BS; no UP, no UV");
            assert_eq!(&auth[33..], &[0, 0, 0, 0], "the counter stays 0");
            verify(
                algorithm,
                &unb64(&created.public_key_spki),
                &auth,
                &unb64(&assertion.client_data_json),
                &unb64(&assertion.signature),
            );
        }
        assert_eq!(
            fx.count("SELECT sign_count FROM haex_passwords_passkeys"),
            0,
            "a confirmation writes no counter"
        );
        assert!(fx.last_used()[0].is_some(), "last_used_at is recorded");
    }
}

#[tokio::test]
async fn excluded_credentials_algorithms_and_user_handles_are_checked_first() {
    let fx = fixture();
    let item = fx.entry("Konto", &[]).await;
    let created = fx.create(&item, &[-8, -7]).await;
    assert_eq!(created.algorithm, -7, "ES256 before EdDSA");

    let mut again = create_request(&item, &[-7]);
    again.exclude_credentials = vec![created.credential_id.clone()];
    let excluded = fx.service.passkey_create(&Caller::User, &[], again).await;
    assert!(matches!(
        excluded,
        Err(HolziError::PasswordsPasskeyExcluded)
    ));

    let rsa_only = fx
        .service
        .passkey_create(&Caller::User, &[], create_request(&item, &[-257]))
        .await;
    assert!(matches!(
        rsa_only,
        Err(HolziError::PasswordsPasskeyUnsupportedAlgorithm)
    ));

    let mut long = create_request(&item, &[-7]);
    long.user_handle = b64(&[1u8; 65]);
    let long = fx.service.passkey_create(&Caller::User, &[], long).await;
    assert!(matches!(long, Err(HolziError::InvalidInput { ref reason }) if reason == "userHandle"));

    let mut no_item = create_request(&item, &[-7]);
    no_item.target = PasskeyTarget::Existing(String::new());
    let no_item = fx.service.passkey_create(&Caller::User, &[], no_item).await;
    assert!(matches!(no_item, Err(HolziError::InvalidInput { ref reason }) if reason == "itemId"));

    let mut evil = create_request(&item, &[-7]);
    evil.origin = "https://example.com.evil.com".to_string();
    let evil = fx.service.passkey_create(&Caller::User, &[], evil).await;
    assert!(matches!(
        evil,
        Err(HolziError::PasswordsPasskeyOriginMismatch)
    ));
    assert_eq!(
        fx.count("SELECT COUNT(*) FROM haex_passwords_passkeys"),
        1,
        "only the first passkey was created"
    );
}

#[tokio::test]
async fn an_origin_mismatch_signs_nothing_and_rs256_is_listed_but_not_confirmed() {
    let fx = fixture();
    let item = fx.entry("Konto", &[]).await;
    import_rs256(&fx, &item);

    let listed = fx
        .service
        .passkey_list(&Caller::User, &[], PasskeyListRequest::default())
        .await
        .expect("list");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].algorithm, -257);
    assert_eq!(listed[0].credential_id, b64(b"rsa-credential"));

    let mut evil = confirm_request(&[]);
    evil.origin = "https://evil.com".to_string();
    let refused = fx.service.passkey_confirm(&Caller::User, &[], evil).await;
    assert!(matches!(
        refused,
        Err(HolziError::PasswordsPasskeyOriginMismatch)
    ));

    let rsa = fx
        .service
        .passkey_confirm(&Caller::User, &[], confirm_request(&[]))
        .await;
    assert!(matches!(
        rsa,
        Err(HolziError::PasswordsPasskeyUnsupportedAlgorithm)
    ));
    assert_eq!(fx.last_used(), [None], "no usage state was written");
    assert_eq!(
        fx.count("SELECT sign_count FROM haex_passwords_passkeys"),
        7
    );
}

#[tokio::test]
async fn no_answer_error_or_debug_output_carries_a_private_key() {
    let fx = fixture();
    let item = fx.entry("Konto", &[]).await;
    let created = fx.create(&item, &[-7]).await;
    let private: String = fx
        .db
        .with_connection(|c| {
            Ok(
                c.query_row("SELECT private_key FROM haex_passwords_passkeys", [], |r| {
                    r.get(0)
                })?,
            )
        })
        .expect("key");
    // The PKCS8 of a P-256 key ends with the public point; the secret is the scalar.
    let der = Zeroizing::new(STANDARD.decode(&private).expect("pkcs8"));
    let secret = p256::SecretKey::from_pkcs8_der(&der)
        .expect("p256")
        .to_bytes();
    let scalar_url = b64(&secret);
    let scalar_std = STANDARD.encode(secret);
    let scalar_hex: String = secret.iter().map(|b| format!("{b:02x}")).collect();

    let assertion = fx
        .service
        .passkey_confirm(&Caller::User, &[], confirm_request(&[]))
        .await
        .expect("confirm");
    let listed = fx
        .service
        .passkey_list(&Caller::User, &[], PasskeyListRequest::default())
        .await
        .expect("list");
    fx.create(&item, &[-8]).await;
    let choice = fx
        .service
        .passkey_confirm(&Caller::User, &[], confirm_request(&[]))
        .await
        .expect_err("a choice");
    let detail = fx
        .service
        .get_item(&Caller::User, item.clone())
        .await
        .expect("detail");

    let printed = [
        format!("{created:?}"),
        serde_json::to_string(&created_json(&created)).expect("json"),
        format!("{assertion:?}"),
        format!("{listed:?}"),
        serde_json::to_string(&listed).expect("json"),
        format!("{choice:?}"),
        serde_json::to_string(&choice).expect("json"),
        serde_json::to_string(&detail).expect("json"),
    ];
    for text in printed {
        assert!(!text.contains(&private), "{text}");
        assert!(!text.contains(&scalar_url), "{text}");
        assert!(!text.contains(&scalar_std), "{text}");
        assert!(!text.contains(&scalar_hex), "{text}");
    }
}

fn created_json(created: &PasskeyCreated) -> serde_json::Value {
    serde_json::to_value(created).expect("value")
}

/// The flags byte of a created passkey's authenticator data.
fn created_flags(created: &PasskeyCreated) -> u8 {
    let Value::Map(object) =
        ciborium::from_reader::<Value, _>(unb64(&created.attestation_object).as_slice())
            .expect("attestation")
    else {
        panic!("a map");
    };
    object[2].1.as_bytes().expect("authData")[32]
}

#[tokio::test]
async fn a_confirmed_ceremony_sets_up_and_the_vault_password_also_uv() {
    let fx = fixture();
    let item = fx.entry("Konto", &[]).await;
    let mut request = create_request(&item, &[-7]);
    request.presence = UserPresence::Present;
    let created = fx
        .service
        .passkey_create(&Caller::User, &[], request)
        .await
        .expect("create");
    assert_eq!(created_flags(&created), 0x59, "BE, BS, AT and UP; no UV");

    for (presence, flags) in [
        (UserPresence::None, 0x18),
        (UserPresence::Present, 0x19),
        (UserPresence::Verified, 0x1d),
    ] {
        let mut request = confirm_request(&[]);
        request.presence = presence;
        let assertion = fx
            .service
            .passkey_confirm(&Caller::User, &[], request)
            .await
            .expect("confirm");
        let auth = unb64(&assertion.authenticator_data);
        assert_eq!(auth[32], flags, "{presence:?}");
        verify(
            -7,
            &unb64(&created.public_key_spki),
            &auth,
            &unb64(&assertion.client_data_json),
            &unb64(&assertion.signature),
        );
    }
}

fn new_entry(tags: &[&str]) -> PasskeyTarget {
    PasskeyTarget::New(NewPasskeyEntry {
        title: "Example".to_owned(),
        url: Some(ORIGIN.to_owned()),
        username: Some("anna".to_owned()),
        tags: tags.iter().map(|t| (*t).to_owned()).collect(),
    })
}

#[tokio::test]
async fn a_passkey_with_a_new_entry_comes_with_it_or_not_at_all() {
    let fx = fixture();
    let (caller, grants) = extension("web", GrantAction::ReadWrite);
    let entries = || fx.count("SELECT COUNT(*) FROM haex_passwords_item_details");
    let before = entries();

    let mut request = create_request("", &[-7]);
    request.target = new_entry(&["web"]);
    let created = fx
        .service
        .passkey_create(&caller, &grants, request)
        .await
        .expect("create with a new entry");
    assert_eq!(entries(), before + 1);
    let detail = fx
        .service
        .get_item(&Caller::User, created.item_id.clone())
        .await
        .expect("the new entry");
    assert_eq!(detail.header.title.as_deref(), Some("Example"));
    assert_eq!(detail.header.username.as_deref(), Some("anna"));
    let tags: Vec<_> = detail.header.tags.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(tags, ["web"]);
    assert_eq!(detail.passkeys.len(), 1);
    assert_eq!(fx.count("SELECT COUNT(*) FROM haex_passwords_passkeys"), 1);

    // The passkey fails: the entry is not created either.
    let mut excluded = create_request("", &[-7]);
    excluded.target = new_entry(&["web"]);
    excluded.exclude_credentials = vec![created.credential_id.clone()];
    let refused = fx.service.passkey_create(&caller, &grants, excluded).await;
    assert!(matches!(refused, Err(HolziError::PasswordsPasskeyExcluded)));
    let mut unsupported = create_request("", &[-257]);
    unsupported.target = new_entry(&["web"]);
    let refused = fx
        .service
        .passkey_create(&caller, &grants, unsupported)
        .await;
    assert!(matches!(
        refused,
        Err(HolziError::PasswordsPasskeyUnsupportedAlgorithm)
    ));
    // A tag outside the grant is refused before anything is written.
    let mut outside = create_request("", &[-7]);
    outside.target = new_entry(&["work"]);
    assert!(fx
        .service
        .passkey_create(&caller, &grants, outside)
        .await
        .is_err());
    assert_eq!(entries(), before + 1);
    assert_eq!(fx.count("SELECT COUNT(*) FROM haex_passwords_passkeys"), 1);
}
