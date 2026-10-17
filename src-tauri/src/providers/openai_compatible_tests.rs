//! Tests for the generic OpenAI-compatible provider (spec 055 R7): its registration, a refresh
//! against a double, and the statements the user makes about a model (FR-019).

use std::sync::Arc;

use serde_json::json;
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::capabilities::{set_user_capabilities, SetUserCapabilitiesArgs};
use super::providers_tests::open_vault;
use super::{build_adapter, do_refresh, validate_adapter, OPENAI_COMPATIBLE};
use crate::adapters::AttachmentKind;
use crate::error::HolziError;
use crate::model_capabilities::{ModelCapabilities, ReasoningControl};
use crate::storage::models as models_store;
use crate::storage::providers::{Provider, ProviderCapability, ProviderKind};
use crate::storage::query;
use crate::vault_gate::VaultGate;

fn provider(base_url: String, key: Option<&str>) -> Provider {
    Provider {
        id: Uuid::new_v4(),
        kind: ProviderKind::ApiKey,
        adapter: Some(OPENAI_COMPATIBLE.to_string()),
        name: "School server".to_string(),
        base_url: Some(base_url),
        credentials: key.map(|k| k.as_bytes().to_vec()),
        created_at: 0,
        capability: ProviderCapability::Chat,
    }
}

async fn serve_models(ids: &[&str]) -> MockServer {
    let server = MockServer::start().await;
    let data: Vec<_> = ids.iter().map(|id| json!({"id": id})).collect();
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": data})))
        .mount(&server)
        .await;
    server
}

#[test]
fn the_adapter_id_is_accepted_and_a_lookalike_is_not() {
    assert!(validate_adapter(Some(OPENAI_COMPATIBLE)).is_ok());
    assert!(matches!(
        validate_adapter(Some("openai")),
        Err(HolziError::InvalidInput { .. })
    ));
}

#[tokio::test]
async fn a_provider_without_a_key_builds_and_sends_no_authorization() {
    let server = serve_models(&["qwen3-vl"]).await;
    let adapter = build_adapter(&provider(format!("{}/v1", server.uri()), None), None)
        .expect("a keyless provider builds");

    let models = adapter.list_models().await.expect("list");

    assert_eq!(models.len(), 1);
    let requests = server.received_requests().await.expect("recorded");
    assert!(requests[0].headers.get("authorization").is_none());
}

#[tokio::test]
async fn a_key_is_sent_as_a_bearer_token() {
    let server = serve_models(&["qwen3-vl"]).await;
    let adapter = build_adapter(
        &provider(format!("{}/v1", server.uri()), Some("sk-school")),
        None,
    )
    .expect("builds");

    adapter.list_models().await.expect("list");

    let requests = server.received_requests().await.expect("recorded");
    assert_eq!(
        requests[0].headers.get("authorization").expect("header"),
        "Bearer sk-school"
    );
}

#[test]
fn an_anthropic_provider_still_needs_its_key() {
    let mut anthropic = provider("https://api.anthropic.com".to_string(), None);
    anthropic.adapter = Some("anthropic".to_string());

    match build_adapter(&anthropic, None) {
        Err(HolziError::InvalidInput { reason }) => assert!(reason.contains("credentials")),
        Err(other) => panic!("unexpected error: {other}"),
        Ok(_) => panic!("a missing key must fail"),
    }
}

#[tokio::test]
async fn a_refresh_stores_the_models_with_composite_ids() {
    let dir = tempfile::tempdir().expect("tmp dir");
    let db = open_vault(dir.path());
    let server = serve_models(&["qwen3-vl", "llama3"]).await;
    let provider = provider(format!("{}/v1", server.uri()), None);

    let count = do_refresh(
        &VaultGate::new().vault_db(Arc::clone(&db)).expect("gate"),
        &provider,
    )
    .await
    .expect("refresh");

    assert_eq!(count, 2);
    let rows = query::read(&db, |r| {
        Ok(models_store::list_models_by_provider(r, provider.id).expect("list"))
    })
    .expect("read");
    let ids: Vec<_> = rows.iter().map(|m| m.id.clone()).collect();
    assert!(ids.contains(&format!("{}:qwen3-vl", provider.id)));
    // The server says nothing about what a model can do.
    assert!(rows.iter().all(|m| m.capabilities.is_none()));
}

// --- What the user says about a model ----------------------------------------------------------

/// A vault with one refreshed model of a provider; returns the gate, the provider and the model id.
async fn with_one_model(
    dir: &std::path::Path,
) -> (
    Arc<haex_crdt::Database>,
    crate::vault_gate::VaultDb,
    Provider,
    String,
) {
    let db = open_vault(dir);
    let server = serve_models(&["qwen3-vl"]).await;
    let provider = provider(format!("{}/v1", server.uri()), None);
    let gate = VaultGate::new().vault_db(Arc::clone(&db)).expect("gate");
    do_refresh(&gate, &provider).await.expect("refresh");
    // The double must outlive the refresh only; later refreshes mount their own.
    drop(server);
    let id = format!("{}:qwen3-vl", provider.id);
    (db, gate, provider, id)
}

fn takes_images(capabilities: &ModelCapabilities) -> bool {
    capabilities
        .accepted_attachment_kinds
        .as_ref()
        .is_some_and(|kinds| kinds.contains(&AttachmentKind::Image))
}

fn stored(db: &Arc<haex_crdt::Database>, id: &str) -> Option<ModelCapabilities> {
    query::read(db, |r| Ok(models_store::get_model(r, id).expect("get")))
        .expect("read")
        .expect("row")
        .capabilities
}

#[tokio::test]
async fn the_user_can_say_a_model_takes_images_and_has_effort_levels() {
    let dir = tempfile::tempdir().expect("tmp dir");
    let (db, gate, _provider, id) = with_one_model(dir.path()).await;

    let capabilities = set_user_capabilities(
        &gate,
        SetUserCapabilitiesArgs {
            model_id: id.clone(),
            accepts_images: Some(true),
            effort_levels: Some(vec!["low".into(), "medium".into(), "high".into()]),
        },
    )
    .await
    .expect("set");

    // Every ordinary reader sees the statement.
    assert!(takes_images(&capabilities));
    assert!(matches!(
        capabilities.reasoning,
        Some(ReasoningControl::Presets { ref options }) if options.len() == 3
    ));
    assert_eq!(stored(&db, &id), Some(capabilities));
}

#[tokio::test]
async fn a_refresh_keeps_what_the_user_said() {
    let dir = tempfile::tempdir().expect("tmp dir");
    let (db, gate, provider, id) = with_one_model(dir.path()).await;
    set_user_capabilities(
        &gate,
        SetUserCapabilitiesArgs {
            model_id: id.clone(),
            accepts_images: Some(true),
            effort_levels: None,
        },
    )
    .await
    .expect("set");

    let server = serve_models(&["qwen3-vl"]).await;
    let mut moved = provider.clone();
    moved.base_url = Some(format!("{}/v1", server.uri()));
    do_refresh(&gate, &moved).await.expect("second refresh");

    let capabilities = stored(&db, &id).expect("still determined");
    assert!(takes_images(&capabilities));
}

#[tokio::test]
async fn an_empty_statement_withdraws_the_earlier_one() {
    let dir = tempfile::tempdir().expect("tmp dir");
    let (db, gate, _provider, id) = with_one_model(dir.path()).await;
    let say = |images| SetUserCapabilitiesArgs {
        model_id: id.clone(),
        accepts_images: images,
        effort_levels: None,
    };
    set_user_capabilities(&gate, say(Some(true)))
        .await
        .expect("set");

    set_user_capabilities(&gate, say(None))
        .await
        .expect("withdraw");

    assert_eq!(stored(&db, &id), None, "back to not determined");
}

#[tokio::test]
async fn bad_effort_levels_and_an_unknown_model_are_refused() {
    let dir = tempfile::tempdir().expect("tmp dir");
    let (_db, gate, _provider, id) = with_one_model(dir.path()).await;
    let levels = |levels: Vec<&str>| SetUserCapabilitiesArgs {
        model_id: id.clone(),
        accepts_images: None,
        effort_levels: Some(levels.into_iter().map(str::to_string).collect()),
    };

    for bad in [vec![""], vec!["two words"], vec!["low", "low"]] {
        assert!(
            matches!(
                set_user_capabilities(&gate, levels(bad.clone())).await,
                Err(HolziError::InvalidInput { .. })
            ),
            "{bad:?}"
        );
    }
    let too_many: Vec<String> = (0..9).map(|n| format!("l{n}")).collect();
    assert!(set_user_capabilities(
        &gate,
        SetUserCapabilitiesArgs {
            model_id: id.clone(),
            accepts_images: None,
            effort_levels: Some(too_many),
        }
    )
    .await
    .is_err());
    assert!(matches!(
        set_user_capabilities(
            &gate,
            SetUserCapabilitiesArgs {
                model_id: "nope:nope".into(),
                accepts_images: Some(true),
                effort_levels: None,
            }
        )
        .await,
        Err(HolziError::InvalidInput { .. })
    ));
}
