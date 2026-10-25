//! The running requests of contacts (spec 056 FR-024, research R10): each runs under a token per
//! share, vault and the level it was allowed with. When a grant goes or gets lower,
//! [`crate::files::share::served`] cancels the tokens it no longer covers, so a read in progress
//! ends at once and an upload stops without a half file.

use std::collections::HashMap;
use std::sync::Mutex;

use tokio_util::sync::CancellationToken;

use crate::files::share::FileShareLevel;

/// What a contact's request was allowed as.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StreamKey {
    pub share: String,
    pub vault: [u8; 32],
    pub level: FileShareLevel,
}

/// The tokens of the running requests of contacts.
pub struct Streams {
    map: Mutex<HashMap<StreamKey, CancellationToken>>,
    own_device: Mutex<CancellationToken>,
}

impl Default for Streams {
    fn default() -> Self {
        Self {
            map: Mutex::default(),
            own_device: Mutex::new(CancellationToken::new()),
        }
    }
}

impl Streams {
    fn map(&self) -> std::sync::MutexGuard<'_, HashMap<StreamKey, CancellationToken>> {
        self.map.lock().unwrap_or_else(|error| error.into_inner())
    }

    /// The token requests allowed as `key` run under.
    pub fn token(&self, key: StreamKey) -> CancellationToken {
        self.map().entry(key).or_default().clone()
    }

    /// The token for requests from another own device.
    pub fn own_device_token(&self) -> CancellationToken {
        self.own_device
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    /// Ends every own-device request; later ones get a new token.
    pub fn cancel_own_device(&self) {
        let token = {
            let mut token = self
                .own_device
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            std::mem::replace(&mut *token, CancellationToken::new())
        };
        token.cancel();
    }

    /// Every key with running or past requests.
    pub fn keys(&self) -> Vec<StreamKey> {
        self.map().keys().cloned().collect()
    }

    /// Ends every request allowed as `key`; later ones get a new token.
    pub fn cancel(&self, key: &StreamKey) {
        if let Some(token) = self.map().remove(key) {
            token.cancel();
        }
    }
}

#[cfg(test)]
#[path = "streams_tests.rs"]
mod tests;
