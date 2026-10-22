//! Spec 049, nothing secret leaves the bridge (T074, SC-004, research R16): markers in a TOTP secret,
//! the passkeys' private keys, an entry outside the scope, the vault password and the request
//! itself never show up in an answer, an error, a question to the window or a log line, on the
//! paths that work and on those that fail. The test binary has one capturing logger, so it holds
//! only this test.

#[path = "common/bridge_fixture.rs"]
mod bridge_fixture;

use std::sync::Mutex;

use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine as _;
use serde_json::{json, Value};

use bridge_fixture::*;
use holzi_lib::browser_bridge::declaration::BridgeAccess;
use holzi_lib::browser_bridge::prompts::{BridgeAnswer, BridgeQuestion, PresenceAnswer};
use holzi_lib::instances::passphrase::Passphrase;
use holzi_lib::passwords::access::Caller;
use holzi_lib::passwords::model::ItemInput;
use holzi_lib::passwords::model_passkeys::{PasskeyCreateRequest, PasskeyTarget, UserPresence};
use holzi_lib::storage::query::Query as _;

const TOTP_SECRET: &str = "LEAKMARKERTOTPSECRETQQQQQQQQQQQQ";
const HIDDEN_TOTP_SECRET: &str = "LEAKMARKERHIDDENTOTPQQQQQQQQQQQQ";
const HIDDEN_TITLE: &str = "leak-marker-hidden-title";
const HIDDEN_PASSWORD: &str = "leak-marker-hidden-password";
const HIDDEN_USER: &str = "leak-marker-hidden-user";
const PAGE_URL: &str = "https://leak-marker-page.example.de/login?session=leak-marker-query";
const SAVED_PASSWORD: &str = "leak-marker-saved-password";
const WRONG_PASSPHRASE: &str = "leak-marker-wrong-passphrase";
const MISSING_ID: &str = "leak-marker-missing-id";
const UNKNOWN_ACTION: &str = "leak-marker-action";
const FOREIGN_ORIGIN: &str = "https://leak-marker-origin.other.de";
const GARBAGE: &str = "leak-marker-garbage";

const MARKERS: &[&str] = &[
    TOTP_SECRET,
    HIDDEN_TOTP_SECRET,
    HIDDEN_TITLE,
    HIDDEN_PASSWORD,
    HIDDEN_USER,
    "leak-marker-page",
    "leak-marker-query",
    SAVED_PASSWORD,
    WRONG_PASSPHRASE,
    MISSING_ID,
    "leak-marker-origin",
    GARBAGE,
];

// ---------------------------------------------------------------------------
// The log of the whole binary
// ---------------------------------------------------------------------------

static LINES: Mutex<Vec<String>> = Mutex::new(Vec::new());

struct Capture;

impl log::Log for Capture {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        true
    }

    /// Keeps what a log could hold: holzi's own lines at every level, and every line at `Info` and
    /// above, the level holzi writes (`lib.rs`). Below that, tungstenite traces raw frames, whose
    /// envelope repeats the action in the clear and whose payload is sealed.
    fn log(&self, record: &log::Record<'_>) {
        if record.level() > log::Level::Info && !record.target().starts_with("holzi") {
            return;
        }
        LINES.lock().expect("lines").push(format!(
            "{} {}: {}",
            record.level(),
            record.target(),
            record.args()
        ));
    }

    fn flush(&self) {}
}

static CAPTURE: Capture = Capture;

fn capture_logs() {
    log::set_logger(&CAPTURE).expect("the only logger of this binary");
    log::set_max_level(log::LevelFilter::Trace);
}

// ---------------------------------------------------------------------------
// Seeding and talking
// ---------------------------------------------------------------------------

async fn entry(
    fx: &Fixture,
    title: &str,
    password: &str,
    otp_secret: &str,
    tags: &[&str],
) -> String {
    fx.service()
        .create_item(
            &Caller::User,
            &[],
            ItemInput {
                title: Some(title.to_owned()),
                url: Some("https://example.de".to_owned()),
                username: Some(format!("user-{title}")),
                password: Some(password.to_owned()),
                otp_secret: Some(otp_secret.to_owned()),
                tags: tags.iter().map(|t| (*t).to_owned()).collect(),
                ..ItemInput::default()
            },
            None,
        )
        .await
        .expect("entry")
}

async fn passkey(fx: &Fixture, item_id: &str, user: &str) {
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
        .expect("passkey");
}

/// Everything that left the bridge: raw frames, opened answers and questions to the window.
struct Talk<'a> {
    fx: &'a mut Fixture,
    socket: Socket,
    client: TestClient,
    key: String,
    seen: Vec<String>,
}

impl Talk<'_> {
    fn request(&self, action: &str, mut payload: Value) -> Value {
        payload["requestId"] = json!(format!("r-{action}"));
        self.client.request(&self.key, action, payload)
    }

    async fn frame(&mut self) -> Value {
        let frame = recv(&mut self.socket).await.expect("a frame");
        self.seen.push(frame.to_string());
        frame
    }

    async fn reply(&mut self) -> Value {
        let frame = self.frame().await;
        assert_eq!(frame["type"], "response", "{frame}");
        let reply = self.client.open(&frame);
        self.seen.push(reply.to_string());
        reply
    }

    async fn call(&mut self, action: &str, payload: Value) -> Value {
        let request = self.request(action, payload);
        send(&mut self.socket, request).await;
        self.reply().await
    }

    /// A ceremony: the presence question, then the answer `respond` picks for it.
    async fn ceremony(
        &mut self,
        action: &str,
        payload: Value,
        respond: impl FnOnce(&BridgeQuestion) -> BridgeAnswer,
    ) -> Value {
        let request = self.request(action, payload);
        send(&mut self.socket, request).await;
        let (id, question) = self.fx.next_question().await;
        self.seen.push(format!("{question:?}"));
        self.fx.answer_with(&id, respond(&question));
        self.reply().await
    }
}

fn code(reply: &Value) -> &str {
    assert_eq!(reply["success"], false, "{reply}");
    reply["errorCode"].as_str().expect("errorCode")
}

fn confirm_first(question: &BridgeQuestion, passphrase: Option<&str>) -> BridgeAnswer {
    let BridgeQuestion::Presence(question) = question else {
        panic!("a presence question, got {question:?}");
    };
    BridgeAnswer::Presence(PresenceAnswer {
        confirm: true,
        choice: question.choices.first().cloned(),
        passphrase: passphrase.map(Passphrase::from),
    })
}

fn decline(_: &BridgeQuestion) -> BridgeAnswer {
    BridgeAnswer::Presence(PresenceAnswer {
        confirm: false,
        choice: None,
        passphrase: None,
    })
}

fn create_payload(extra: Value) -> Value {
    let mut payload = json!({
        "relyingPartyId": "example.de",
        "relyingPartyName": "Example",
        "userHandle": STANDARD.encode(b"anna-handle"),
        "userName": "anna@example.de",
        "challenge": STANDARD.encode(b"a challenge"),
        "userVerification": "preferred",
        "origin": "https://login.example.de",
    });
    for (key, value) in extra.as_object().expect("object") {
        payload[key] = value.clone();
    }
    payload
}

fn get_payload(origin: &str) -> Value {
    json!({
        "relyingPartyId": "example.de",
        "challenge": STANDARD.encode(b"a challenge"),
        "userVerification": "preferred",
        "origin": origin,
    })
}

/// Every stored private key as text the way it could slip out: standard and URL-safe Base64 of the
/// PKCS#8 bytes, and their hex.
async fn private_key_markers(fx: &Fixture) -> Vec<String> {
    let keys: Vec<String> = fx
        .db
        .read(|q| {
            q.query_map(
                "SELECT private_key FROM haex_passwords_passkeys",
                &[],
                |r| r.get::<_, String>(0),
            )
        })
        .await
        .expect("keys");
    keys.into_iter()
        .flat_map(|key| {
            let der = STANDARD.decode(&key).expect("standard Base64");
            let hex: String = der.iter().map(|b| format!("{b:02x}")).collect();
            [key, URL_SAFE_NO_PAD.encode(&der), hex]
        })
        .collect()
}

// ---------------------------------------------------------------------------
// The probe
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn no_secret_reaches_an_answer_an_error_a_question_or_the_log() {
    capture_logs();
    let mut fx = fixture();
    let web = entry(&fx, "Web", "pw-web", TOTP_SECRET, &["web"]).await;
    let hidden = entry(
        &fx,
        HIDDEN_TITLE,
        HIDDEN_PASSWORD,
        HIDDEN_TOTP_SECRET,
        &["private"],
    )
    .await;
    passkey(&fx, &web, "anna").await;
    passkey(&fx, &hidden, HIDDEN_USER).await;
    let bridge = fx.bridge().await;
    let client = TestClient::new(70);
    let (socket, key) = approved_as(
        &mut fx,
        bridge.port(),
        &client,
        BridgeAccess::ReadWrite,
        Some(&["web"]),
        Some("web"),
    )
    .await;
    let mut talk = Talk {
        fx: &mut fx,
        socket,
        client,
        key,
        seen: Vec::new(),
    };

    // Filling and TOTP, inside and outside the scope.
    let found = talk.call("get-items", json!({ "url": PAGE_URL })).await;
    assert_eq!(
        found["data"]["entries"].as_array().expect("entries").len(),
        1
    );
    assert_eq!(found["data"]["entries"][0]["fields"]["otp"], "TOTP");
    assert_eq!(
        code(&talk.call("get-items", json!({ "url": 42 })).await),
        "INVALID_REQUEST"
    );
    assert_eq!(
        code(
            &talk
                .call("get-items", json!({ "url": "http://[leak-marker-page" }))
                .await
        ),
        "INVALID_REQUEST"
    );
    let totp = talk.call("get-totp", json!({ "entryId": web })).await;
    assert_eq!(totp["success"], true, "{totp}");
    for id in [hidden.as_str(), MISSING_ID] {
        assert_eq!(
            code(&talk.call("get-totp", json!({ "entryId": id })).await),
            "NOT_FOUND"
        );
    }

    // Saving and the presets.
    let saved = talk
        .call(
            "create-item",
            json!({ "title": "Saved", "url": "https://saved.example.de", "username": "u", "password": SAVED_PASSWORD }),
        )
        .await;
    assert_eq!(saved["success"], true, "{saved}");
    assert_eq!(
        code(&talk.call("create-item", json!({ "title": 5 })).await),
        "INVALID_REQUEST"
    );
    for action in ["get-password-config", "get-password-presets"] {
        let reply = talk.call(action, json!({})).await;
        assert_eq!(reply["success"], true, "{reply}");
    }

    // Passkeys: listing, signing, declining, a foreign origin, a wrong vault password, creating.
    let listed = talk
        .call("passkey-list", json!({ "relyingPartyId": "example.de" }))
        .await;
    assert_eq!(
        listed["data"]["passkeys"].as_array().expect("list").len(),
        1
    );
    let signed = talk
        .ceremony(
            "passkey-get",
            get_payload("https://login.example.de"),
            |q| confirm_first(q, None),
        )
        .await;
    assert_eq!(signed["success"], true, "{signed}");
    let declined = talk
        .ceremony(
            "passkey-get",
            get_payload("https://login.example.de"),
            decline,
        )
        .await;
    assert_eq!(code(&declined), "USER_CANCELLED");
    let foreign = talk.call("passkey-get", get_payload(FOREIGN_ORIGIN)).await;
    assert_eq!(code(&foreign), "ORIGIN_MISMATCH");
    let wrong = talk
        .ceremony(
            "passkey-create",
            create_payload(json!({ "userVerification": "required" })),
            |q| confirm_first(q, Some(WRONG_PASSPHRASE)),
        )
        .await;
    assert_eq!(code(&wrong), "USER_CANCELLED");
    let created = talk
        .ceremony("passkey-create", create_payload(json!({})), |q| {
            confirm_first(q, None)
        })
        .await;
    assert_eq!(created["success"], true, "{created}");

    // Methods holzi does not answer, and a request that does not open.
    assert_eq!(
        code(
            &talk
                .call("update-item", json!({ "password": SAVED_PASSWORD }))
                .await
        ),
        "NOT_SUPPORTED"
    );
    assert_eq!(
        code(&talk.call(UNKNOWN_ACTION, json!({})).await),
        "NOT_SUPPORTED"
    );
    let mut garbled = talk.request("get-items", json!({ "url": PAGE_URL }));
    garbled["message"] = json!(STANDARD.encode(GARBAGE));
    send(&mut talk.socket, garbled).await;
    let refused = talk.frame().await;
    assert_eq!(refused["code"], "DECRYPTION_ERROR", "{refused}");

    let seen = std::mem::take(&mut talk.seen);
    drop(talk);
    bridge.stop().await;
    let notices = format!("{:?}", fx.notices.lock().expect("notices"));
    let mut markers: Vec<String> = MARKERS.iter().map(|m| (*m).to_owned()).collect();
    let keys = private_key_markers(&fx).await;
    assert_eq!(keys.len(), 9, "three passkeys, three spellings each");
    markers.extend(keys);
    let lines = LINES.lock().expect("lines").clone();
    assert!(
        lines.iter().any(|l| l.contains("get-items -> ok")),
        "the logger hears the bridge: {lines:#?}"
    );
    // The answer's envelope repeats the action the client sent in the clear (wire protocol), but the
    // log names an unknown one only as "unknown".
    assert!(
        lines.iter().all(|l| !l.contains(UNKNOWN_ACTION)),
        "{UNKNOWN_ACTION} in the log: {lines:#?}"
    );
    for marker in &markers {
        for text in &seen {
            assert!(
                !text.contains(marker.as_str()),
                "{marker} in what the client or window saw: {text}"
            );
        }
        assert!(
            !notices.contains(marker.as_str()),
            "{marker} in a notice: {notices}"
        );
        for line in &lines {
            assert!(
                !line.contains(marker.as_str()),
                "{marker} in the log: {line}"
            );
        }
    }
}
