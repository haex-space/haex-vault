//! The models of every provider are read again when a vault opens (spec 055 FR-037): a provider
//! that answers gets the models it offers now, one that does not keeps its list, a local one is
//! left alone.

use std::sync::Arc;

use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::providers_tests::open_vault;
use super::refresh_all::refresh_all;
use crate::storage::models::{self as models_store, IntegrityStatus, ModelRow, SourceKind};
use crate::storage::providers::{
    self as providers_store, Provider, ProviderCapability, ProviderKind,
};
use crate::vault_gate::VaultGate;

fn provider(kind: ProviderKind, adapter: Option<&str>, base_url: Option<String>) -> Provider {
    Provider {
        id: Uuid::new_v4(),
        kind,
        adapter: adapter.map(str::to_owned),
        name: "Test".into(),
        base_url,
        credentials: Some(b"sk-test".to_vec()),
        created_at: 0,
        capability: ProviderCapability::Chat,
    }
}

fn row(provider: &Provider, remote: &str) -> ModelRow {
    ModelRow {
        id: format!("{}:{remote}", provider.id),
        provider_id: provider.id,
        name: remote.to_owned(),
        context_window: None,
        fetched_at: Some(1),
        tokenizer_repo: None,
        hf_repo: None,
        hf_filename: None,
        hf_revision: None,
        hf_revision_ref: None,
        file_sha256: None,
        integrity_status: IntegrityStatus::Unknown,
        source_kind: SourceKind::Provider,
        capabilities: None,
    }
}

fn store(db: &crate::vault_gate::VaultDb, provider: &Provider, cached: &[&str]) {
    let provider = provider.clone();
    let rows: Vec<ModelRow> = cached.iter().map(|name| row(&provider, name)).collect();
    db.write_blocking(move |tx| {
        providers_store::insert_provider(tx, &provider)?;
        models_store::replace_provider_models(tx, provider.id, &rows)?;
        Ok(())
    })
    .expect("store");
}

fn names(db: &crate::vault_gate::VaultDb, provider: &Provider) -> Vec<String> {
    let id = provider.id;
    let mut names: Vec<String> = db
        .read_blocking(move |r| models_store::list_models_by_provider(r, id))
        .expect("read")
        .into_iter()
        .map(|m| m.name)
        .collect();
    names.sort();
    names
}

async fn serve(models: &[&str]) -> MockServer {
    let server = MockServer::start().await;
    let data: Vec<_> = models
        .iter()
        .map(|id| serde_json::json!({ "id": id, "display_name": id }))
        .collect();
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({ "data": data, "has_more": false })),
        )
        .mount(&server)
        .await;
    server
}

#[tokio::test]
async fn a_provider_that_answers_gets_its_new_models_and_one_that_does_not_keeps_its_list() {
    let dir = tempfile::tempdir().expect("tmp dir");
    let db = VaultGate::new()
        .vault_db(Arc::clone(&open_vault(dir.path())))
        .expect("open gate");

    let live = serve(&["claude-opus-5", "claude-opus-5-5"]).await;
    let answering = provider(ProviderKind::ApiKey, Some("anthropic"), Some(live.uri()));
    store(&db, &answering, &["claude-opus-5"]);

    // Nothing listens at this port.
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let silent = provider(
        ProviderKind::ApiKey,
        Some("anthropic"),
        Some(format!("http://127.0.0.1:{port}")),
    );
    store(&db, &silent, &["claude-sonnet-5"]);

    let local = provider(ProviderKind::Local, None, None);
    store(&db, &local, &["gguf-model"]);

    assert_eq!(refresh_all(&db).await, 1, "one provider answered");

    assert_eq!(names(&db, &answering), ["claude-opus-5", "claude-opus-5-5"]);
    assert_eq!(
        names(&db, &silent),
        ["claude-sonnet-5"],
        "the old list stays"
    );
    assert_eq!(
        names(&db, &local),
        ["gguf-model"],
        "a local model is not a listing"
    );
}
