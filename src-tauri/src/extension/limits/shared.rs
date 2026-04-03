use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};

/// Generic concurrent operation tracker per extension.
/// Used by database, filesystem, and web enforcers.
#[derive(Debug, Default)]
pub struct ConcurrencyTracker {
    counts: RwLock<HashMap<String, Arc<AtomicUsize>>>,
}

impl ConcurrencyTracker {
    pub fn new() -> Self {
        Self {
            counts: RwLock::new(HashMap::new()),
        }
    }

    /// Acquire a slot for an extension.
    /// Returns the current count AFTER incrementing.
    pub fn acquire(&self, extension_id: &str) -> usize {
        let counter = {
            let counts = self.counts.read().unwrap_or_else(|e| e.into_inner());
            counts.get(extension_id).cloned()
        };

        match counter {
            Some(counter) => counter.fetch_add(1, Ordering::SeqCst) + 1,
            None => {
                let mut counts = self.counts.write().unwrap_or_else(|e| e.into_inner());
                let counter = counts
                    .entry(extension_id.to_string())
                    .or_insert_with(|| Arc::new(AtomicUsize::new(0)));
                counter.fetch_add(1, Ordering::SeqCst) + 1
            }
        }
    }

    /// Release a slot for an extension.
    pub fn release(&self, extension_id: &str) {
        let counts = self.counts.read().unwrap_or_else(|e| e.into_inner());
        if let Some(counter) = counts.get(extension_id) {
            counter.fetch_sub(1, Ordering::SeqCst);
        }
    }

    /// Get current count for an extension.
    pub fn get_count(&self, extension_id: &str) -> usize {
        let counts = self.counts.read().unwrap_or_else(|e| e.into_inner());
        counts
            .get(extension_id)
            .map(|c| c.load(Ordering::SeqCst))
            .unwrap_or(0)
    }
}
