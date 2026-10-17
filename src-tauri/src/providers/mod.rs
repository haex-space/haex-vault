//! Provider commands: add / list / get / delete + refresh model listing.
//!
//! CRUD is provider-kind-agnostic; the refresh path dispatches on
//! the persisted adapter discriminator to build the right adapter.
//! `cli_delegate` live invocation is spec 007-cli-delegate.
//!
//! Maintainability exception (spaex 500-LoC rule): the add/list/get/delete
//! commands and the refresh path share the same `db`/adapter construction
//! and error mapping, so they have stayed together as the file grew past 500
//! lines. Concrete split plan, if this grows further: move `do_refresh`,
//! `compose_model_row` and the model-list payload structs into a
//! `providers/refresh.rs` next to `providers/connect.rs`, leaving the
//! provider CRUD commands here.

pub mod capabilities;
pub mod connect;
#[cfg(test)]
mod connect_tests;
pub mod local;
#[cfg(test)]
mod openai_compatible_tests;
#[cfg(test)]
mod providers_tests;

use serde::{Deserialize, Serialize};
use tauri::State;
use uuid::Uuid;

use crate::adapters::anthropic::AnthropicAdapter;
use crate::adapters::cli_delegate::{CliDelegateAdapter, DelegateChatContext, DelegateVendor};
use crate::adapters::openai_compatible::OpenAiCompatibleAdapter;
use crate::adapters::{AdapterError, ProviderAdapter, ProviderModel};
use crate::error::{HolziError, Result};
use crate::model_capabilities::ModelCapabilities;
use crate::state::AppState;
use crate::state_utils::active_database;
use crate::storage::models::{self as models_store, IntegrityStatus, ModelRow, SourceKind};
use crate::storage::providers::{self as storage, Provider, ProviderCapability, ProviderKind};
use crate::vault_gate::VaultDb;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddProviderArgs {
    pub kind: ProviderKind,
    pub name: String,
    /// Adapter/vendor discriminator. API-key providers currently support
    /// `anthropic`; CLI delegates support `claude` and `codex`.
    #[serde(default)]
    pub adapter: Option<String>,
    /// Base URL for API-key providers (e.g. `https://api.anthropic.com`) or
    /// the executable name/path for a CLI delegate. `None` for `local`.
    pub base_url: Option<String>,
    /// API key or CLI credential blob. Sent/stored as opaque UTF-8 bytes for
    /// API-key providers and as vendor-specific bytes for CLI delegates.
    pub api_key: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderPayload {
    pub id: Uuid,
    pub kind: ProviderKind,
    pub adapter: Option<String>,
    pub name: String,
    pub base_url: Option<String>,
    /// True when a credentials blob is present; the blob itself is
    /// never returned to the frontend.
    pub has_credentials: bool,
    pub created_at: i64,
}

impl From<Provider> for ProviderPayload {
    fn from(p: Provider) -> Self {
        ProviderPayload {
            id: p.id,
            kind: p.kind,
            adapter: p.adapter,
            name: p.name,
            base_url: p.base_url,
            has_credentials: p.credentials.is_some(),
            created_at: p.created_at,
        }
    }
}

/// Return value of [`add_provider`]. Carries the created provider plus
/// an optional `refresh_error` set when the auto-refresh triggered
/// after insert failed. The add itself always succeeds when the return
/// is `Ok(_)` — a refresh failure never rolls back the row so the
/// operator can retry manually with a fixed key.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddProviderResult {
    pub provider: ProviderPayload,
    pub model_count: Option<usize>,
    pub refresh_error: Option<String>,
}

/// Adds a provider. API-key providers require a supported adapter,
/// `base_url`, and a non-empty `api_key`; `local` needs neither. For
/// `api_key` providers the model catalog is refreshed inline; if that call
/// fails the provider is kept and `refresh_error` is set so the frontend can
/// toast it.
#[tauri::command]
pub async fn add_provider(
    state: State<'_, AppState>,
    args: AddProviderArgs,
) -> Result<AddProviderResult> {
    if args.name.trim().is_empty() {
        return Err(HolziError::InvalidInput {
            reason: "name is empty".into(),
        });
    }
    match args.kind {
        ProviderKind::ApiKey => {
            validate_adapter(args.adapter.as_deref())?;
            if args.base_url.as_deref().unwrap_or("").is_empty() {
                return Err(HolziError::InvalidInput {
                    reason: "api_key provider requires base_url".into(),
                });
            }
            // A server in the own network (Ollama, a school server) often wants no key.
            if !key_is_optional(args.adapter.as_deref())
                && args.api_key.as_deref().unwrap_or("").is_empty()
            {
                return Err(HolziError::InvalidInput {
                    reason: "api_key provider requires api_key".into(),
                });
            }
        }
        ProviderKind::CliDelegate => {
            // `adapter` is the vendor discriminator here (spec 007-cli-delegate
            // data-model.md), the same column role `api_key` providers already
            // use it for — not the "adapter is only for api_key" restriction
            // this used to enforce.
            match args.adapter.as_deref() {
                Some(v) if crate::adapters::cli_delegate::DelegateVendor::parse(v).is_some() => {}
                _ => {
                    return Err(HolziError::InvalidInput {
                        reason:
                            "cli_delegate provider requires adapter to be \"claude\" or \"codex\""
                                .into(),
                    });
                }
            }
            if args.base_url.as_deref().unwrap_or("").is_empty() {
                return Err(HolziError::InvalidInput {
                    reason: "cli_delegate provider requires base_url (cli command)".into(),
                });
            }
            if args
                .api_key
                .as_deref()
                .map(str::trim)
                .unwrap_or("")
                .is_empty()
            {
                return Err(HolziError::InvalidInput {
                    reason: "cli_delegate provider requires non-empty credentials".into(),
                });
            }
        }
        ProviderKind::Local => {
            // No requirements — one row per local runtime is enough.
            if args.adapter.is_some() {
                return Err(HolziError::InvalidInput {
                    reason: "adapter is only valid for api_key providers".into(),
                });
            }
        }
    }

    let db = active_database(&state)?;
    let now = now_ms();
    let key_optional = key_is_optional(args.adapter.as_deref());
    let provider = Provider {
        id: Uuid::new_v4(),
        kind: args.kind,
        adapter: args.adapter,
        name: args.name,
        base_url: args.base_url,
        credentials: args
            .api_key
            .filter(|k| !(key_optional && k.trim().is_empty()))
            .map(|k| k.into_bytes()),
        created_at: now,
        // `add_provider` only ever creates chat providers today (spec 008's
        // transcription capability is additive but not yet exposed through
        // this command — the bundled STT row comes from
        // `ensure_local_transcription_provider` instead).
        capability: ProviderCapability::Chat,
    };
    let inserted = provider.clone();
    db.write(move |tx| {
        storage::insert_provider(tx, &inserted)?;
        Ok(())
    })
    .await?;

    // Auto-refresh remote providers so the model picker is populated
    // immediately after the credentials land. Refresh failures are
    // surfaced to the caller but do not undo the insert — the operator
    // can retry `refresh_provider_models` after fixing the key.
    let (model_count, refresh_error) = if matches!(
        provider.kind,
        ProviderKind::ApiKey | ProviderKind::CliDelegate
    ) {
        match do_refresh(&db, &provider).await {
            Ok(count) => (Some(count), None),
            Err(e) => (None, Some(format_holzi_error(&e))),
        }
    } else {
        (None, None)
    };

    Ok(AddProviderResult {
        provider: provider.into(),
        model_count,
        refresh_error,
    })
}

/// Lists all providers configured on this instance.
#[tauri::command]
pub async fn list_providers(state: State<'_, AppState>) -> Result<Vec<ProviderPayload>> {
    let db = active_database(&state)?;
    let rows = db.read(move |r| storage::list_providers(r)).await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

/// Payload returned by [`refresh_provider_models`]. Reports what the
/// live fetch produced; the frontend can then re-query `models` via
/// the existing storage path for full display data.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshProviderModelsResult {
    pub provider_id: Uuid,
    pub model_count: usize,
    pub fetched_at: i64,
}

/// Fetches the given provider's model catalog live and replaces the
/// cached `models` rows for it. Plan §"Anbietermodelle": "Modelllisten
/// werden immer abgefragt, nie hartkodiert." A failed fetch leaves the
/// existing cache untouched — the caller sees an error and the UI
/// stays on the previous listing (or empty state).
///
/// Never routes for `ProviderKind::Local` — local models are managed
/// via the download/import commands. `ProviderKind::CliDelegate`
/// awaits a separate design pass and returns `InvalidInput`.
#[tauri::command]
pub async fn refresh_provider_models(
    state: State<'_, AppState>,
    provider_id: Uuid,
) -> Result<RefreshProviderModelsResult> {
    let db = active_database(&state)?;
    let provider = db
        .read(move |r| storage::get_provider(r, provider_id))
        .await?
        .ok_or_else(|| HolziError::InvalidInput {
            reason: format!("provider {provider_id} not found"),
        })?;

    let model_count = state.gate().run(do_refresh(&db, &provider)).await??;

    Ok(RefreshProviderModelsResult {
        provider_id,
        model_count,
        fetched_at: now_ms(),
    })
}

/// Frontend view of a provider-scoped model row. Local providers
/// intentionally return an empty list from this command — installed
/// GGUFs come through `list_installed_models` and carry file-system
/// info the api_key rows do not.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderModelPayload {
    pub id: String,
    pub name: String,
    pub provider_id: Uuid,
    pub context_window: Option<i64>,
    /// What this model supports (spec 012); `None` means not determined.
    pub capabilities: Option<ModelCapabilities>,
}

/// Reads the cached models for one provider. Does NOT trigger a live
/// fetch — call `refresh_provider_models` for that.
#[tauri::command]
pub async fn list_provider_models(
    state: State<'_, AppState>,
    provider_id: Uuid,
) -> Result<Vec<ProviderModelPayload>> {
    let db = active_database(&state)?;
    let rows = db
        .read(move |r| models_store::list_models_by_provider(r, provider_id))
        .await?;
    Ok(rows
        .into_iter()
        .map(|r| ProviderModelPayload {
            id: r.id,
            name: r.name,
            provider_id: r.provider_id,
            context_window: r.context_window,
            capabilities: r.capabilities,
        })
        .collect())
}

/// Shared refresh core used by both [`add_provider`] and
/// [`refresh_provider_models`]. Returns the number of models cached.
async fn do_refresh(db: &VaultDb, provider: &Provider) -> Result<usize> {
    let provider = repair_legacy_adapter(db, provider).await?;
    let adapter = build_adapter(&provider, None)?;
    let fetched = adapter.list_models().await.map_err(map_adapter_error)?;

    let fetched_at = now_ms();
    let provider_id = provider.id;
    let rows: Vec<ModelRow> = fetched
        .into_iter()
        .map(|m| compose_model_row(provider_id, fetched_at, m))
        .collect();
    let model_count = rows.len();

    db.write(move |tx| {
        models_store::replace_provider_models(tx, provider_id, &rows)?;
        Ok(())
    })
    .await?;

    Ok(model_count)
}

/// Repairs a legacy API-key row only when its stored base URL identifies the
/// Anthropic endpoint unambiguously. Other legacy rows stay unresolved and
/// are rejected by `build_adapter` instead of being guessed as Anthropic.
pub(crate) async fn repair_legacy_adapter(db: &VaultDb, provider: &Provider) -> Result<Provider> {
    if provider.adapter.is_some() {
        return Ok(provider.clone());
    }
    let Some(adapter) = legacy_adapter_for_url(provider.base_url.as_deref()) else {
        return Ok(provider.clone());
    };

    let provider_id = provider.id;
    db.write(move |tx| {
        storage::set_adapter(tx, provider_id, adapter)?;
        Ok(())
    })
    .await?;

    let mut repaired = provider.clone();
    repaired.adapter = Some(adapter.to_string());
    Ok(repaired)
}

fn legacy_adapter_for_url(base_url: Option<&str>) -> Option<&'static str> {
    match base_url?.trim_end_matches('/') {
        "https://api.anthropic.com" | "http://api.anthropic.com" => Some("anthropic"),
        _ => None,
    }
}

/// Builds the right adapter for the provider's kind and credentials.
/// `Local` is rejected here — the chat path uses `LocalAdapter`
/// directly with an in-process `LocalModel`; refresh only makes sense
/// for remote listings.
///
/// `delegate_chat_ctx` is only needed for a `cli_delegate` provider that
/// will actually run `stream_chat` (real chat use, not just
/// `list_models`) — pass `None` from a refresh-only call site such as
/// [`do_refresh`] (spec 007-cli-delegate data-model.md).
pub(crate) fn build_adapter(
    provider: &Provider,
    delegate_chat_ctx: Option<DelegateChatContext>,
) -> Result<Box<dyn ProviderAdapter>> {
    match provider.kind {
        ProviderKind::ApiKey => {
            let base_url =
                provider
                    .base_url
                    .as_deref()
                    .ok_or_else(|| HolziError::InvalidInput {
                        reason: "api_key provider is missing base_url".into(),
                    })?;
            let key = provider
                .credentials
                .as_ref()
                .map(|credentials| {
                    std::str::from_utf8(credentials.as_slice())
                        .map(str::to_string)
                        .map_err(|e| HolziError::InvalidInput {
                            reason: format!("api_key credentials are not UTF-8: {e}"),
                        })
                })
                .transpose()?;
            let adapter: Box<dyn ProviderAdapter> = match provider.adapter.as_deref() {
                Some("anthropic") => {
                    let api_key = key.ok_or_else(|| HolziError::InvalidInput {
                        reason: "api_key provider is missing credentials".into(),
                    })?;
                    Box::new(
                        AnthropicAdapter::new(base_url.to_string(), api_key)
                            .map_err(map_adapter_error)?,
                    )
                }
                Some(OPENAI_COMPATIBLE) => Box::new(
                    OpenAiCompatibleAdapter::new(base_url.to_string(), key.unwrap_or_default())
                        .map_err(map_adapter_error)?,
                ),
                Some(other) => {
                    return Err(HolziError::InvalidInput {
                        reason: format!("unsupported provider adapter: {other}"),
                    });
                }
                None => {
                    return Err(HolziError::InvalidInput {
                        reason: "api_key provider is missing adapter".into(),
                    });
                }
            };
            Ok(adapter)
        }
        ProviderKind::CliDelegate => {
            let vendor = provider
                .adapter
                .as_deref()
                .and_then(DelegateVendor::parse)
                .ok_or_else(|| HolziError::InvalidInput {
                    reason: "cli_delegate provider has an invalid or missing adapter".into(),
                })?;
            let credentials =
                provider
                    .credentials
                    .clone()
                    .ok_or_else(|| HolziError::InvalidInput {
                        reason: "cli_delegate provider is missing credentials".into(),
                    })?;
            let binary = provider.base_url.clone().unwrap_or_default();
            Ok(Box::new(CliDelegateAdapter::new(
                vendor,
                credentials,
                binary,
                delegate_chat_ctx,
            )))
        }
        ProviderKind::Local => Err(HolziError::InvalidInput {
            reason: "refresh does not apply to local providers".into(),
        }),
    }
}

/// Adapter id of the generic OpenAI-compatible provider (spec 055 R7).
pub(crate) const OPENAI_COMPATIBLE: &str = "openai_compatible";

/// Whether a provider of this adapter may be saved without a key.
fn key_is_optional(adapter: Option<&str>) -> bool {
    adapter == Some(OPENAI_COMPATIBLE)
}

/// Validates the adapter discriminator before storing an API-key provider.
/// Dispatch must remain explicit as more vendor adapters are added.
fn validate_adapter(adapter: Option<&str>) -> Result<()> {
    match adapter {
        Some("anthropic") | Some(OPENAI_COMPATIBLE) => Ok(()),
        Some(other) => Err(HolziError::InvalidInput {
            reason: format!("unsupported provider adapter: {other}"),
        }),
        None => Err(HolziError::InvalidInput {
            reason: "api_key provider requires adapter".into(),
        }),
    }
}

pub(crate) fn compose_model_row(provider_id: Uuid, fetched_at: i64, m: ProviderModel) -> ModelRow {
    // Composite id per plan §"Datenmodell". Keeps the same remote id
    // distinguishable when the operator configures two accounts with
    // the same provider.
    ModelRow {
        id: format!("{provider_id}:{}", m.remote_id),
        provider_id,
        name: m.display_name,
        context_window: m.context_window,
        fetched_at: Some(fetched_at),
        // API providers do not need a local tokenizer — the vendor's
        // server-side tokenizer handles that.
        tokenizer_repo: None,
        hf_repo: None,
        hf_filename: None,
        hf_revision: None,
        hf_revision_ref: None,
        file_sha256: None,
        integrity_status: IntegrityStatus::Unknown,
        source_kind: SourceKind::Provider,
        // An all-`None` answer is stored as SQL NULL ("not determined").
        capabilities: (!m.capabilities.is_undetermined()).then_some(m.capabilities),
    }
}

pub(crate) fn map_adapter_error(err: AdapterError) -> HolziError {
    // Surface `InvalidCredentials` as its own kind so the frontend can
    // render "Ungültige Zugangsdaten" per plan §"Anbietermodelle".
    // Other adapter errors ride the generic `InvalidInput` bucket for
    // now — a dedicated `HolziError::Provider*` variant lands with the
    // chat integration in slice (b), when the UI needs finer control.
    match err {
        AdapterError::InvalidCredentials => HolziError::InvalidInput {
            reason: "provider rejected credentials".into(),
        },
        AdapterError::Http { reason } => HolziError::InvalidInput {
            reason: format!("provider transport error: {reason}"),
        },
        AdapterError::Status { status, body } => HolziError::InvalidInput {
            reason: format!("provider returned {status}: {body}"),
        },
        AdapterError::Parse { reason } => HolziError::InvalidInput {
            reason: format!("provider response parse error: {reason}"),
        },
        AdapterError::Unavailable { reason } => HolziError::InvalidInput {
            reason: format!("backend unavailable: {reason}"),
        },
        AdapterError::UntrustedCertificate { reason } => {
            HolziError::UntrustedCertificate { reason }
        }
    }
}

fn format_holzi_error(err: &HolziError) -> String {
    match err {
        HolziError::InvalidInput { reason } => reason.clone(),
        other => other.to_string(),
    }
}

/// Deletes a provider by id. Callers (frontend) are responsible for
/// warning the operator when a provider still has referencing threads
/// or models — the schema does not enforce foreign keys.
#[tauri::command]
pub async fn delete_provider(state: State<'_, AppState>, id: Uuid) -> Result<()> {
    let db = active_database(&state)?;
    db.write(move |tx| {
        storage::delete_provider(tx, id)?;
        Ok(())
    })
    .await?;
    Ok(())
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
