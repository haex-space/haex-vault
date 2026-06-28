//! iroh Endpoint management
//!
//! Manages the iroh QUIC endpoint: starting, stopping, accepting connections,
//! and handling incoming file requests. Access control ensures only peers
//! registered in the same Space can access shared folders.
//!
//! On Android, shared folders may use Content URIs (from the Storage Access
//! Framework). These are opaque URIs that require `tauri_plugin_android_fs` for
//! reading — standard `std::fs` calls do not work. The handlers detect Content
//! URI shares (JSON strings starting with `{`) and delegate to the android_fs
//! plugin via the `AppHandle` stored in `PeerState`.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::RwLock;

use iroh::{Endpoint, EndpointId, RelayUrl, SecretKey};

use ed25519_dalek::SigningKey;

use crate::space_delivery::local::dos_defence::config::DosDefenceConfig;
use crate::space_delivery::local::dos_defence::tracker::RejectRateTracker;

/// Sliding-window length used by the Phase 2 pre-auth accept tracker. The
/// configured rates are expressed per-second (`l1_global_rate_per_sec`,
/// `l1_per_source_rate_per_sec`), so a one-second window lets us read those
/// values directly as integer thresholds.
const ACCEPT_TRACKER_WINDOW: Duration = Duration::from_secs(1);

/// Synthetic key used for the global accept-rate bucket. The tracker is
/// keyed by `String`; production keys are remote endpoint ids, which are
/// always URL-safe base32 — no real endpoint id can collide with this.
pub(crate) const ACCEPT_TRACKER_GLOBAL_KEY: &str = "__global__";

pub(super) const DEFAULT_RELAY_URL: &str = "https://relay.sync.haex.space";

mod diagnostics;
mod lifecycle;
mod stream;

/// Identity material the endpoint uses to prove its DID on outbound
/// connections and to challenge inbound peers. Loaded once at start from
/// `haex_devices.owner_did` + `haex_identities.private_key`. Held in
/// `PeerEndpoint`, NOT in `PeerState` — the secret key never leaves the
/// endpoint struct.
#[derive(Clone)]
pub struct OwnIdentity {
    pub did: String,
    pub signing_key: SigningKey,
}

impl std::fmt::Debug for OwnIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OwnIdentity")
            .field("did", &self.did)
            .field("signing_key", &"<redacted>")
            .finish()
    }
}

// ============================================================================
// Delivery connection handler trait
// ============================================================================

/// Trait for handling space delivery connections. Implemented by space_delivery module.
pub trait DeliveryConnectionHandler: Send + Sync {
    fn handle_connection(
        &self,
        conn: iroh::endpoint::Connection,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>>;
}

// ============================================================================
// Shared state
// ============================================================================

/// A folder shared with peers
#[derive(Debug, Clone)]
pub struct SharedFolder {
    /// Display name
    pub name: String,
    /// Local filesystem path or Android Content URI (JSON string starting with `{`)
    pub local_path: String,
    /// Space this share belongs to (for access control)
    pub space_id: String,
}

/// Check if a path string is an Android Content URI (JSON-encoded)
pub fn is_content_uri(path: &str) -> bool {
    path.starts_with('{')
}

/// What kind of network path a QUIC connection is currently using.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PathType {
    /// Hole-punched/LAN — packets travel directly between the two endpoints.
    Direct,
    /// Relayed via a relay server — every packet round-trips through the relay.
    Relay,
    /// Connection exists but the path type is not classifiable.
    Unknown,
    /// Connection has already been closed.
    Closed,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionDiagnostics {
    pub path_type: PathType,
    pub remote_addr: Option<String>,
    pub rtt_ms: Option<f64>,
}

/// State shared between PeerEndpoint methods and the accept loop
pub struct PeerState {
    /// Shared folders (share_id -> folder)
    pub shares: HashMap<String, SharedFolder>,
    /// Access control: remote EndpointId (string) -> set of space_ids they may access
    pub allowed_peers: HashMap<String, HashSet<String>>,
    /// Tauri AppHandle for android_fs operations (set on Android before start)
    pub app_handle: Option<tauri::AppHandle>,
    /// Handler for incoming space delivery connections (set by space_delivery module)
    pub delivery_handler: Option<Arc<dyn DeliveryConnectionHandler>>,
    /// Verified DID per connected remote endpoint id, populated by the
    /// quic_did_auth handshake at connection-accept time. Used by request
    /// handlers to enforce UCAN audience match. Cleared when the connection
    /// closes.
    pub endpoint_dids: HashMap<String, String>,
    /// Expected owner DID per remote endpoint id, loaded from
    /// `haex_devices.owner_did` (which the `haex_space_devices_ensure_refs`
    /// trigger populates from UCAN-attributed CRDT rows). Cross-checked
    /// against the cryptographically verified DID from the quic_did_auth
    /// handshake — any mismatch is treated as a vault-internal inconsistency
    /// (database drift, partial sync, or worse) and the connection is closed.
    pub peer_owner_dids: HashMap<String, String>,
    /// Live connection-watcher count per remote endpoint. A peer can have
    /// several concurrent connections (inbound + outbound, or a stale one
    /// lingering across a reconnect); the watcher only emits a `Closed`
    /// diagnostic once the LAST one is torn down, so a transient connection
    /// dropping never flips a still-live peer offline in the UI.
    pub connection_watchers: HashMap<EndpointId, u32>,
    /// DoS-defence config for the pre-auth layers (L1 accept-rate, L2
    /// per-connection stream cap, L3 handshake timeout). Defaults to
    /// `DosDefenceConfig::defaults()` and is replaced via
    /// `PeerEndpoint::set_dos_config` once the vault is open and the
    /// `haex_vault_settings` rows can be read. Wrapping in `Arc` keeps the
    /// accept loop's hot path cheap — one pointer clone per accepted
    /// connection, no struct copy.
    pub dos_config: Arc<DosDefenceConfig>,
    /// Sliding-window accept-rate tracker for Phase 2 L1 enforcement. Keys:
    /// per-source remote endpoint id strings + the `ACCEPT_TRACKER_GLOBAL_KEY`
    /// bucket. Separate from the L4 `LeaderState::reject_tracker` because:
    /// (a) endpoint lifetime ≠ leader lifetime — peer_storage starts before
    /// any leader exists; (b) L1 keys are pre-auth source ids, L4 keys are
    /// post-auth DIDs; merging them would require key namespacing and an
    /// `Arc<RwLock<_>>` cross-module access path.
    pub accept_tracker: Arc<RejectRateTracker>,
    /// Phase 3 runtime: FloodMode state machine + contacts resolver +
    /// DDoS-episode notifier. `None` until the vault-open path wires it via
    /// `PeerEndpoint::set_dos_runtime`; while `None`, accept_loop behaves
    /// exactly as before (Phase 2 semantics only — no DDoS-mode check, no
    /// auto-escalation).
    pub dos_runtime:
        Option<Arc<crate::space_delivery::local::dos_defence::state::DosDefenceRuntime>>,
}

impl Default for PeerState {
    fn default() -> Self {
        Self {
            shares: HashMap::new(),
            allowed_peers: HashMap::new(),
            app_handle: None,
            delivery_handler: None,
            endpoint_dids: HashMap::new(),
            peer_owner_dids: HashMap::new(),
            connection_watchers: HashMap::new(),
            dos_config: Arc::new(DosDefenceConfig::defaults()),
            accept_tracker: Arc::new(RejectRateTracker::new(ACCEPT_TRACKER_WINDOW)),
            dos_runtime: None,
        }
    }
}

// Manual Debug impl because tauri::AppHandle doesn't implement Debug
impl std::fmt::Debug for PeerState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PeerState")
            .field("shares", &self.shares)
            .field("allowed_peers", &self.allowed_peers)
            .field(
                "app_handle",
                &self.app_handle.as_ref().map(|_| "Some(AppHandle)"),
            )
            .field(
                "delivery_handler",
                &self
                    .delivery_handler
                    .as_ref()
                    .map(|_| "Some(DeliveryConnectionHandler)"),
            )
            .finish()
    }
}

/// Peer storage endpoint state
pub struct PeerEndpoint {
    /// The iroh endpoint (None if not running)
    pub(super) endpoint: Option<Endpoint>,
    /// Secret key for this node
    pub(super) secret_key: SecretKey,
    /// Shared state (accessible by both endpoint methods and accept loop)
    pub(crate) state: Arc<RwLock<PeerState>>,
    /// Handle to the accept loop task
    pub(super) accept_task: Option<tokio::task::JoinHandle<()>>,
    /// Handle to the endpoint-closed watcher task; aborted on user-initiated stop
    /// so it does not emit a spurious "endpoint-closed" event that would trigger
    /// the TS auto-restart handler.
    pub(super) watcher_task: Option<tokio::task::JoinHandle<()>>,
    /// Configured relay URL (set at start, available even before relay connection is established)
    pub(super) configured_relay_url: Option<RelayUrl>,
    /// Cached connections to remote peers. Reusing a single QUIC connection for
    /// multiple streams avoids per-request TLS handshakes and the race condition
    /// where a closing connection interferes with a subsequent connect() call.
    ///
    /// **Cached connections are always already-authenticated**: the
    /// `quic_did_auth` handshake runs once on the first opened/accepted
    /// bi-stream of a fresh connection. Subsequent stream opens on the same
    /// connection skip the handshake.
    pub(super) connections: Mutex<HashMap<EndpointId, iroh::endpoint::Connection>>,
    /// Identity used to prove our DID to remote peers (outbound) and to
    /// challenge inbound peers. Set via `set_own_identity` before `start`.
    /// `None` until set — used only by the quic_did_auth handshake; other
    /// peer_storage paths do not depend on it.
    ///
    /// Held in an `Arc<Mutex<_>>` so the accept loop and concurrent `open_stream`
    /// calls can read it without going through the outer endpoint reference.
    pub(super) own_identity: Arc<Mutex<Option<OwnIdentity>>>,
}

impl PeerEndpoint {
    /// Create a new PeerEndpoint with a persistent device key.
    pub fn new(secret_key: SecretKey) -> Self {
        Self {
            endpoint: None,
            secret_key,
            state: Arc::new(RwLock::new(PeerState::default())),
            accept_task: None,
            watcher_task: None,
            configured_relay_url: None,
            connections: Mutex::new(HashMap::new()),
            own_identity: Arc::new(Mutex::new(None)),
        }
    }

    /// Set the DID + signing key used by the quic_did_auth handshake.
    ///
    /// Must be called before `start` — swapping the identity while the
    /// endpoint is running would leave cached connections authenticated
    /// against the *old* DID while new connections authenticate against the
    /// new one, producing a single endpoint that effectively serves two
    /// mixed identities. Panics in that case (programmer error, not a
    /// recoverable runtime condition).
    pub fn set_own_identity(&self, identity: OwnIdentity) {
        assert!(
            self.endpoint.is_none(),
            "set_own_identity must be called before start (endpoint is already running)"
        );
        if let Ok(mut slot) = self.own_identity.lock() {
            *slot = Some(identity);
        }
    }

    /// Clone the configured identity, if any. Used by the accept loop and by
    /// `open_stream` to drive the handshake.
    pub(super) fn own_identity(&self) -> Option<OwnIdentity> {
        self.own_identity.lock().ok().and_then(|g| g.clone())
    }

    /// Create a PeerEndpoint with a temporary random key (for testing or pre-init state).
    pub fn new_ephemeral() -> Self {
        let bytes: [u8; 32] = rand::random();
        Self::new(SecretKey::from_bytes(&bytes))
    }

    /// Replace the secret key with a persistent device key.
    /// Must be called before starting the endpoint. Panics if endpoint is running.
    pub fn replace_key(&mut self, secret_key: SecretKey) {
        assert!(
            self.endpoint.is_none(),
            "Cannot replace key while endpoint is running"
        );
        self.secret_key = secret_key;
    }

    /// Store the Tauri AppHandle for android_fs operations.
    /// Must be called before start() on Android so Content URI shares can be served.
    pub async fn set_app_handle(&self, app_handle: tauri::AppHandle) {
        self.state.write().await.app_handle = Some(app_handle);
    }

    /// Register a handler for space delivery connections.
    pub async fn set_delivery_handler(&self, handler: Arc<dyn DeliveryConnectionHandler>) {
        self.state.write().await.delivery_handler = Some(handler);
    }

    /// Get the public EndpointId
    pub fn endpoint_id(&self) -> EndpointId {
        self.secret_key.public()
    }

    /// Check if the endpoint is running
    pub fn is_running(&self) -> bool {
        self.endpoint.is_some()
    }

    /// Get the configured relay URL (available even before relay connection is established)
    pub fn configured_relay_url(&self) -> Option<&RelayUrl> {
        self.configured_relay_url.as_ref()
    }

    /// Add a shared folder
    pub async fn add_share(&self, id: String, name: String, local_path: String, space_id: String) {
        eprintln!("[PeerStorage] Added share '{name}' at {local_path} (space: {space_id})");
        self.state.write().await.shares.insert(
            id,
            SharedFolder {
                name,
                local_path,
                space_id,
            },
        );
    }

    /// Remove a shared folder
    pub async fn remove_share(&self, id: &str) -> bool {
        self.state.write().await.shares.remove(id).is_some()
    }

    /// List shared folders
    pub async fn list_shares(&self) -> Vec<(String, SharedFolder)> {
        self.state
            .read()
            .await
            .shares
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    /// Clear all shares (used before reloading from DB)
    pub async fn clear_shares(&self) {
        self.state.write().await.shares.clear();
    }

    /// Get a reference to the underlying iroh endpoint
    pub fn endpoint_ref(&self) -> Option<&Endpoint> {
        self.endpoint.as_ref()
    }

    /// Update the allowed peers map (remote EndpointId -> set of space_ids)
    pub async fn set_allowed_peers(&self, allowed: HashMap<String, HashSet<String>>) {
        eprintln!(
            "[PeerStorage] Updated allowed peers: {} peers across spaces",
            allowed.len()
        );
        self.state.write().await.allowed_peers = allowed;
    }

    /// Update the expected `(endpoint_id -> owner_did)` map used as a
    /// defense-in-depth cross-check against the cryptographically verified
    /// DID from the handshake. Loaded from `haex_devices.owner_did` for
    /// every endpoint we expect to see — keep this in sync with
    /// `allowed_peers`, since a peer that passes `allowed_peers` but has no
    /// entry here will be rejected by `handle_connection`.
    pub async fn set_peer_owner_dids(&self, dids: HashMap<String, String>) {
        eprintln!(
            "[PeerStorage] Updated peer owner DIDs: {} entries",
            dids.len()
        );
        self.state.write().await.peer_owner_dids = dids;
    }
}
