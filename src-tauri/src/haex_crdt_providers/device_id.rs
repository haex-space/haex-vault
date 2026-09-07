//! `haex-crdt` [`DeviceIdProvider`] adapter.
//!
//! In haex-vault the persistent device UUID lives on the filesystem at
//! `<app_data>/device_id` (see `crate::device`), NOT in
//! `haex_crdt_configs`. The provider is a thin cache in front of that
//! resolver: the closure is invoked at most once and the resulting `Uuid`
//! is memoised in an internal `OnceLock` so `device_id()` is safe to call
//! from any code path without re-reading the file.
//!
//! Two constructors:
//!
//! - [`HaexVaultDeviceIdProvider::new`] takes an arbitrary resolver — used
//!   in Batch 5 by the production wire-up (e.g. reading
//!   `<app_data>/device_id` via `tauri::AppHandle`). Kept generic here so
//!   the module has no `tauri::AppHandle` dependency on the sign hot path.
//! - [`HaexVaultDeviceIdProvider::from_state_test_seed`] is a `#[cfg(test)]`
//!   constructor that seeds a deterministic UUID for round-trip tests
//!   without touching the filesystem.

use std::sync::OnceLock;

use haex_crdt::error::{Error as CrdtError, Result as CrdtResult};
use haex_crdt::DeviceIdProvider;
use uuid::Uuid;

/// Resolver closure supplied by the caller. Under contended first-call the
/// resolver can run more than once (each racing thread resolves before the
/// `OnceLock` slot is filled; the losing thread's value is discarded, the
/// trait's stable-UUID contract still holds). After the first successful
/// resolve, subsequent [`DeviceIdProvider::device_id`] calls hit the cache
/// and do no I/O. Callers whose resolver has non-trivial I/O cost should
/// serialise the first call themselves (e.g. by resolving once at startup
/// before handing the provider to `Arc<dyn DeviceIdProvider>`).
type Resolver = Box<dyn Fn() -> Result<Uuid, String> + Send + Sync>;

pub struct HaexVaultDeviceIdProvider {
    cached: OnceLock<Uuid>,
    resolver: Resolver,
}

impl HaexVaultDeviceIdProvider {
    /// General constructor. See the [`Resolver`] doc for the first-call race
    /// semantics. Wire this to `<app_data>/device_id` in Batch 5's `AppState`
    /// construction.
    pub fn new<F>(resolver: F) -> Self
    where
        F: Fn() -> Result<Uuid, String> + Send + Sync + 'static,
    {
        Self {
            cached: OnceLock::new(),
            resolver: Box::new(resolver),
        }
    }

    /// Test-only constructor. Derives a deterministic `Uuid` from the first
    /// 16 bytes of the seed so tests can assert exact values without
    /// touching the filesystem or the Tauri handle.
    #[cfg(test)]
    pub fn from_state_test_seed(seed: &[u8; 32]) -> Self {
        let mut bytes = [0u8; 16];
        bytes.copy_from_slice(&seed[..16]);
        let uuid = Uuid::from_bytes(bytes);
        Self::new(move || Ok(uuid))
    }
}

impl DeviceIdProvider for HaexVaultDeviceIdProvider {
    fn device_id(&self) -> CrdtResult<Uuid> {
        if let Some(id) = self.cached.get() {
            return Ok(*id);
        }
        let resolved = (self.resolver)()
            .map_err(|e| CrdtError::Message(format!("device_id resolver: {e}")))?;
        // `get_or_init` won't call its closure if another thread has already
        // filled the slot — the trait contract requires the same UUID across
        // calls, so throwing away our freshly-resolved value in that race is
        // fine.
        let stored = self.cached.get_or_init(|| resolved);
        Ok(*stored)
    }
}
