//! The model downloads that are running, so that one of them can be cancelled.
//!
//! A download registers under its model id and is handed a token; `cancel` fires the token and the
//! download stops at its next await point. Downloads hold the chat's operation slot, so at most one
//! runs at a time and the id is only a name for the settings view to cancel by.

use std::collections::HashMap;
use std::sync::Mutex;

use tokio_util::sync::CancellationToken;

#[derive(Default)]
pub struct DownloadRegistry {
    running: Mutex<HashMap<String, CancellationToken>>,
}

/// A registered download; dropping it removes the registration, however the download ends.
pub struct DownloadGuard<'a> {
    registry: &'a DownloadRegistry,
    model_id: String,
    token: CancellationToken,
}

impl DownloadRegistry {
    /// Registers the download of `model_id`.
    pub fn begin(&self, model_id: &str) -> DownloadGuard<'_> {
        let token = CancellationToken::new();
        self.lock().insert(model_id.to_owned(), token.clone());
        DownloadGuard {
            registry: self,
            model_id: model_id.to_owned(),
            token,
        }
    }

    /// Cancels the running download of `model_id`; `false` when none runs.
    pub fn cancel(&self, model_id: &str) -> bool {
        match self.lock().get(model_id) {
            Some(token) => {
                token.cancel();
                true
            }
            None => false,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, CancellationToken>> {
        self.running.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl DownloadGuard<'_> {
    pub fn token(&self) -> &CancellationToken {
        &self.token
    }
}

impl Drop for DownloadGuard<'_> {
    fn drop(&mut self) {
        self.registry.lock().remove(&self.model_id);
    }
}

#[cfg(test)]
#[path = "cancel_tests.rs"]
mod tests;
