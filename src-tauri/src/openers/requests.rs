//! The request a person handed to one extension, parked until that extension takes it (spec 063
//! data-model §3). Memory only: it ends with the process (FR-017), and an extension that does not
//! take it within [`TAKE_TIMEOUT`] loses it, so a request never waits for a frame that is not coming.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use serde_json::{json, Value};
use uuid::Uuid;

use super::mail::MailRequest;
use crate::extensions::bridge::dispatch::Emit;
use crate::extensions::bridge::events::emit_to_frames;
use crate::extensions::host::ExtensionHost;

/// How long a parked request waits for its extension.
pub const TAKE_TIMEOUT: Duration = Duration::from_secs(15);

/// Frame event telling an extension that a request waits for it.
pub const REQUEST_EVENT: &str = "opener-request";
/// Event to holzi's window: the request of `extensionId` was not taken in time.
pub const EXPIRED_EVENT: &str = "opener-request-expired";

#[derive(Default)]
pub struct OpenRequests {
    parked: Mutex<HashMap<Uuid, (u64, MailRequest)>>,
    tickets: AtomicU64,
}

impl OpenRequests {
    /// Parks `request` for `extension_id`, replacing an earlier one of that extension. The ticket
    /// names this request for [`OpenRequests::expire`].
    pub fn park(&self, extension_id: Uuid, request: MailRequest) -> u64 {
        let ticket = self.tickets.fetch_add(1, Ordering::Relaxed);
        self.lock().insert(extension_id, (ticket, request));
        ticket
    }

    /// The request that waits for `extension_id`, gone from here on: exactly one call gets it.
    pub fn take(&self, extension_id: Uuid) -> Option<MailRequest> {
        self.lock()
            .remove(&extension_id)
            .map(|(_, request)| request)
    }

    /// Drops the request `ticket` named, if it still waits. A newer request of the extension stays.
    pub fn expire(&self, extension_id: Uuid, ticket: u64) -> bool {
        let mut parked = self.lock();
        if parked.get(&extension_id).is_some_and(|(t, _)| *t == ticket) {
            parked.remove(&extension_id);
            return true;
        }
        false
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<Uuid, (u64, MailRequest)>> {
        self.parked.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Parks `request`, tells every open frame of the extension, and, if nobody takes it within
/// `timeout`, tells holzi's window that it expired.
pub fn park_and_announce(
    host: &Arc<ExtensionHost>,
    emitter: Arc<dyn Emit>,
    extension_id: Uuid,
    request: MailRequest,
    timeout: Duration,
) {
    let ticket = host.openers.park(extension_id, request);
    // The event carries nothing: a frame that asks gets its own request or none.
    emit_to_frames(&*emitter, host, extension_id, REQUEST_EVENT, &json!({}));
    let host = Arc::clone(host);
    std::thread::spawn(move || {
        std::thread::sleep(timeout);
        if host.openers.expire(extension_id, ticket) {
            emitter.emit(
                EXPIRED_EVENT,
                json!({ "extensionId": extension_id.to_string() }),
            );
        }
    });
}

/// What `extension_opener_take` answers for a parked request.
pub fn answer_of(request: &MailRequest) -> Value {
    json!({ "action": "mail.compose", "mail": request })
}

#[cfg(test)]
#[path = "requests_tests.rs"]
mod tests;
