use ed25519_dalek::SigningKey;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

#[derive(Clone, Default)]
pub struct SpaceKeyCache {
    inner: Arc<RwLock<HashMap<String, SigningKey>>>,
}

impl SpaceKeyCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, space_id: &str) -> Option<SigningKey> {
        self.inner.read().ok()?.get(space_id).cloned()
    }

    pub fn insert(&self, space_id: &str, key: SigningKey) {
        if let Ok(mut w) = self.inner.write() {
            w.insert(space_id.to_string(), key);
        }
    }

    pub fn remove(&self, space_id: &str) {
        if let Ok(mut w) = self.inner.write() {
            w.remove(space_id);
        }
    }

    pub fn contains(&self, space_id: &str) -> bool {
        self.inner
            .read()
            .ok()
            .map(|r| r.contains_key(space_id))
            .unwrap_or(false)
    }
}
