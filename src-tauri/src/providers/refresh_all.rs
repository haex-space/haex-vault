//! The models of every provider are read again each time a vault is opened (spec 055 FR-037): a
//! model a provider offers since the user connected it (a new Claude model, a model pulled on a
//! server) shows without anyone pressing "refresh". It runs as tracked session work in the
//! background; a provider that does not answer keeps its list and never blocks opening.

use super::{do_refresh, storage};
use crate::state::AppState;
use crate::storage::providers::{ProviderCapability, ProviderKind};
use crate::vault_gate::VaultDb;

/// Reads the models of every chat provider but the local ones, one after the other; returns how
/// many providers answered.
pub(crate) async fn refresh_all(db: &VaultDb) -> usize {
    let providers = match db.read(|r| storage::list_providers(r)).await {
        Ok(providers) => providers,
        Err(error) => {
            log::warn!("providers: the providers could not be read: {error}");
            return 0;
        }
    };
    let mut answered = 0;
    for provider in providers
        .into_iter()
        .filter(|p| p.kind != ProviderKind::Local && p.capability == ProviderCapability::Chat)
    {
        // Never the error itself: a provider's answer can quote what was asked.
        match do_refresh(db, &provider).await {
            Ok(_) => answered += 1,
            Err(_) => log::warn!(
                "providers: the models of provider {} could not be refreshed",
                provider.id
            ),
        }
    }
    answered
}

/// Starts reading the models of every provider of the open vault in the background.
pub fn start_after_open(state: &AppState) {
    let Ok(db) = state.database() else {
        return;
    };
    let started = state.gate().spawn(async move {
        refresh_all(&db).await;
    });
    if let Err(error) = started {
        log::warn!("providers: the models could not be refreshed: {error}");
    }
}
