//! Tests for the provider-command boundary's adapter validation and
//! legacy-row repair.

use std::sync::Arc;

use haex_crdt::{Database, DatabaseConfig, NoopSignatureProvider, SqlCipherKey};
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::{do_refresh, legacy_adapter_for_url, validate_adapter};
use crate::error::HaexError;
use crate::identity::{
    haex_migration_source, installation_id_path, HaexBootstrap, HAEX_TRIGGER_VERSION,
};
use crate::model_capabilities::{
    ModelCapabilities, ReasoningControl, ToolSupport, ToolUse, ToolUseBasis,
};
use crate::storage::models::{self as models_store, IntegrityStatus, ModelRow, SourceKind};
use crate::storage::providers::{Provider, ProviderCapability, ProviderKind};
use crate::storage::query;
use crate::vault_gate::VaultGate;

#[test]
/// Only adapters with an implemented protocol may be persisted.
fn validate_adapter_rejects_missing_and_unknown_values() {
    assert!(validate_adapter(Some("anthropic")).is_ok());
    assert!(matches!(
        validate_adapter(None),
        Err(HaexError::InvalidInput { reason }) if reason.contains("requires adapter")
    ));
    assert!(matches!(
        validate_adapter(Some("openai")),
        Err(HaexError::InvalidInput { reason }) if reason.contains("unsupported")
    ));
}

#[test]
/// Legacy rows are repaired only when their endpoint identifies Anthropic.
fn legacy_adapter_repair_does_not_guess_from_kind() {
    assert_eq!(
        legacy_adapter_for_url(Some("https://api.anthropic.com/")),
        Some("anthropic")
    );
    assert_eq!(legacy_adapter_for_url(Some("https://api.openai.com")), None);
    assert_eq!(legacy_adapter_for_url(None), None);
}

// --- Model refresh and capabilities (spec 012) ------------------------------

pub(super) fn open_vault(dir: &std::path::Path) -> Arc<Database> {
    Arc::new(
        Database::open(DatabaseConfig {
            path: dir.join("vault.db"),
            key: SqlCipherKey::new("provider-refresh-capabilities-test"),
            create_if_missing: true,
            bootstrap: Arc::new(HaexBootstrap::new(installation_id_path(dir))),
            signature_provider: Arc::new(NoopSignatureProvider),
            migration_source: haex_migration_source(),
            trigger_version: HAEX_TRIGGER_VERSION,
            max_transaction_bytes: haex_crdt::MAX_CRDT_TRANSACTION_BYTES,
            max_value_bytes: haex_crdt::MAX_VALUE_BYTES,
        })
        .expect("open vault"),
    )
}

fn anthropic_provider(base_url: String) -> Provider {
    Provider {
        id: Uuid::new_v4(),
        kind: ProviderKind::ApiKey,
        adapter: Some("anthropic".to_string()),
        name: "Anthropic".to_string(),
        base_url: Some(base_url),
        credentials: Some(b"sk-test".to_vec()),
        created_at: 0,
        capability: ProviderCapability::Chat,
    }
}

fn cached_row(provider: &Provider, capabilities: Option<ModelCapabilities>) -> ModelRow {
    ModelRow {
        id: format!("{}:claude-opus-5", provider.id),
        provider_id: provider.id,
        name: "Claude Opus 5".to_string(),
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
        capabilities,
    }
}

fn seed(db: &Arc<Database>, provider: &Provider, capabilities: Option<ModelCapabilities>) {
    let row = cached_row(provider, capabilities);
    db.write(|tx| {
        models_store::replace_provider_models(tx, provider.id, &[row]).expect("seed cache");
        Ok(())
    })
    .expect("seed");
}

fn stored_capabilities(db: &Arc<Database>, provider: &Provider) -> Option<ModelCapabilities> {
    let id = format!("{}:claude-opus-5", provider.id);
    query::read(db, |r| {
        Ok(models_store::get_model(r, &id).expect("get model"))
    })
    .expect("read")
    .expect("row exists")
    .capabilities
}

fn full_listing() -> serde_json::Value {
    serde_json::json!({
        "data": [{
            "id": "claude-opus-5",
            "display_name": "Claude Opus 5",
            "capabilities": {
                "image_input": { "supported": true },
                "pdf_input": { "supported": true },
                "thinking": {
                    "supported": true,
                    "types": {
                        "enabled": { "supported": false },
                        "adaptive": { "supported": true }
                    }
                },
                "effort": {
                    "supported": true,
                    "low": { "supported": true },
                    "high": { "supported": true }
                }
            }
        }],
        "has_more": false
    })
}

async fn serve(status: u16, body: serde_json::Value) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(ResponseTemplate::new(status).set_body_json(body))
        .mount(&server)
        .await;
    server
}

#[tokio::test]
/// A model cached before capabilities existed is `NULL`; one refresh makes
/// it determined.
async fn a_refresh_makes_a_not_yet_determined_row_determined() {
    let dir = tempfile::tempdir().expect("tmp dir");
    let db = open_vault(dir.path());
    let server = serve(200, full_listing()).await;
    let provider = anthropic_provider(server.uri());
    seed(&db, &provider, None);

    do_refresh(
        &VaultGate::new()
            .vault_db(Arc::clone(&db))
            .expect("open gate"),
        &provider,
    )
    .await
    .expect("refresh succeeds");

    let caps = stored_capabilities(&db, &provider).expect("now determined");
    assert!(matches!(
        caps.reasoning,
        Some(ReasoningControl::Presets { ref options }) if options.len() == 2
    ));
}

#[tokio::test]
/// A successful refresh replaces what was stored with the new answer, even
/// when the new answer is "nothing determined" — no stale merging.
async fn a_refresh_with_absent_capabilities_replaces_the_stored_record() {
    let dir = tempfile::tempdir().expect("tmp dir");
    let db = open_vault(dir.path());
    let server = serve(
        200,
        serde_json::json!({
            "data": [{ "id": "claude-opus-5", "display_name": "Claude Opus 5" }],
            "has_more": false
        }),
    )
    .await;
    let provider = anthropic_provider(server.uri());
    seed(
        &db,
        &provider,
        Some(ModelCapabilities::local("Qwen3-4B-Instruct")),
    );

    do_refresh(
        &VaultGate::new()
            .vault_db(Arc::clone(&db))
            .expect("open gate"),
        &provider,
    )
    .await
    .expect("refresh succeeds");

    // The refresh replaced the local record entirely: only what Anthropic always states remains.
    assert_eq!(
        stored_capabilities(&db, &provider),
        Some(ModelCapabilities {
            tool_use: Some(ToolUse::new(ToolSupport::Supported, ToolUseBasis::Provider)),
            ..ModelCapabilities::default()
        })
    );
}

#[tokio::test]
/// A failed refresh must leave the previously learned capabilities intact
/// (spec 012 FR-010).
async fn a_failed_refresh_keeps_the_stored_capabilities() {
    let dir = tempfile::tempdir().expect("tmp dir");
    let db = open_vault(dir.path());
    let server = serve(500, serde_json::json!({ "error": "boom" })).await;
    let provider = anthropic_provider(server.uri());
    let learned = ModelCapabilities::local("Qwen3-4B-Instruct");
    seed(&db, &provider, Some(learned.clone()));

    let result = do_refresh(
        &VaultGate::new()
            .vault_db(Arc::clone(&db))
            .expect("open gate"),
        &provider,
    )
    .await;

    assert!(result.is_err(), "the listing failed");
    assert_eq!(stored_capabilities(&db, &provider), Some(learned));
}

// Deleting a provider row removes the credential entry of a vendor that keeps it in the
// password manager, with the last row only (spec 096).

mod delegate_rows {
    use super::*;
    use crate::providers::delegate_credential::{self, Read, KIND_DEVICE_CODE, VAULT_SCOPE};
    use crate::providers::delete_provider_in;
    use crate::storage::providers::{self as providers_store, ProviderCapability};
    use crate::vault_gate::VaultDb;

    const FIRST: &str = "00000000-0000-4000-8000-000000000001";
    const SECOND: &str = "00000000-0000-4000-8000-000000000002";

    fn row(id: &str, vendor: &str) -> Provider {
        Provider {
            id: Uuid::parse_str(id).expect("uuid"),
            kind: ProviderKind::CliDelegate,
            adapter: Some(vendor.to_owned()),
            name: vendor.to_owned(),
            base_url: None,
            credentials: Some(Vec::new()),
            created_at: 0,
            capability: ProviderCapability::Chat,
        }
    }

    fn vault_with(rows: Vec<Provider>) -> (tempfile::TempDir, VaultDb) {
        let dir = tempfile::tempdir().expect("tmp dir");
        let db = open_vault(dir.path());
        let seeded = rows.clone();
        db.write(move |tx| {
            for row in &seeded {
                providers_store::insert_provider(tx, row)?;
            }
            Ok(())
        })
        .expect("seed");
        let gate = VaultGate::new().vault_db(db).expect("vault gate");
        (dir, gate)
    }

    async fn has_entry(db: &VaultDb) -> bool {
        matches!(
            delegate_credential::read(db, "abacusai", VAULT_SCOPE)
                .await
                .expect("read"),
            Read::Ready(_)
        )
    }

    async fn store_entry(db: &VaultDb) {
        delegate_credential::save(
            db,
            "abacusai",
            KIND_DEVICE_CODE,
            "a-device-code",
            VAULT_SCOPE,
        )
        .await
        .expect("save");
    }

    #[tokio::test]
    async fn deleting_the_last_abacus_row_deletes_the_credential_entry() {
        let (_dir, db) = vault_with(vec![row(FIRST, "abacusai")]);
        store_entry(&db).await;

        delete_provider_in(&db, Uuid::parse_str(FIRST).unwrap())
            .await
            .expect("delete");

        assert!(!has_entry(&db).await);
    }

    #[tokio::test]
    async fn deleting_a_duplicate_row_keeps_the_entry() {
        let (_dir, db) = vault_with(vec![row(FIRST, "abacusai"), row(SECOND, "abacusai")]);
        store_entry(&db).await;

        delete_provider_in(&db, Uuid::parse_str(SECOND).unwrap())
            .await
            .expect("delete the duplicate");
        assert!(has_entry(&db).await, "a row survives, so does the entry");

        delete_provider_in(&db, Uuid::parse_str(FIRST).unwrap())
            .await
            .expect("delete the last");
        assert!(!has_entry(&db).await);
    }

    #[tokio::test]
    async fn a_missing_entry_is_not_an_error() {
        let (_dir, db) = vault_with(vec![row(FIRST, "abacusai")]);

        delete_provider_in(&db, Uuid::parse_str(FIRST).unwrap())
            .await
            .expect("no entry to delete");
    }

    #[tokio::test]
    async fn deleting_a_claude_or_codex_row_touches_no_credential_entry() {
        let (_dir, db) = vault_with(vec![
            row(FIRST, "abacusai"),
            row(SECOND, "claude"),
            row("00000000-0000-4000-8000-000000000003", "codex"),
        ]);
        store_entry(&db).await;

        for id in [SECOND, "00000000-0000-4000-8000-000000000003"] {
            delete_provider_in(&db, Uuid::parse_str(id).unwrap())
                .await
                .expect("delete");
        }

        assert!(has_entry(&db).await);
    }

    #[tokio::test]
    async fn deleting_an_unknown_row_changes_nothing() {
        let (_dir, db) = vault_with(vec![row(FIRST, "abacusai")]);
        store_entry(&db).await;

        delete_provider_in(&db, Uuid::parse_str(SECOND).unwrap())
            .await
            .expect("nothing to delete");

        assert!(has_entry(&db).await);
    }
}
