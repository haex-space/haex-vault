//! Helpers for the singleton `local` provider rows — one per capability
//! (spec 008: chat and transcription are now two distinct `kind = local`
//! rows, disambiguated by `capability`).
//!
//! Downloads and imports need a `provider_id` for the `models` table.
//! The local runtime is a singleton per instance, so we auto-create the row on
//! first use rather than making the operator do it during onboarding.

use haex_crdt::{CrdtTransaction, Result};
use uuid::Uuid;

use crate::storage::providers::{
    delete_provider, find_provider_by_kind_and_capability, get_provider, insert_provider,
    move_provider_references, provider_ids_by_kind_and_capability, update_provider, Provider,
    ProviderCapability, ProviderKind,
};
use crate::storage::query::Query;

const LOCAL_PROVIDER_NAME: &str = "Local (llama.cpp)";
const LEGACY_LOCAL_PROVIDER_NAME: &str = "Local (mistral.rs)";
const LOCAL_TRANSCRIPTION_PROVIDER_NAME: &str = "Gebündelt (offline)";
const LOCAL_TRANSCRIPTION_ADAPTER: &str = "parakeet-local";
const LEGACY_LOCAL_TRANSCRIPTION_ADAPTER: &str = "whisper-local";

/// Fixed id of the local chat provider. Every installation uses the same
/// id, so the devices of a synced vault share one row instead of each
/// adding its own (spec 024).
pub fn local_provider_id() -> Uuid {
    Uuid::new_v5(&Uuid::NAMESPACE_URL, b"holzi:provider/local/chat")
}

/// Fixed id of the bundled local transcription provider; see
/// [`local_provider_id`].
pub fn local_transcription_provider_id() -> Uuid {
    Uuid::new_v5(&Uuid::NAMESPACE_URL, b"holzi:provider/local/transcription")
}

/// Returns the local chat provider's id, creating the row if missing.
/// Idempotent: safe to call from every download / import command.
pub fn ensure_local_provider(tx: &mut CrdtTransaction<'_>) -> Result<Uuid> {
    let id = ensure_fixed(
        tx,
        local_provider_id(),
        ProviderCapability::Chat,
        None,
        LOCAL_PROVIDER_NAME,
    )?;
    normalize_legacy_local_row(tx, id, Some(LEGACY_LOCAL_PROVIDER_NAME), None)
}

pub fn find_local_provider(q: &mut impl Query) -> Result<Option<Uuid>> {
    find_provider_by_kind_and_capability(q, ProviderKind::Local, ProviderCapability::Chat)
}

/// Returns the bundled local transcription provider's id, creating the row
/// if missing (spec 008 data-model.md §"Neue Runtime-/Storage-Entität").
/// Idempotent, singleton, mirrors [`ensure_local_provider`] — the only
/// differences are the capability and the `"parakeet-local"` adapter
/// discriminator that [`super::super::stt::sherpa::ParakeetAdapter`]
/// dispatch matches on.
pub fn ensure_local_transcription_provider(tx: &mut CrdtTransaction<'_>) -> Result<Uuid> {
    let id = ensure_fixed(
        tx,
        local_transcription_provider_id(),
        ProviderCapability::Transcription,
        Some(LOCAL_TRANSCRIPTION_ADAPTER),
        LOCAL_TRANSCRIPTION_PROVIDER_NAME,
    )?;
    normalize_legacy_local_row(tx, id, None, Some(LEGACY_LOCAL_TRANSCRIPTION_ADAPTER))
}

/// Rewrites the old engine names on the fixed local rows without replacing a name the user has
/// changed. Existing vaults may still carry labels and adapter ids from the removed runtimes.
fn normalize_legacy_local_row(
    tx: &mut CrdtTransaction<'_>,
    id: Uuid,
    legacy_name: Option<&str>,
    legacy_adapter: Option<&str>,
) -> Result<Uuid> {
    let Some(mut provider) = get_provider(tx, id)? else {
        return Ok(id);
    };
    let mut changed = false;
    if legacy_name.is_some_and(|legacy| provider.name == legacy) {
        provider.name = LOCAL_PROVIDER_NAME.to_owned();
        changed = true;
    }
    if legacy_adapter.is_some_and(|legacy| provider.adapter.as_deref() == Some(legacy)) {
        provider.adapter = Some(LOCAL_TRANSCRIPTION_ADAPTER.to_owned());
        changed = true;
    }
    if changed {
        update_provider(tx, &provider)?;
    }
    Ok(id)
}

pub fn find_local_transcription_provider(q: &mut impl Query) -> Result<Option<Uuid>> {
    find_provider_by_kind_and_capability(q, ProviderKind::Local, ProviderCapability::Transcription)
}

/// Makes `id` the only local row of `capability`. A row from before the
/// fixed ids (a random id, or several after a sync) moves to `id`: the
/// oldest row's values are kept, every reference is pointed at `id`, and the
/// old rows are deleted.
fn ensure_fixed(
    tx: &mut CrdtTransaction<'_>,
    id: Uuid,
    capability: ProviderCapability,
    adapter: Option<&str>,
    name: &str,
) -> Result<Uuid> {
    let existing = provider_ids_by_kind_and_capability(tx, ProviderKind::Local, capability)?;
    let replacement = match existing.first() {
        Some(&oldest) => get_provider(tx, oldest)?,
        None => None,
    };
    match replacement {
        Some(provider) if provider.id != id => {
            let provider = Provider { id, ..provider };
            if existing.contains(&id) {
                update_provider(tx, &provider)?;
            } else {
                insert_provider(tx, &provider)?;
            }
        }
        None if !existing.contains(&id) => {
            insert_provider(
                tx,
                &Provider {
                    id,
                    kind: ProviderKind::Local,
                    adapter: adapter.map(str::to_string),
                    name: name.to_string(),
                    base_url: None,
                    credentials: None,
                    created_at: now_ms(),
                    capability,
                },
            )?;
        }
        _ => {}
    }
    for old in existing.into_iter().filter(|&old| old != id) {
        move_provider_references(tx, old, id)?;
        delete_provider(tx, old)?;
    }
    Ok(id)
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
#[path = "local_tests.rs"]
mod tests;
