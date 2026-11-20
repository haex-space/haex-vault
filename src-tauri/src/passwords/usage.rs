//! Which haex-vault functions use an entry (spec 034, FR-034, `contracts/access.md`): a function such as
//! the own S3 storage (spec 029) registers a provider at its start, and the window asks before an
//! entry is deleted, so the user is warned that a function depends on it (the delete is still
//! allowed). The password manager recognizes this by nothing but what the functions report: not by
//! tags and not by names. No provider is registered in this spec; spec 029 registers the first.

use std::sync::{Arc, Mutex};

/// A haex-vault function that keeps references to entries and can say whether it uses one.
pub trait EntryUsage: Send + Sync {
    /// The name the warning shows, for example `s3-storage`.
    fn feature(&self) -> &'static str;
    /// Whether the function uses this entry. Cheap: it reads the function's own data.
    fn uses(&self, item_id: &str) -> bool;
}

/// The providers in registration order.
#[derive(Default)]
pub struct UsageRegistry {
    providers: Mutex<Vec<Arc<dyn EntryUsage>>>,
}

impl UsageRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a provider; one with the same feature name replaces the earlier one in its place.
    pub fn register(&self, provider: Arc<dyn EntryUsage>) {
        let Ok(mut providers) = self.providers.lock() else {
            return;
        };
        match providers
            .iter()
            .position(|existing| existing.feature() == provider.feature())
        {
            Some(at) => providers[at] = provider,
            None => providers.push(provider),
        }
    }

    /// The names of the functions that use the entry, in registration order.
    pub fn used_by(&self, item_id: &str) -> Vec<&'static str> {
        let providers: Vec<Arc<dyn EntryUsage>> = self
            .providers
            .lock()
            .map(|providers| providers.clone())
            .unwrap_or_default();
        // The lock is not held while the providers answer, so a provider may use the manager.
        providers
            .iter()
            .filter(|provider| provider.uses(item_id))
            .map(|provider| provider.feature())
            .collect()
    }
}
