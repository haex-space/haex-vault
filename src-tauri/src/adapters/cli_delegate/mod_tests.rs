//! Tests for `DelegateVendor` parsing and `build_adapter`'s `CliDelegate`
//! construction path (tasks.md T007).

use uuid::Uuid;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::{build_transcript_prompt, CliDelegateAdapter, DelegateChatContext, DelegateVendor};
use crate::adapters::types::{Attachment, AttachmentKind, ChatMessage, ChatRequest, ChatRole};
use crate::adapters::{AdapterError, ProviderAdapter};
use crate::cli_tools::platform::PlatformKey;
use crate::cli_tools::runtime::{DelegateTools, RuntimeSource};
use crate::instances::vault_paths::VaultPaths;
use crate::providers::build_adapter;
use crate::storage::providers::{Provider, ProviderCapability, ProviderKind};

#[test]
fn delegate_vendor_round_trips() {
    for vendor in [DelegateVendor::Claude, DelegateVendor::Codex] {
        let parsed = DelegateVendor::parse(vendor.as_str());
        assert_eq!(parsed, Some(vendor));
    }
}

#[test]
fn delegate_vendor_rejects_unknown_strings() {
    assert_eq!(DelegateVendor::parse("anthropic"), None);
    assert_eq!(DelegateVendor::parse(""), None);
    assert_eq!(DelegateVendor::parse("Claude"), None);
}

#[test]
fn transcript_gives_duplicate_attachment_names_unique_sandbox_names() {
    let request = ChatRequest {
        sampling: Default::default(),
        model_id: "claude-opus-5".to_string(),
        thread_id: None,
        system_prompt: None,
        messages: vec![ChatMessage {
            role: ChatRole::User,
            content: "read both files".to_string(),
            attachments: vec![
                Attachment {
                    name: "report.txt".to_string(),
                    kind: AttachmentKind::Text,
                    media_type: "text/plain".to_string(),
                    bytes: b"first".to_vec(),
                },
                Attachment {
                    name: "report.txt".to_string(),
                    kind: AttachmentKind::Text,
                    media_type: "text/plain".to_string(),
                    bytes: b"second".to_vec(),
                },
            ],
        }],
        reasoning_requested: false,
        max_new_tokens: None,
        tools: Vec::new(),
        autonomy_mode: Default::default(),
        autonomy_deny: Default::default(),
        reasoning_option: Default::default(),
        capabilities: Default::default(),
    };

    let prompt = build_transcript_prompt(&request);
    assert!(prompt.contains("attachment-0-report.txt"));
    assert!(prompt.contains("attachment-1-report.txt"));
}

fn sample_provider(adapter: Option<&str>) -> Provider {
    Provider {
        id: Uuid::new_v4(),
        kind: ProviderKind::CliDelegate,
        adapter: adapter.map(str::to_string),
        name: "Claude Code".to_string(),
        // A delegate row has no address: the tool is the vault's own installation (spec 094).
        base_url: None,
        credentials: Some(b"fake-oauth-token".to_vec()),
        created_at: 0,
        capability: ProviderCapability::Chat,
    }
}

fn minimal_request() -> ChatRequest {
    ChatRequest {
        sampling: Default::default(),
        model_id: "claude-opus-5".to_string(),
        thread_id: None,
        system_prompt: None,
        messages: vec![ChatMessage {
            role: ChatRole::User,
            content: "hello".to_string(),
            attachments: Vec::new(),
        }],
        reasoning_requested: false,
        max_new_tokens: None,
        tools: Vec::new(),
        autonomy_mode: Default::default(),
        autonomy_deny: Default::default(),
        reasoning_option: Default::default(),
        capabilities: Default::default(),
    }
}

fn context(tools: Option<DelegateTools>) -> DelegateChatContext {
    DelegateChatContext {
        pending_tool_approvals: Default::default(),
        emit: std::sync::Arc::new(|_event: &str, _payload: serde_json::Value| {}),
        database: None,
        children: Default::default(),
        haex: None,
        deny_rules: Vec::new(),
        tools,
    }
}

async fn start_error(
    vendor: DelegateVendor,
    source: RuntimeSource,
    ctx: DelegateChatContext,
) -> String {
    let adapter = CliDelegateAdapter::new(vendor, b"fake-token".to_vec(), source, Some(ctx));
    match adapter.stream_chat(minimal_request()).await {
        Err(AdapterError::Unavailable { reason }) => reason,
        Err(other) => panic!("expected Unavailable, got {other:?}"),
        Ok(_) => panic!("a tool that is not installed must not start"),
    }
}

#[tokio::test]
async fn without_an_installation_in_the_vault_a_start_ends_before_anything_runs() {
    let data = tempfile::TempDir::new().unwrap();
    let tools = DelegateTools::new(VaultPaths::new(data.path()), "anna");
    for vendor in [DelegateVendor::Claude, DelegateVendor::Codex] {
        let source = RuntimeSource::Vault {
            tools: tools.clone(),
            vendor: vendor.as_str(),
        };
        let reason = start_error(vendor, source, context(Some(tools.clone()))).await;
        assert_eq!(
            reason,
            format!("not installed in this vault: {}", vendor.as_str())
        );
    }
    // Nothing was started, so nothing was written: no run folder, no home.
    assert_eq!(std::fs::read_dir(data.path()).unwrap().count(), 0);
}

#[tokio::test]
async fn an_adapter_built_without_the_vaults_tools_never_starts_a_tool() {
    let reason = start_error(
        DelegateVendor::Codex,
        RuntimeSource::Unwired { vendor: "codex" },
        context(None),
    )
    .await;
    assert_eq!(reason, "not installed in this vault: codex");
}

#[tokio::test]
async fn an_installation_for_another_device_is_a_reinstall_and_is_not_started() {
    let data = tempfile::TempDir::new().unwrap();
    let paths = VaultPaths::new(data.path());
    let tools = DelegateTools::new(paths.clone(), "anna");
    // A complete installation, but made for a platform this device is not.
    let foreign = if PlatformKey::host() == Some(PlatformKey::DarwinArm64) {
        PlatformKey::Win32X64
    } else {
        PlatformKey::DarwinArm64
    };
    let vendor = crate::cli_tools::catalog::Catalog::shipped()
        .vendor("claude")
        .unwrap();
    let version_dir = paths.cli_tool_dir("anna", "claude").join("2.1.311");
    let program = version_dir.join(vendor.executable_for(foreign).unwrap());
    std::fs::create_dir_all(program.parent().unwrap()).unwrap();
    std::fs::write(&program, b"program").unwrap();
    crate::cli_tools::installed::InstallMarker::new(
        "claude",
        "2.1.311",
        foreign,
        &"a".repeat(64),
        1,
        7,
        vec![crate::cli_tools::installed::InstalledFile {
            path: vendor.executable_for(foreign).unwrap().into(),
            size: 7,
        }],
    )
    .write(&version_dir)
    .unwrap();

    let source = RuntimeSource::Vault {
        tools: tools.clone(),
        vendor: "claude",
    };
    let reason = start_error(DelegateVendor::Claude, source, context(Some(tools))).await;
    assert_eq!(reason, "needs reinstall: claude");
}

#[test]
fn build_adapter_ignores_an_address_a_row_still_carries() {
    // A host path in the old `base_url` column is never read: the tool is the vault's own.
    let mut provider = sample_provider(Some("claude"));
    provider.base_url = Some("/usr/local/bin/claude".to_string());
    let adapter = build_adapter(&provider, None).expect("a delegate adapter");
    assert_eq!(adapter.delegate_vendor(), Some(DelegateVendor::Claude));
}

#[test]
fn build_adapter_constructs_cli_delegate_for_well_formed_row() {
    let provider = sample_provider(Some("claude"));
    let adapter = build_adapter(&provider, None).expect("a delegate adapter");
    // The chat learns from it which delegate runs (spec 021 US6).
    assert_eq!(adapter.delegate_vendor(), Some(DelegateVendor::Claude));
}

#[test]
fn build_adapter_errors_on_malformed_adapter_value() {
    let provider = sample_provider(Some("not-a-real-vendor"));
    let err = build_adapter(&provider, None)
        .err()
        .expect("malformed adapter must be rejected");
    let message = format!("{err}");
    assert!(
        message.contains("invalid or missing adapter"),
        "unexpected error message: {message}"
    );
}

#[test]
fn build_adapter_errors_on_missing_adapter_value() {
    let provider = sample_provider(None);
    assert!(build_adapter(&provider, None).is_err());
}

#[test]
fn build_adapter_errors_on_missing_credentials() {
    let mut provider = sample_provider(Some("codex"));
    provider.credentials = None;
    let err = build_adapter(&provider, None)
        .err()
        .expect("missing credentials must be rejected");
    let message = format!("{err}");
    assert!(
        message.contains("missing credentials"),
        "unexpected error message: {message}"
    );
}

/// Codex's models are asked of the program in the vault. An adapter built without it (a refresh
/// that has no tools) invents no placeholder model: it says the program is missing, and the
/// refresh keeps the list it has.
#[tokio::test]
async fn codex_without_its_program_lists_no_invented_model() {
    let provider = sample_provider(Some("codex"));
    let adapter = build_adapter(&provider, None).expect("delegate adapter should build");

    let error = adapter
        .list_models()
        .await
        .expect_err("no program, no list");

    assert!(
        matches!(error, AdapterError::Unavailable { .. }),
        "unexpected error: {error}"
    );
}

/// What the account's list says about a model is stored with its row: the reasoning efforts the
/// chat offers as "Aufwand", and the image input.
#[tokio::test]
async fn a_codex_model_with_listed_efforts_is_stored_with_its_capabilities() {
    let models = super::codex_models::parse_models(&serde_json::json!({
        "data": [{
            "id": "gpt-x", "model": "gpt-x", "displayName": "GPT-X",
            "supportedReasoningEfforts": [{"reasoningEffort": "low", "description": ""}],
            "inputModalities": ["text", "image"]
        }]
    }));

    let row = crate::providers::compose_model_row(Uuid::nil(), 0, models[0].clone());

    let capabilities = row
        .capabilities
        .expect("a described model is stored with them");
    assert!(
        matches!(
            capabilities.reasoning,
            Some(crate::model_capabilities::ReasoningControl::Presets { .. })
        ),
        "the efforts are stored: {capabilities:?}"
    );
}

#[tokio::test]
async fn claude_model_list_comes_from_the_models_api_via_oauth_bearer() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .and(header("authorization", "Bearer test-oauth-token"))
        .and(header("anthropic-beta", "oauth-2025-04-20"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": [
                {
                    "id": "claude-opus-5",
                    "display_name": "Claude Opus 5",
                    "type": "model",
                    "created_at": "2026-07-24T00:00:00Z",
                    "max_input_tokens": 200000,
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
                            "medium": { "supported": true },
                            "high": { "supported": true },
                            "xhigh": { "supported": true },
                            "max": { "supported": true }
                        }
                    }
                },
                {
                    "id": "claude-sonnet-5",
                    "display_name": "Claude Sonnet 5",
                    "type": "model",
                    "created_at": "2026-07-24T00:00:00Z",
                    "max_input_tokens": 200000,
                }
            ],
            "has_more": false,
            "first_id": "claude-opus-5",
            "last_id": "claude-sonnet-5"
        })))
        .expect(1)
        .mount(&server)
        .await;

    // Not routed through `CliDelegateAdapter`/`build_adapter`: the provider
    // row has no API endpoint, so `fetch_claude_models` takes the base URL
    // directly — that's what makes it testable against a wiremock server in
    // the first place.
    let models = super::fetch_claude_models(&server.uri(), "test-oauth-token")
        .await
        .expect("fetch_claude_models should succeed");

    assert_eq!(models.len(), 2);
    assert_eq!(models[0].remote_id, "claude-opus-5");
    assert_eq!(models[0].display_name, "Claude Opus 5");
    assert_eq!(models[1].remote_id, "claude-sonnet-5");

    // The delegate lists models through the same `fetch_models` the API-key
    // provider uses, so it carries the mapped live capabilities with no
    // delegate-specific code (spec 012 FR-003): the real supported levels
    // instead of a hardcoded "always all five".
    let opus = &models[0].capabilities;
    assert!(matches!(
        &opus.reasoning,
        Some(crate::model_capabilities::ReasoningControl::Presets { options }) if options.len() == 5
    ));
    assert_eq!(
        opus.thinking_style,
        Some(crate::model_capabilities::ThinkingStyle::Adaptive)
    );
    assert_eq!(
        models[1].capabilities.reasoning, None,
        "an entry without a capabilities object leaves reasoning not determined"
    );
}

#[tokio::test]
async fn claude_model_list_rejects_empty_credentials() {
    let mut provider = sample_provider(Some("claude"));
    provider.credentials = Some(b"   ".to_vec());
    let adapter = build_adapter(&provider, None).expect("delegate adapter should build");
    let err = adapter
        .list_models()
        .await
        .expect_err("blank credentials must be rejected before any network call");
    assert!(matches!(
        err,
        crate::adapters::AdapterError::InvalidCredentials
    ));
}
