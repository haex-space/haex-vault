//! Vault's device-UUID resolution, used at HLC init time.
//!
//! In haex-vault the persistent device UUID lives in the Tauri `instance.json`
//! store, NOT in `haex_crdt_configs`. `haex-crdt` removed its `DeviceIdProvider`
//! trait object in favour of `HlcService::initialize_in_place` taking an
//! already-resolved `Uuid` directly (and, separately, a transaction-scoped
//! `DatabaseBootstrap` hook for consumers using the crate's own
//! `Database::open` — not yet adopted here, that is Batch 5's scope). This
//! module keeps only the resolution logic vault's own `database::open` calls
//! directly; there is no trait to implement any more.

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use serde_json::json;
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;
use uuid::Uuid;

static DEVICE_ID_STORE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

/// Deterministic UUID derived from an arbitrary name string. Test-only.
///
/// Mirrors the (now removed / deprecated) `HlcService::new_for_testing`
/// shim: hashes the input with BLAKE3 and truncates to 16 bytes to seed a
/// [`Uuid`]. Same name always yields the same UUID, so callers can spin up
/// multiple `HlcService::new_with_uuid(...)` handles across tests and know
/// two tests using the same name produce identical HLC node ids.
///
/// NOT for production — production must resolve the device UUID from
/// persistent state (see [`get_or_create_device_id_from_store`]).
#[doc(hidden)]
pub fn test_device_uuid_from_name(name: &str) -> Uuid {
    let hash = blake3::hash(name.as_bytes());
    let bytes = hash.as_bytes();
    let mut uuid_bytes = [0u8; 16];
    uuid_bytes.copy_from_slice(&bytes[..16]);
    Uuid::from_bytes(uuid_bytes)
}

/// Reads (or lazily creates) the persistent device UUID stored in the Tauri
/// `instance.json` store under the `id` key. Extracted verbatim from the
/// pre-extraction `HlcService::get_or_create_device_id` so call sites that
/// need the raw String — outbound sync commands (`space_delivery::local::
/// commands::{peers,owner_sync}`) — and `database::open`'s HLC init both
/// resolve the same value.
pub fn get_or_create_device_id_from_store(app: &AppHandle) -> Result<String, String> {
    let _guard = DEVICE_ID_STORE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "device ID store lock poisoned".to_string())?;
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
