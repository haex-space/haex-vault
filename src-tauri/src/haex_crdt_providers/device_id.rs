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

use std::fs::{File, OpenOptions};
use std::sync::{Mutex, OnceLock};

use fs2::FileExt;
use serde_json::json;
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;
use uuid::Uuid;

const INSTANCE_STORE_PATH: &str = "instance.json";
const INSTANCE_STORE_LOCK_SUFFIX: &str = ".lock";

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
    // The in-process mutex above does not coordinate separate vault
    // processes. Hold an OS-level lock on a sibling file for the complete
    // read/create/save sequence so two processes cannot mint different IDs.
    let _process_lock = lock_instance_store(app)?;
    let store = app.store(INSTANCE_STORE_PATH).map_err(|e| e.to_string())?;

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

/// Acquire an inter-process lock matching the path used by the store plugin.
///
/// The lock file is intentionally kept after the handle is dropped. Removing
/// it would introduce a TOCTOU race where another process could open a new
/// inode between unlock and recreate, bypassing the shared lock.
fn lock_instance_store(app: &AppHandle) -> Result<File, String> {
    let store_path = tauri_plugin_store::resolve_store_path(app, INSTANCE_STORE_PATH)
        .map_err(|e| e.to_string())?;
    let parent = store_path
        .parent()
        .ok_or_else(|| "instance store path has no parent directory".to_string())?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;

    let mut lock_path = store_path;
    let file_name = lock_path
        .file_name()
        .ok_or_else(|| "instance store path has no file name".to_string())?;
    let mut lock_file_name = file_name.to_os_string();
    lock_file_name.push(INSTANCE_STORE_LOCK_SUFFIX);
    lock_path.set_file_name(lock_file_name);

    let lock_file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(lock_path)
        .map_err(|e| e.to_string())?;
    lock_file.lock_exclusive().map_err(|e| e.to_string())?;
    Ok(lock_file)
}
