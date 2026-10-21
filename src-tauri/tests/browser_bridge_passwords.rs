//! Spec 049, the password methods of the External Bridge against a real vault: filling and TOTP
//! (US2, T044, SC-002, SC-003) and saving, presets, the write question and the unsupported methods
//! (US3, T048, SC-009).

#[path = "common/bridge_fixture.rs"]
mod bridge_fixture;

use std::time::{Duration, Instant};

use serde_json::{json, Value};

use bridge_fixture::*;
use holzi_lib::browser_bridge::approvals::Status;
use holzi_lib::browser_bridge::declaration::BridgeAccess;
use holzi_lib::browser_bridge::prompts::{BridgeAnswer, BridgeQuestion, WriteAnswer};
use holzi_lib::passwords::access::Caller;
use holzi_lib::passwords::model::{ItemInput, PresetInput};

fn tags(list: &[&str]) -> Vec<String> {
    list.iter().map(|t| (*t).to_owned()).collect()
}

async fn create(fx: &Fixture, title: &str, url: &str, tag_list: &[&str], totp: bool) -> String {
    fx.service()
        .create_item(
            &Caller::User,
            &[],
            ItemInput {
                title: Some(title.to_owned()),
                url: Some(url.to_owned()),
                username: Some(format!("user-{title}")),
                password: Some(format!("pw-{title}")),
                otp_secret: totp.then(|| "JBSWY3DPEHPK3PXP".to_owned()),
                tags: tags(tag_list),
                ..ItemInput::default()
            },
            None,
        )
        .await
        .expect("create")
}

fn entry<'a>(reply: &'a Value, id: &str) -> Option<&'a Value> {
    reply["data"]["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .find(|e| e["id"] == id)
}

fn error_code(reply: &Value) -> &str {
    assert_eq!(reply["success"], false, "{reply}");
    reply["errorCode"].as_str().expect("errorCode")
}

// ---------------------------------------------------------------------------
// User Story 2: filling and TOTP
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn get_items_finds_the_site_in_the_scope_and_nothing_else() {
    let mut fx = fixture();
    let plain = create(&fx, "plain", "https://example.de", &["web"], false).await;
    let login = create(&fx, "login", "https://www.example.de/login", &["web"], true).await;
    let lookalike = create(&fx, "bad", "https://bad-example.de", &["web"], false).await;
    let untagged = create(&fx, "untagged", "https://example.de", &[], false).await;
    let trashed = create(&fx, "trashed", "https://example.de", &["web"], false).await;
    fx.service()
        .delete_item(&Caller::User, &[], trashed.clone())
        .await
        .expect("trash");
    // A username that points into an entry outside the scope (ADR-0009): the field is left out.
    let referencing = fx
        .service()
        .create_item(
            &Caller::User,
            &[],
            ItemInput {
                title: Some("ref".to_owned()),
                url: Some("https://example.de".to_owned()),
                username: Some(format!("{{${untagged}:username}}")),
                password: Some("pw-ref".to_owned()),
                tags: tags(&["web"]),
                ..ItemInput::default()
            },
            None,
        )
        .await
        .expect("create");

    let bridge = fx.bridge().await;
    let client = TestClient::new(21);
    let (mut socket, key) = approved_as(
        &mut fx,
        bridge.port(),
        &client,
        BridgeAccess::Read,
        Some(&["web"]),
        None,
    )
    .await;

    let reply = call(
        &mut socket,
        &client,
        &key,
        "get-items",
        json!({ "url": "https://login.example.de/x" }),
    )
    .await;
    assert_eq!(reply["success"], true, "{reply}");
    let found: Vec<&str> = reply["data"]["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .map(|e| e["id"].as_str().expect("id"))
        .collect();
    assert_eq!(found.len(), 3, "{found:?}");
    for id in [&plain, &login, &referencing] {
        assert!(found.contains(&id.as_str()), "{id} is a hit");
    }
    for id in [&lookalike, &untagged, &trashed] {
        assert!(!found.contains(&id.as_str()), "{id} is no hit");
    }

    let with_totp = entry(&reply, &login).expect("login");
    assert_eq!(with_totp["title"], "login");
    assert_eq!(with_totp["url"], "https://www.example.de/login");
    assert_eq!(with_totp["hasTotp"], true);
    assert_eq!(
        with_totp["fields"],
        json!({ "username": "user-login", "password": "pw-login", "otp": "TOTP" })
    );
    let text = reply.to_string();
    for forbidden in [
        "otpSecret",
        "JBSWY3DPEHPK3PXP",
        "note",
        "attachments",
        "passkeys",
    ] {
        assert!(
            !text.contains(forbidden),
            "{forbidden} must not reach the client"
        );
    }
    let with_reference = entry(&reply, &referencing).expect("ref");
    assert_eq!(with_reference["fields"], json!({ "password": "pw-ref" }));

    let only_otp = call(
        &mut socket,
        &client,
        &key,
        "get-items",
        json!({ "url": "example.de", "fields": ["otp"] }),
    )
    .await;
    let ids: Vec<&Value> = only_otp["data"]["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .collect();
    assert_eq!(ids.len(), 1);
    assert_eq!(ids[0]["id"], json!(login));

    let without_url = call(&mut socket, &client, &key, "get-items", json!({})).await;
    assert_eq!(error_code(&without_url), "INVALID_REQUEST");
    bridge.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn get_totp_gives_the_code_in_the_scope_and_the_same_answer_for_everything_else() {
    let mut fx = fixture();
    let login = create(&fx, "login", "https://example.de", &["web"], true).await;
    let no_totp = create(&fx, "plain", "https://example.de", &["web"], false).await;
    let outside = create(&fx, "outside", "https://example.de", &[], true).await;
    let bridge = fx.bridge().await;
    let client = TestClient::new(22);
    let (mut socket, key) = approved_as(
        &mut fx,
        bridge.port(),
        &client,
        BridgeAccess::Read,
        Some(&["web"]),
        None,
    )
    .await;

    let reply = call(
        &mut socket,
        &client,
        &key,
        "get-totp",
        json!({ "entryId": login }),
    )
    .await;
    assert_eq!(reply["success"], true, "{reply}");
    let code = reply["data"]["code"].as_str().expect("code");
    assert_eq!(code.len(), 6);
    assert!(code.chars().all(|c| c.is_ascii_digit()));
    assert!(reply["data"]["validFor"].as_u64().expect("validFor") <= 30);
    assert!(!reply.to_string().contains("JBSWY3DPEHPK3PXP"));

    let mut answers = Vec::new();
    for id in [&no_totp, &outside, &"missing".to_owned()] {
        let reply = call(
            &mut socket,
            &client,
            &key,
            "get-totp",
            json!({ "entryId": id }),
        )
        .await;
        answers.push((error_code(&reply).to_owned(), reply["error"].clone()));
    }
    assert!(answers.iter().all(|a| *a == answers[0]), "{answers:?}");
    assert_eq!(answers[0].0, "NOT_FOUND");
    bridge.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn get_items_in_a_large_vault_answers_quickly() {
    // SC-002: 5 000 entries, 20 hits, under 500 ms (the seeding takes most of the ten seconds).
    let mut fx = fixture();
    for n in 0..5_000 {
        let url = if n < 20 {
            format!("https://s{n}.example.de")
        } else {
            format!("https://site{n}.example.org")
        };
        create(&fx, &format!("e{n}"), &url, &["web"], false).await;
    }
    let bridge = fx.bridge().await;
    let client = TestClient::new(23);
    let (mut socket, key) = approved(&mut fx, bridge.port(), &client).await;
    let started = Instant::now();
    let reply = call(
        &mut socket,
        &client,
        &key,
        "get-items",
        json!({ "url": "https://example.de" }),
    )
    .await;
    let took = started.elapsed();
    assert_eq!(
        reply["data"]["entries"].as_array().expect("entries").len(),
        20
    );
    assert!(took < Duration::from_millis(500), "get-items took {took:?}");
    bridge.stop().await;
}

// ---------------------------------------------------------------------------
// User Story 3: saving, presets, the write question
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_presets_reach_every_grant() {
    let mut fx = fixture();
    let bridge = fx.bridge().await;
    let client = TestClient::new(24);
    let (mut socket, key) = approved_as(
        &mut fx,
        bridge.port(),
        &client,
        BridgeAccess::Read,
        None,
        None,
    )
    .await;

    let none = call(&mut socket, &client, &key, "get-password-config", json!({})).await;
    assert_eq!(none["data"], json!({ "config": null, "presetName": null }));

    fx.service()
        .preset_save(
            &Caller::User,
            PresetInput {
                name: "Standard".to_owned(),
                length: 24,
                is_default: true,
                ..PresetInput::default()
            },
        )
        .await
        .expect("preset");
    let config = call(&mut socket, &client, &key, "get-password-config", json!({})).await;
    assert_eq!(config["data"]["presetName"], "Standard");
    let keys: Vec<&str> = config["data"]["config"]
        .as_object()
        .expect("config")
        .keys()
        .map(String::as_str)
        .collect();
    for key in [
        "length",
        "uppercase",
        "lowercase",
        "numbers",
        "symbols",
        "excludeChars",
        "usePattern",
        "pattern",
    ] {
        assert!(keys.contains(&key), "{key}");
    }
    assert_eq!(config["data"]["config"]["length"], 24);
    let presets = call(
        &mut socket,
        &client,
        &key,
        "get-password-presets",
        json!({}),
    )
    .await;
    assert_eq!(presets["data"]["presets"][0]["isDefault"], true);
    assert_eq!(presets["data"]["presets"][0]["name"], "Standard");
    bridge.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn create_item_takes_four_fields_and_the_default_tag() {
    let mut fx = fixture();
    let bridge = fx.bridge().await;
    let client = TestClient::new(25);
    let (mut socket, key) = approved_as(
        &mut fx,
        bridge.port(),
        &client,
        BridgeAccess::ReadWrite,
        Some(&["web"]),
        Some("web"),
    )
    .await;
    let reply = call(
        &mut socket,
        &client,
        &key,
        "create-item",
        json!({
            "title": "", "url": "https://shop.de/x", "username": "anna", "password": "geheim",
            "groupId": "g", "otpSecret": "JBSWY3DPEHPK3PXP", "iconBase64": "AAAA", "tags": ["other"]
        }),
    )
    .await;
    assert_eq!(reply["success"], true, "{reply}");
    assert_eq!(reply["data"]["title"], "shop.de");
    let id = reply["data"]["entryId"].as_str().expect("id").to_owned();
    let detail = fx
        .service()
        .get_item(&Caller::User, id.clone())
        .await
        .expect("entry");
    assert_eq!(detail.header.title.as_deref(), Some("shop.de"));
    assert_eq!(detail.header.url.as_deref(), Some("https://shop.de/x"));
    assert_eq!(detail.header.username.as_deref(), Some("anna"));
    assert_eq!(
        detail
            .header
            .tags
            .iter()
            .map(|t| t.name.as_str())
            .collect::<Vec<_>>(),
        ["web"]
    );
    assert_eq!(detail.header.group_id, None);
    assert!(!detail.has_otp_secret);
    assert_eq!(detail.header.icon, None);
    let history = fx
        .service()
        .history_list(&Caller::User, id)
        .await
        .expect("history");
    assert_eq!(history.len(), 1, "a first state in the history");
    bridge.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unsupported_methods_and_targets_change_nothing() {
    let mut fx = fixture();
    let existing = create(&fx, "keep", "https://example.de", &["web"], false).await;
    let bridge = fx.bridge().await;
    let client = TestClient::new(26);
    let (mut socket, key) = approved(&mut fx, bridge.port(), &client).await;
    for action in [
        "update-item",
        "bookmarks-collections-list",
        "bookmarks-collection-create",
        "bookmarks-list",
        "bookmarks-upsert",
        "bookmarks-delete",
        "bookmarks-device-upsert",
        "delete-item",
    ] {
        let reply = call(
            &mut socket,
            &client,
            &key,
            action,
            json!({ "id": existing, "title": "changed", "name": "c" }),
        )
        .await;
        assert_eq!(error_code(&reply), "NOT_SUPPORTED", "{action}");
    }
    // Another target than the password manager (FR-029).
    let mut other = client.request(
        &key,
        "get-items",
        json!({ "url": "https://example.de", "requestId": "t" }),
    );
    other["extensionPublicKey"] = json!("abc");
    send(&mut socket, other).await;
    let response = recv(&mut socket).await.expect("response");
    let reply = client.open(&response);
    assert_eq!(error_code(&reply), "NOT_SUPPORTED");
    let detail = fx
        .service()
        .get_item(&Caller::User, existing)
        .await
        .expect("entry");
    assert_eq!(detail.header.title.as_deref(), Some("keep"));
    fx.no_question().await;
    bridge.stop().await;
}

/// A read-only client that saves: the write question, answered with `answer`.
async fn save_as_reader(
    fx: &mut Fixture,
    seed: u8,
    answer: Option<WriteAnswer>,
) -> (Value, String) {
    let bridge = fx.bridge().await;
    let client = TestClient::new(seed);
    let (mut socket, key) = approved_as(
        fx,
        bridge.port(),
        &client,
        BridgeAccess::Read,
        Some(&["web"]),
        None,
    )
    .await;
    let mut payload = json!({ "title": "Shop", "url": "https://shop.de", "requestId": "w" });
    send(
        &mut socket,
        client.request(&key, "create-item", payload.take()),
    )
    .await;
    let (id, question) = fx.next_question().await;
    let BridgeQuestion::Write(question) = question else {
        panic!("a write question");
    };
    assert_eq!(question.method, "create-item");
    assert!(question.needs_default_tag);
    if let Some(answer) = answer {
        fx.answer_with(&id, BridgeAnswer::Write(answer));
    }
    let response = recv(&mut socket).await.expect("response");
    let reply = client.open(&response);
    bridge.stop().await;
    (reply, client.id)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_reader_is_asked_before_it_writes_and_a_no_changes_nothing() {
    let mut fx = fixture();
    let (reply, _) = save_as_reader(
        &mut fx,
        27,
        Some(WriteAnswer {
            allow: false,
            remember: false,
            default_tag: None,
        }),
    )
    .await;
    assert_eq!(error_code(&reply), "PERMISSION_DENIED");
    assert_eq!(
        fx.service()
            .list_headers(&Caller::User, &[])
            .await
            .map(|h| match h {
                holzi_lib::passwords::service::Headers::Items(items) => items.len(),
                _ => usize::MAX,
            })
            .expect("headers"),
        0
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn allowing_with_remember_widens_the_approval_for_good() {
    let mut fx = fixture();
    let (reply, client_id) = save_as_reader(
        &mut fx,
        28,
        Some(WriteAnswer {
            allow: true,
            remember: true,
            default_tag: Some("web".to_owned()),
        }),
    )
    .await;
    assert_eq!(reply["success"], true, "{reply}");
    let Some(stored) = fx.stored(&client_id).await else {
        panic!("remembered");
    };
    let Status::Allowed(approval) = stored.status else {
        panic!("allowed");
    };
    assert_eq!(approval.access, BridgeAccess::ReadWrite);
    assert_eq!(approval.default_tag.as_deref(), Some("web"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_remembered_no_to_writing_holds_without_a_question() {
    let mut fx = fixture();
    let bridge = fx.bridge().await;
    let client = TestClient::new(29);
    let (mut socket, key) = approved_as(
        &mut fx,
        bridge.port(),
        &client,
        BridgeAccess::Read,
        Some(&["web"]),
        None,
    )
    .await;
    send(
        &mut socket,
        client.request(
            &key,
            "create-item",
            json!({ "title": "a", "requestId": "1" }),
        ),
    )
    .await;
    let (id, _) = fx.next_question().await;
    fx.answer_with(
        &id,
        BridgeAnswer::Write(WriteAnswer {
            allow: false,
            remember: true,
            default_tag: None,
        }),
    );
    let first = client.open(&recv(&mut socket).await.expect("response"));
    assert_eq!(error_code(&first), "PERMISSION_DENIED");
    let second = call(
        &mut socket,
        &client,
        &key,
        "create-item",
        json!({ "title": "b" }),
    )
    .await;
    assert_eq!(error_code(&second), "PERMISSION_DENIED");
    fx.no_question().await;
    bridge.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_declaration_without_writing_is_refused_without_a_question() {
    let mut fx = fixture();
    let bridge = fx.bridge().await;
    let client = TestClient::new(30);
    let mut socket = extension(bridge.port()).await;
    send(
        &mut socket,
        client.handshake_with(
            2,
            json!({ "core": { "passwords": [{ "target": "*", "operation": "read" }] } }),
        ),
    )
    .await;
    let first = recv(&mut socket).await.expect("answer");
    let (id, _) = fx.next_question().await;
    fx.answer_with(
        &id,
        BridgeAnswer::Approval(holzi_lib::browser_bridge::prompts::ApprovalAnswer {
            allow: true,
            remember: true,
            access: Some(BridgeAccess::Read),
            scope_tags: None,
            default_tag: None,
        }),
    );
    assert_eq!(recv(&mut socket).await.expect("update")["authorized"], true);
    let key = first["serverPublicKey"].as_str().expect("key").to_owned();
    let reply = call(
        &mut socket,
        &client,
        &key,
        "create-item",
        json!({ "title": "a" }),
    )
    .await;
    assert_eq!(error_code(&reply), "PERMISSION_DENIED");
    fx.no_question().await;
    bridge.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unanswered_write_question_times_out() {
    let mut fx = fixture_with_prompt_timeout(Duration::from_millis(1000));
    let bridge = fx.bridge().await;
    let client = TestClient::new(31);
    let (mut socket, key) = approved_as(
        &mut fx,
        bridge.port(),
        &client,
        BridgeAccess::Read,
        None,
        None,
    )
    .await;
    send(
        &mut socket,
        client.request(
            &key,
            "create-item",
            json!({ "title": "a", "requestId": "1" }),
        ),
    )
    .await;
    fx.next_question().await;
    let reply = client.open(&recv(&mut socket).await.expect("response"));
    assert_eq!(error_code(&reply), "PERMISSION_PROMPT_TIMEOUT");
    bridge.stop().await;
}
