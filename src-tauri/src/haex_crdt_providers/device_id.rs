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

use std::path::PathBuf;
use std::sync::OnceLock;

use haex_crdt::error::{Error as CrdtError, Result as CrdtResult};
use haex_crdt::DeviceIdProvider;
use serde_json::json;
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;
use uuid::Uuid;

/// Deterministic UUID derived from an arbitrary name string. Test-only.
///
/// Mirrors the (now removed / deprecated) `HlcService::new_for_testing`
/// shim: hashes the input with BLAKE3 and truncates to 16 bytes to seed a
/// [`Uuid`]. Same name always yields the same UUID, so callers can spin up
/// multiple `HlcService::new_with_uuid(...)` handles across tests and know
/// two tests using the same name produce identical HLC node ids.
///
/// NOT for production — production must resolve the device UUID from
/// persistent state (see [`HaexVaultDeviceIdProvider::from_instance_store`]).
#[doc(hidden)]
pub fn test_device_uuid_from_name(name: &str) -> Uuid {
    let hash = blake3::hash(name.as_bytes());
    let bytes = hash.as_bytes();
    let mut uuid_bytes = [0u8; 16];
    uuid_bytes.copy_from_slice(&bytes[..16]);
    Uuid::from_bytes(uuid_bytes)
}

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

/// Reads (or lazily creates) the persistent device UUID stored in the Tauri
/// `instance.json` store under the `id` key. Extracted verbatim from the
/// pre-extraction `HlcService::get_or_create_device_id` so call sites that
/// need the raw String — outbound sync commands (`space_delivery::local::
/// commands::{peers,owner_sync}`) — keep working while Batch 5 threads the
/// [`DeviceIdProvider`] handle through `AppState`.
///
/// TODO(Batch 5): once every consumer takes a `&dyn DeviceIdProvider` this
/// helper collapses back into a private detail of the provider adapter.
pub fn get_or_create_device_id_from_store(app: &AppHandle) -> Result<String, String> {
    let store_path = PathBuf::from("instance.json");
    let store = app.store(store_path).map_err(|e| e.to_string())?;

    if let Some(value) = store.get("id") {
        if let Some(s) = value.as_str() {
            if Uuid::parse_str(s).is_ok() {
                return Ok(s.to_string());
            }
        }
        // Value exists but is not a valid UUID string — fall through to
        // regenerate, matching the pre-extraction behaviour.
    }

    let new_id = Uuid::new_v4().to_string();
    store.set("id".to_string(), json!(new_id.clone()));
    store.save().map_err(|e| e.to_string())?;
    Ok(new_id)
}

/// Constructor that reads (or creates) the vault's device UUID from the
/// Tauri `instance.json` store. Preserves the pre-extraction resolution
/// semantics of `HlcService::get_or_create_device_id` so `initialize_in_place`
/// at DB-open sees the same UUID that outbound sync commands do.
///
/// TODO(Batch 5): drop in favour of a single canonical resolver once
/// `AppState` owns a `&dyn DeviceIdProvider`.
impl HaexVaultDeviceIdProvider {
    pub fn from_instance_store(app: &AppHandle) -> Self {
        let app = app.clone();
        Self::new(move || {
            let id_str = get_or_create_device_id_from_store(&app)?;
            Uuid::parse_str(&id_str).map_err(|e| format!("instance.json id not a UUID: {e}"))
        })
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
