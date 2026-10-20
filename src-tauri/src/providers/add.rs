//! Adding a provider (spec 001, spec 055 T052): the checks per kind, the row, and the first
//! refresh of its models. A failed refresh keeps the row, so the user can fix the key and retry.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{do_refresh, format_holzi_error, key_is_optional, validate_adapter, ProviderPayload};
use crate::error::{HolziError, Result};
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
pub(crate) async fn add(
    db: &VaultDb,
    args: AddProviderArgs,
    now: i64,
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
        match do_refresh(db, &provider).await {
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
