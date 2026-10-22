//! Spec 049, the passkey methods of the External Bridge against a real vault and real sockets (US4,
//! T064, SC-005 without a browser): a presence confirmation per ceremony, UP after confirming, UV
//! only with the vault password, and every request that would fail anyway refused without a
//! question.

#[path = "common/bridge_fixture.rs"]
mod bridge_fixture;

use std::time::Duration;

use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine as _;
use ciborium::Value as Cbor;
use p256::ecdsa::signature::Verifier as _;
use pkcs8::DecodePublicKey as _;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use bridge_fixture::*;
use holzi_lib::browser_bridge::declaration::BridgeAccess;
use holzi_lib::browser_bridge::prompts::{
    BridgeAnswer, BridgeQuestion, PresenceAnswer, PresenceCeremony, PresenceChoice,
    PresenceQuestion, WriteAnswer,
};
use holzi_lib::browser_bridge::runtime::Notice;
use holzi_lib::instances::passphrase::Passphrase;
use holzi_lib::passwords::access::Caller;
use holzi_lib::passwords::model::ItemInput;
use holzi_lib::passwords::model_passkeys::{
    PasskeyCreateRequest, PasskeyCreated, PasskeyTarget, UserPresence,
};
use holzi_lib::passwords::passkeys::{self, PasskeyInput};
use holzi_lib::storage::query::Query as _;

const ORIGIN: &str = "https://login.example.de";
const CHALLENGE: &[u8] = b"a challenge of the relying party";

async fn entry(fx: &Fixture, title: &str, url: &str, tags: &[&str]) -> String {
    fx.service()
        .create_item(
            &Caller::User,
            &[],
            ItemInput {
                title: Some(title.to_owned()),
                url: Some(url.to_owned()),
                tags: tags.iter().map(|t| (*t).to_owned()).collect(),
                ..ItemInput::default()
            },
            None,
        )
        .await
        .expect("entry")
}

/// A passkey made by the user at `item_id`, for `example.de`.
async fn passkey(fx: &Fixture, item_id: &str, user: &str) -> PasskeyCreated {
    fx.service()
        .passkey_create(
            &Caller::User,
            &[],
            PasskeyCreateRequest {
                target: PasskeyTarget::Existing(item_id.to_owned()),
                rp_id: "example.de".to_owned(),
                rp_name: "Example".to_owned(),
                origin: "https://example.de".to_owned(),
                user_handle: URL_SAFE_NO_PAD.encode(user.as_bytes()),
                user_name: user.to_owned(),
                user_display_name: None,
                challenge: URL_SAFE_NO_PAD.encode(b"c"),
                exclude_credentials: Vec::new(),
                pub_key_cred_params: vec![-7],
                discoverable: true,
                presence: UserPresence::None,
            },
        )
        .await
        .expect("passkey")
}

fn create_payload(extra: Value) -> Value {
    let mut payload = json!({
        "relyingPartyId": "example.de",
        "relyingPartyName": "Example",
        "userHandle": STANDARD.encode(b"anna-handle"),
        "userName": "anna@example.de",
        "userDisplayName": "Anna",
        "challenge": STANDARD.encode(CHALLENGE),
        "excludeCredentials": [],
        "requireResidentKey": true,
        "userVerification": "preferred",
        "origin": ORIGIN,
    });
    for (key, value) in extra.as_object().expect("object") {
        payload[key] = value.clone();
    }
    payload
}

fn get_payload(extra: Value) -> Value {
    let mut payload = json!({
        "relyingPartyId": "example.de",
        "challenge": STANDARD.encode(CHALLENGE),
        "userVerification": "preferred",
        "origin": ORIGIN,
    });
    for (key, value) in extra.as_object().expect("object") {
        payload[key] = value.clone();
    }
    payload
}

/// Sends a request and returns the presence question it raises.
async fn ask(
    fx: &mut Fixture,
    socket: &mut Socket,
    client: &TestClient,
    key: &str,
    action: &str,
    payload: Value,
) -> (String, PresenceQuestion) {
    let mut payload = payload;
    payload["requestId"] = json!(format!("r-{action}"));
    send(socket, client.request(key, action, payload)).await;
    let (id, question) = fx.next_question().await;
    let BridgeQuestion::Presence(question) = question else {
        panic!("a presence question, got {question:?}");
    };
    (id, question)
}

fn confirm(choice: &PresenceChoice, passphrase: Option<&str>) -> BridgeAnswer {
    BridgeAnswer::Presence(PresenceAnswer {
        confirm: true,
        choice: Some(choice.clone()),
        passphrase: passphrase.map(Passphrase::from),
    })
}

async fn reply(socket: &mut Socket, client: &TestClient) -> Value {
    let response = recv(socket).await.expect("a response");
    assert_eq!(response["type"], "response", "{response}");
    client.open(&response)
}

fn error_code(reply: &Value) -> &str {
    assert_eq!(reply["success"], false, "{reply}");
    reply["errorCode"].as_str().expect("errorCode")
}

fn std_bytes(value: &Value) -> Vec<u8> {
    STANDARD
        .decode(value.as_str().expect("text"))
        .expect("standard Base64")
}

/// The flags byte of the `authData` inside an attestation object.
fn attested_flags(attestation: &[u8]) -> u8 {
    let Cbor::Map(object) = ciborium::from_reader::<Cbor, _>(attestation).expect("CBOR") else {
        panic!("a map");
    };
    let fmt = object
        .iter()
        .find(|(k, _)| k.as_text() == Some("fmt"))
        .expect("fmt");
    assert_eq!(fmt.1.as_text(), Some("none"));
    let auth = object
        .iter()
        .find(|(k, _)| k.as_text() == Some("authData"))
        .expect("authData");
    auth.1.as_bytes().expect("bytes")[32]
}

async fn passkey_count(fx: &Fixture) -> i64 {
    fx.db
        .read(|q| {
            q.query_row("SELECT COUNT(*) FROM haex_passwords_passkeys", &[], |r| {
                r.get::<_, i64>(0)
            })
            .map(|n| n.unwrap_or(0))
        })
        .await
        .expect("count")
}

async fn approved_web(fx: &mut Fixture, port: u16, seed: u8) -> (Socket, String, TestClient) {
    let client = TestClient::new(seed);
    let (socket, key) = approved_as(
        fx,
        port,
        &client,
        BridgeAccess::ReadWrite,
        Some(&["web"]),
        Some("web"),
    )
    .await;
    (socket, key, client)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn passkey_create_with_a_new_entry_answers_like_a_platform_authenticator() {
    let mut fx = fixture();
    let login = entry(&fx, "Login", "https://www.example.de", &["web"]).await;
    entry(&fx, "Work", "https://example.de", &["work"]).await;
    entry(&fx, "Other", "https://other.de", &["web"]).await;
    let bridge = fx.bridge().await;
    let (mut socket, key, client) = approved_web(&mut fx, bridge.port(), 60).await;

    let payload = create_payload(json!({ "itemId": login }));
    let (id, question) = ask(
        &mut fx,
        &mut socket,
        &client,
        &key,
        "passkey-create",
        payload,
    )
    .await;
    assert_eq!(question.ceremony, PresenceCeremony::Create);
    assert_eq!(question.rp_id, "example.de");
    assert_eq!(question.rp_name.as_deref(), Some("Example"));
    assert_eq!(question.origin, ORIGIN);
    assert_eq!(question.user_name.as_deref(), Some("anna@example.de"));
    assert!(!question.require_uv);
    assert_eq!(
        question.challenge_hash,
        Sha256::digest(CHALLENGE)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    );
    assert_eq!(question.choices.len(), 2, "{:?}", question.choices);
    assert_eq!(question.choices[0], PresenceChoice::NewEntry);
    assert!(matches!(
        &question.choices[1],
        PresenceChoice::Entry { item_id, .. } if *item_id == login
    ));

    fx.answer_with(&id, confirm(&PresenceChoice::NewEntry, None));
    let answer = reply(&mut socket, &client).await;
    assert_eq!(answer["success"], true, "{answer}");
    let data = &answer["data"];
    assert_eq!(data["transports"], json!(["internal"]));
    assert!(data["passkeyId"].as_str().is_some_and(|id| !id.is_empty()));
    assert!(!std_bytes(&data["credentialId"]).is_empty());
    assert!(!std_bytes(&data["publicKey"]).is_empty());
    assert_eq!(
        attested_flags(&std_bytes(&data["attestationObject"])),
        0x59,
        "BE, BS, AT and UP; no UV"
    );
    let client_data: Value =
        serde_json::from_slice(&std_bytes(&data["clientDataJson"])).expect("client data");
    assert_eq!(client_data["type"], "webauthn.create");
    assert_eq!(client_data["challenge"], URL_SAFE_NO_PAD.encode(CHALLENGE));
    assert_eq!(client_data["origin"], ORIGIN);

    // A new entry with the relying party's name and the default tag; the injected id is ignored.
    let headers = fx
        .service()
        .list_headers(&Caller::User, &[])
        .await
        .expect("headers");
    let holzi_lib::passwords::service::Headers::Items(headers) = headers else {
        panic!("items");
    };
    let created = headers
        .iter()
        .find(|h| h.title.as_deref() == Some("Example"))
        .expect("the new entry");
    assert_eq!(created.tags.len(), 1);
    assert_eq!(created.tags[0].name, "web");
    let detail = fx
        .service()
        .get_item(&Caller::User, login)
        .await
        .expect("login");
    assert!(detail.passkeys.is_empty(), "the injected itemId is ignored");
    bridge.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn user_verification_takes_the_vault_password_and_a_wrong_one_creates_nothing() {
    let mut fx = fixture();
    let bridge = fx.bridge().await;
    let (mut socket, key, client) = approved_web(&mut fx, bridge.port(), 61).await;
    let required = json!({ "userVerification": "required" });

    let (id, question) = ask(
        &mut fx,
        &mut socket,
        &client,
        &key,
        "passkey-create",
        create_payload(required.clone()),
    )
    .await;
    assert!(question.require_uv);
    fx.answer_with(&id, confirm(&PresenceChoice::NewEntry, Some("falsch")));
    assert_eq!(
        error_code(&reply(&mut socket, &client).await),
        "USER_CANCELLED"
    );
    assert_eq!(passkey_count(&fx).await, 0);

    // No password where one is required counts the same.
    let (id, _) = ask(
        &mut fx,
        &mut socket,
        &client,
        &key,
        "passkey-create",
        create_payload(required.clone()),
    )
    .await;
    fx.answer_with(&id, confirm(&PresenceChoice::NewEntry, None));
    assert_eq!(
        error_code(&reply(&mut socket, &client).await),
        "USER_CANCELLED"
    );

    let (id, _) = ask(
        &mut fx,
        &mut socket,
        &client,
        &key,
        "passkey-create",
        create_payload(required),
    )
    .await;
    fx.answer_with(
        &id,
        confirm(&PresenceChoice::NewEntry, Some(VAULT_PASSWORD)),
    );
    let answer = reply(&mut socket, &client).await;
    assert_eq!(answer["success"], true, "{answer}");
    assert_eq!(
        attested_flags(&std_bytes(&answer["data"]["attestationObject"])),
        0x5d,
        "UP and UV"
    );
    assert_eq!(passkey_count(&fx).await, 1);
    bridge.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn passkey_get_offers_the_passkeys_and_signs_with_the_chosen_one() {
    let mut fx = fixture();
    let first = entry(&fx, "Anna", "https://example.de", &["web"]).await;
    let second = entry(&fx, "Ben", "https://example.de", &["web"]).await;
    passkey(&fx, &first, "anna").await;
    let ben = passkey(&fx, &second, "ben").await;
    let bridge = fx.bridge().await;
    let (mut socket, key, client) = approved_web(&mut fx, bridge.port(), 62).await;

    let (id, question) = ask(
        &mut fx,
        &mut socket,
        &client,
        &key,
        "passkey-get",
        get_payload(json!({})),
    )
    .await;
    assert_eq!(question.ceremony, PresenceCeremony::Get);
    assert_eq!(question.choices.len(), 2);
    let chosen = question
        .choices
        .iter()
        .find(|c| matches!(c, PresenceChoice::Passkey { user_name, .. } if user_name.as_deref() == Some("ben")))
        .expect("ben is offered")
        .clone();
    assert!(matches!(
        &chosen,
        PresenceChoice::Passkey { item_title, .. } if item_title.as_deref() == Some("Ben")
    ));
    fx.answer_with(&id, confirm(&chosen, None));
    let answer = reply(&mut socket, &client).await;
    assert_eq!(answer["success"], true, "{answer}");
    let data = &answer["data"];
    assert_eq!(
        URL_SAFE_NO_PAD.encode(std_bytes(&data["credentialId"])),
        ben.credential_id
    );
    assert_eq!(data["passkeyId"], json!(ben.passkey_id));
    assert_eq!(std_bytes(&data["userHandle"]), b"ben");
    let auth = std_bytes(&data["authenticatorData"]);
    assert_eq!(auth[32], 0x19, "UP, BE, BS");
    assert_eq!(&auth[33..37], &[0, 0, 0, 0], "the counter stays 0");
    let client_data = std_bytes(&data["clientDataJson"]);
    let mut message = auth.clone();
    message.extend_from_slice(&Sha256::digest(&client_data));
    let key_of_ben = p256::ecdsa::VerifyingKey::from_public_key_der(
        &URL_SAFE_NO_PAD.decode(&ben.public_key_spki).expect("spki"),
    )
    .expect("key");
    let signature = p256::ecdsa::Signature::from_der(&std_bytes(&data["signature"])).expect("DER");
    key_of_ben
        .verify(&message, &signature)
        .expect("the signature verifies");
    let detail = fx
        .service()
        .get_item(&Caller::User, second)
        .await
        .expect("ben");
    assert!(detail.passkeys[0].last_used_at.is_some());
    assert_eq!(detail.passkeys[0].sign_count, 0);
    bridge.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn passkey_list_gives_the_headers_in_the_scope_and_nothing_for_other_sites() {
    let mut fx = fixture();
    let web = entry(&fx, "Web", "https://example.de", &["web"]).await;
    let work = entry(&fx, "Work", "https://example.de", &["work"]).await;
    let mine = passkey(&fx, &web, "anna").await;
    passkey(&fx, &work, "boss").await;
    let bridge = fx.bridge().await;
    let (mut socket, key, client) = approved_web(&mut fx, bridge.port(), 63).await;

    let listed = call(
        &mut socket,
        &client,
        &key,
        "passkey-list",
        json!({ "relyingPartyId": "example.de" }),
    )
    .await;
    let passkeys = listed["data"]["passkeys"].as_array().expect("passkeys");
    assert_eq!(passkeys.len(), 1, "only the scope");
    let only = &passkeys[0];
    assert_eq!(
        URL_SAFE_NO_PAD.encode(std_bytes(&only["credentialId"])),
        mine.credential_id
    );
    assert_eq!(only["userName"], "anna");
    assert_eq!(only["isDiscoverable"], true);
    assert_eq!(only["itemId"], json!(web));
    let text = only.to_string();
    assert!(!text.contains("privateKey") && !text.contains("publicKey"));

    // A site without passkeys: the extension hands the sign-in to the browser (a security key).
    let other = call(
        &mut socket,
        &client,
        &key,
        "passkey-list",
        json!({ "relyingPartyId": "github.com" }),
    )
    .await;
    assert_eq!(other["data"]["passkeys"], json!([]));
    fx.no_question().await;
    bridge.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn what_would_fail_anyway_is_refused_without_a_question() {
    let mut fx = fixture();
    let web = entry(&fx, "Web", "https://example.de", &["web"]).await;
    let work = entry(&fx, "Work", "https://example.de", &["work"]).await;
    let in_scope = passkey(&fx, &web, "anna").await;
    let outside = passkey(&fx, &work, "boss").await;
    let rsa = PasskeyInput {
        item_id: Some(web.clone()),
        credential_id: STANDARD.encode(b"rsa-credential"),
        relying_party_id: "rsa.example.de".to_owned(),
        relying_party_name: None,
        user_name: Some("anna".to_owned()),
        user_display_name: None,
        user_handle: URL_SAFE_NO_PAD.encode(b"anna"),
        private_key: Zeroizing::new("AAAA".to_owned()),
        public_key: String::new(),
        algorithm: passkeys::RS256,
        sign_count: 0,
        is_discoverable: true,
        icon: None,
        color: None,
        nickname: None,
        created_at: None,
        last_used_at: None,
    };
    fx.db
        .write(move |tx| {
            passkeys::insert(tx, &rsa)
                .map(|_| ())
                .map_err(haex_crdt::Error::from)
        })
        .await
        .expect("rs256");
    let bridge = fx.bridge().await;
    let (mut socket, key, client) = approved_web(&mut fx, bridge.port(), 64).await;
    let std_id = |created: &PasskeyCreated| {
        STANDARD.encode(URL_SAFE_NO_PAD.decode(&created.credential_id).expect("id"))
    };

    let cases = [
        (
            "passkey-create",
            create_payload(json!({ "origin": null })),
            "ORIGIN_MISMATCH",
        ),
        (
            "passkey-create",
            create_payload(json!({ "origin": "https://example.de.evil.com" })),
            "ORIGIN_MISMATCH",
        ),
        (
            "passkey-get",
            get_payload(json!({ "origin": "https://evil.de" })),
            "ORIGIN_MISMATCH",
        ),
        (
            "passkey-create",
            create_payload(json!({ "excludeCredentials": [std_id(&in_scope)] })),
            "PASSKEY_EXCLUDED",
        ),
        (
            "passkey-get",
            get_payload(json!({
                "relyingPartyId": "rsa.example.de",
                "origin": "https://rsa.example.de",
            })),
            "UNSUPPORTED_ALGORITHM",
        ),
        (
            "passkey-get",
            get_payload(
                json!({ "allowCredentials": [{ "id": std_id(&outside), "type": "public-key" }] }),
            ),
            "NOT_FOUND",
        ),
    ];
    for (action, payload, code) in cases {
        let answer = call(&mut socket, &client, &key, action, payload).await;
        assert_eq!(error_code(&answer), code, "{action}");
    }
    fx.no_question().await;

    // A credential outside the scope does not exclude: the user is asked.
    let (id, _) = ask(
        &mut fx,
        &mut socket,
        &client,
        &key,
        "passkey-create",
        create_payload(json!({ "excludeCredentials": [std_id(&outside)] })),
    )
    .await;
    fx.answer_with(
        &id,
        BridgeAnswer::Presence(PresenceAnswer {
            confirm: false,
            choice: None,
            passphrase: None,
        }),
    );
    assert_eq!(
        error_code(&reply(&mut socket, &client).await),
        "USER_CANCELLED"
    );
    bridge.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn declining_running_out_or_leaving_creates_and_signs_nothing() {
    let mut fx = fixture_with_prompt_timeout(Duration::from_millis(500));
    let web = entry(&fx, "Web", "https://example.de", &["web"]).await;
    passkey(&fx, &web, "anna").await;
    let bridge = fx.bridge().await;
    let (mut socket, key, client) = approved_web(&mut fx, bridge.port(), 65).await;

    // A choice the question did not offer counts as declining.
    let (id, _) = ask(
        &mut fx,
        &mut socket,
        &client,
        &key,
        "passkey-create",
        create_payload(json!({})),
    )
    .await;
    let foreign = PresenceChoice::Entry {
        item_id: "not-offered".to_owned(),
        title: String::new(),
        url: None,
    };
    fx.answer_with(&id, confirm(&foreign, None));
    assert_eq!(
        error_code(&reply(&mut socket, &client).await),
        "USER_CANCELLED"
    );

    // Nobody answers.
    let (id, _) = ask(
        &mut fx,
        &mut socket,
        &client,
        &key,
        "passkey-get",
        get_payload(json!({})),
    )
    .await;
    assert_eq!(
        error_code(&reply(&mut socket, &client).await),
        "USER_CANCELLED"
    );
    assert!(fx
        .notices
        .lock()
        .expect("notices")
        .contains(&Notice::PromptCancelled(id)));
    assert_eq!(passkey_count(&fx).await, 1);

    // The page leaves while the question is open.
    let (id, _) = ask(
        &mut fx,
        &mut socket,
        &client,
        &key,
        "passkey-create",
        create_payload(json!({})),
    )
    .await;
    drop(socket);
    let gone = Notice::PromptCancelled(id.clone());
    for _ in 0..50 {
        if fx.notices.lock().expect("notices").contains(&gone) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(fx.notices.lock().expect("notices").contains(&gone));
    assert!(
        !fx.prompts
            .answer(&id, confirm(&PresenceChoice::NewEntry, None)),
        "the question is gone"
    );
    assert_eq!(passkey_count(&fx).await, 1);
    bridge.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_reader_is_asked_to_write_before_it_is_asked_to_confirm() {
    let mut fx = fixture();
    let bridge = fx.bridge().await;
    let client = TestClient::new(66);
    let (mut socket, key) = approved_as(
        &mut fx,
        bridge.port(),
        &client,
        BridgeAccess::Read,
        None,
        None,
    )
    .await;
    let mut payload = create_payload(json!({}));
    payload["requestId"] = json!("w");
    send(&mut socket, client.request(&key, "passkey-create", payload)).await;
    let (id, question) = fx.next_question().await;
    let BridgeQuestion::Write(write) = question else {
        panic!("the write question first");
    };
    assert_eq!(write.method, "passkey-create");
    fx.answer_with(
        &id,
        BridgeAnswer::Write(WriteAnswer {
            allow: true,
            remember: false,
            default_tag: None,
        }),
    );
    let (id, question) = fx.next_question().await;
    assert!(matches!(question, BridgeQuestion::Presence(_)));
    fx.answer_with(&id, confirm(&PresenceChoice::NewEntry, None));
    let answer = reply(&mut socket, &client).await;
    assert_eq!(answer["success"], true, "{answer}");
    bridge.stop().await;
}
