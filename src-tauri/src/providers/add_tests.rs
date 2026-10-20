//! Tests for adding a provider (spec 055 T052): an OpenAI-compatible server is stored with its
//! address and key, its models are read at once, a server without a key needs none, and a wrong
//! key keeps the row and says why without naming the key.

use std::sync::Arc;

use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::add::{add, AddProviderArgs};
use super::providers_tests::open_vault;
use super::OPENAI_COMPATIBLE;
use crate::error::HolziError;
use crate::storage::models as models_store;
use crate::storage::providers::{self as providers_store, ProviderKind};
use crate::storage::query;
use crate::vault_gate::{VaultDb, VaultGate};

const KEY: &str = "sk-school-0123456789abcdef";

fn args(base_url: &str, key: Option<&str>) -> AddProviderArgs {
    AddProviderArgs {
        kind: ProviderKind::ApiKey,
        name: "School server".into(),
        adapter: Some(OPENAI_COMPATIBLE.into()),
        base_url: Some(base_url.into()),
        api_key: key.map(str::to_owned),
    }
}

async fn serve(status: u16, ids: &[&str]) -> MockServer {
    let server = MockServer::start().await;
    let data: Vec<_> = ids.iter().map(|id| json!({ "id": id })).collect();
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(ResponseTemplate::new(status).set_body_json(json!({ "data": data })))
        .mount(&server)
        .await;
    server
}

fn gate(db: &Arc<haex_crdt::Database>) -> VaultDb {
    VaultGate::new().vault_db(Arc::clone(db)).expect("gate")
}

#[tokio::test]
async fn a_server_is_stored_with_address_and_key_and_its_models_are_read_at_once() {
    let dir = tempfile::tempdir().expect("tmp dir");
    let db = open_vault(dir.path());
    let server = serve(200, &["qwen3-vl", "llama3"]).await;
    let address = format!("{}/v1", server.uri());

    let added = add(&gate(&db), args(&address, Some(KEY)), 7)
        .await
        .expect("added");

    assert_eq!(added.model_count, Some(2));
    assert_eq!(added.refresh_error, None);
    assert!(added.provider.has_credentials);
    let id = added.provider.id;
    let (stored, models) = query::read(&db, |r| {
        Ok((
            providers_store::get_provider(r, id).expect("get"),
            models_store::list_models_by_provider(r, id).expect("models"),
        ))
    })
    .expect("read");
    let stored = stored.expect("row");
    assert_eq!(stored.base_url.as_deref(), Some(address.as_str()));
    assert_eq!(stored.adapter.as_deref(), Some(OPENAI_COMPATIBLE));
    assert_eq!(stored.credentials.as_deref(), Some(KEY.as_bytes()));
    assert_eq!(stored.created_at, 7);
    assert_eq!(models.len(), 2);
    let requests = server.received_requests().await.expect("recorded");
    assert_eq!(
        requests[0].headers.get("authorization").expect("header"),
        format!("Bearer {KEY}").as_str()
    );
    // The vault is encrypted at rest: the key is in none of its files in plain text.
    drop(db);
    for entry in std::fs::read_dir(dir.path()).expect("dir") {
        let bytes = std::fs::read(entry.expect("entry").path()).unwrap_or_default();
        assert!(
            !bytes.windows(KEY.len()).any(|w| w == KEY.as_bytes()),
            "the key lies in plain text on disk"
        );
    }
}

#[tokio::test]
async fn a_server_without_a_key_needs_none() {
    let dir = tempfile::tempdir().expect("tmp dir");
    let db = open_vault(dir.path());
    let server = serve(200, &["qwen3-vl"]).await;

    let added = add(
        &gate(&db),
        args(&format!("{}/v1", server.uri()), Some("  ")),
        1,
    )
    .await
    .expect("added");

    assert!(!added.provider.has_credentials, "a blank key is no key");
    assert_eq!(added.model_count, Some(1));
}

#[tokio::test]
async fn a_wrong_key_keeps_the_server_and_says_why_without_the_key() {
    let dir = tempfile::tempdir().expect("tmp dir");
    let db = open_vault(dir.path());
    let server = serve(401, &[]).await;

    let added = add(
        &gate(&db),
        args(&format!("{}/v1", server.uri()), Some(KEY)),
        1,
    )
    .await
    .expect("the row is kept");

    let error = added.refresh_error.expect("the refresh failed");
    assert!(!error.contains(KEY), "{error}");
    assert_eq!(added.model_count, None);
    let id = added.provider.id;
    let stored = query::read(&db, |r| {
        Ok(providers_store::get_provider(r, id).expect("get"))
    })
    .expect("read");
    assert!(stored.is_some(), "the user can fix the key and retry");
}

#[tokio::test]
async fn an_address_is_required_and_another_vendor_still_needs_its_key() {
    let dir = tempfile::tempdir().expect("tmp dir");
    let db = open_vault(dir.path());
    let refused = |outcome| matches!(outcome, Err(HolziError::InvalidInput { .. }));

    assert!(refused(add(&gate(&db), args("", None), 1).await));
    assert!(refused(
        add(&gate(&db), args("ftp://example.com/v1", None), 1).await
    ));
    assert!(refused(
        add(
            &gate(&db),
            args("https://user:password@example.com/v1", None),
            1,
        )
        .await
    ));
    assert!(refused(
        add(
            &gate(&db),
            args("https://example.com/v1?key=secret", None),
            1,
        )
        .await
    ));
    let mut anthropic = args("https://api.anthropic.com", None);
    anthropic.adapter = Some("anthropic".into());
    assert!(refused(add(&gate(&db), anthropic, 1).await));
    let mut nameless = args("http://localhost:11434/v1", None);
    nameless.name = " ".into();
    assert!(refused(add(&gate(&db), nameless, 1).await));
    let rows = query::read(&db, |r| {
        Ok(providers_store::list_providers(r).expect("list"))
    })
    .expect("read");
    assert!(rows.is_empty(), "nothing is stored");
}
