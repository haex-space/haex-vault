//! Endpoint lifecycle — start/stop, accept loop, and per-connection auth handshake.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::sync::RwLock;

use iroh::{
    endpoint::{QuicTransportConfig, VarInt},
    Endpoint, EndpointId, RelayMode, RelayUrl,
};
use iroh_mdns_address_lookup::MdnsAddressLookup;

use tauri::Emitter;

use crate::peer_storage::endpoint::diagnostics::spawn_connection_watcher;
use crate::peer_storage::endpoint::{
    OwnIdentity, PeerEndpoint, PeerState, ACCEPT_TRACKER_GLOBAL_KEY, DEFAULT_RELAY_URL,
};
use crate::peer_storage::error::PeerStorageError;
use crate::peer_storage::protocol::ALPN;
use crate::space_delivery::local::dos_defence::config::{DosDefenceConfig, EscalationPolicy};
use crate::space_delivery::local::dos_defence::flood_mode::{FloodMode, FloodThresholds};
use crate::space_delivery::local::dos_defence::state::DosDefenceRuntime;
use crate::space_delivery::local::dos_defence::tracker::RejectRateTracker;

#[cfg(test)]
use ed25519_dalek::SigningKey;
#[cfg(test)]
use iroh::EndpointAddr;

impl PeerEndpoint {
    /// Start the iroh endpoint and begin accepting connections.
    /// `relay_url` — optional relay URL from vault settings; falls back to
    /// `HAEX_RELAY_URL` env var, then iroh's default relay servers.
    pub async fn start(
        &mut self,
        relay_url: Option<String>,
    ) -> Result<EndpointId, PeerStorageError> {
        if self.endpoint.is_some() {
            return Err(PeerStorageError::EndpointAlreadyRunning);
        }

        let effective_relay = relay_url
            .filter(|s| !s.is_empty())
            .or_else(|| {
                std::env::var("HAEX_RELAY_URL")
                    .ok()
                    .filter(|s| !s.is_empty())
            })
            .unwrap_or_else(|| DEFAULT_RELAY_URL.to_string());

        let relay_mode = match effective_relay.parse::<RelayUrl>() {
            Ok(parsed) => {
                eprintln!("[PeerStorage] Using relay: {effective_relay}");
                self.configured_relay_url = Some(parsed.clone());
                RelayMode::custom([parsed])
            }
            Err(e) => {
                eprintln!(
                    "[PeerStorage] Invalid relay URL '{effective_relay}': {e} \
                     — falling back to iroh default"
                );
                RelayMode::Default
            }
        };

        // Tune QUIC transport for LAN/WAN bulk transfers. iroh's noq-default
        // sizes its windows for 100 Mbps × 100 ms = 1.25 MB per stream, which
        // becomes the bottleneck for large files: the receiver's stream-level
        // flow control fills up while disk writes are in flight, the sender
        // stalls waiting for window updates, and per-stream throughput pegs
        // around 1 MB/s regardless of link capacity. Sizing the window for
        // ~1 Gbps × 50 ms (worst-case LAN+jitter) gives room for ~6 MB in
        // flight per stream without ballooning RAM (worst-case usage =
        // max_concurrent_bidi_streams × stream_receive_window).
        let transport_config = QuicTransportConfig::builder()
            .stream_receive_window(VarInt::from_u32(16 * 1024 * 1024))
            .send_window(64 * 1024 * 1024)
            .max_concurrent_bidi_streams(VarInt::from_u32(256))
            .build();

        // iroh's `.bind()` can hang indefinitely if relay-URL DNS lookup
        // stalls or socket binding loops on transient OS errors. Cap at 15s
        // so a hung start surfaces as a fast error and the caller can retry,
        // instead of wedging the whole frontend (observed in CI as 30s+
        // playwright timeouts on `peer_storage_start`).
        let bind_future = Endpoint::builder(iroh::endpoint::presets::N0)
            .secret_key(self.secret_key.clone())
            .alpns(vec![
                ALPN.to_vec(),
                crate::space_delivery::local::protocol::ALPN.to_vec(),
            ])
            .relay_mode(relay_mode)
            .address_lookup(MdnsAddressLookup::builder().service_name("haex-peer"))
            .transport_config(transport_config)
            .bind();
        let endpoint = tokio::time::timeout(std::time::Duration::from_secs(15), bind_future)
            .await
            .map_err(|_| PeerStorageError::ConnectionFailed {
                reason: "Endpoint bind timed out after 15s".to_string(),
            })?
            .map_err(|e| PeerStorageError::ConnectionFailed {
                reason: format!("Failed to bind endpoint: {e}"),
            })?;

        let id = endpoint.id();
        eprintln!("[PeerStorage] Endpoint started with ID: {id}");

        // Spawn accept loop with shared state and (a handle to) the own-
        // identity slot so the quic_did_auth handshake can find it. Sharing
        // the Arc<Mutex<_>> (rather than a snapshot) means a late
        // `set_own_identity` after `start` still reaches the accept loop.
        let ep = endpoint.clone();
        let state = self.state.clone();
        let own_identity = self.own_identity.clone();

        let accept_task = tokio::spawn(async move {
            accept_loop(ep, state, own_identity).await;
        });

        // Endpoint death watcher (diag/multi-leader-quic-logging branch).
        // The iroh endpoint can become "closed" without us calling stop()
        // — typically when an internal task (relay actor, socket
        // transport) gives up after unrecoverable errors. The sync loop
        // then spins forever in exponential-backoff reconnect with
        // "Endpoint is closed", and we currently have zero visibility
        // into the why or the when.
        //
        // This watcher resolves when iroh signals the endpoint as
        // closed, and writes one entry to haex_logs with the elapsed
        // uptime. Correlate the timestamp with the stderr stream
        // around it (sync_loop, multi_leader, relay) to spot the
        // trigger event.
        let watch_endpoint = endpoint.clone();
        let watch_state = self.state.clone();
        let started_at = std::time::Instant::now();
        let endpoint_id_short = id.fmt_short();
        let watcher_task = tokio::spawn(async move {
            watch_endpoint.closed().await;
            let uptime = started_at.elapsed();
            let msg = format!(
                "iroh endpoint reported closed after {}s {}ms uptime (id={})",
                uptime.as_secs(),
                uptime.subsec_millis(),
                endpoint_id_short,
            );
            eprintln!("[Endpoint] {msg}");
            let app_handle = watch_state.read().await.app_handle.clone();
            if let Some(app) = app_handle {
                if let Some(state) = <tauri::AppHandle as tauri::Manager<tauri::Wry>>::try_state::<
                    crate::AppState,
                >(&app)
                {
                    let _ = crate::logging::insert_log(
                        &state, "error", "Endpoint", None, &msg, None, "rust",
                    );
                }
                let _ = app.emit_to(
                    "main",
                    crate::event_names::EVENT_PEER_STORAGE_STATE_CHANGED,
                    serde_json::json!({
                        "running": false,
                        "reason": "endpoint-closed",
                        "uptimeSecs": uptime.as_secs(),
                    }),
                );
            }
        });

        self.endpoint = Some(endpoint);
        self.accept_task = Some(accept_task);
        self.watcher_task = Some(watcher_task);

        Ok(id)
    }

    /// Stop the endpoint
    pub async fn stop(&mut self) -> Result<(), PeerStorageError> {
        if let Ok(mut cache) = self.connections.lock() {
            cache.clear();
        }

        // Abort the watcher before closing so it cannot emit a spurious
        // "endpoint-closed" event that would trigger the TS auto-restart handler.
        if let Some(task) = self.watcher_task.take() {
            task.abort();
        }

        if let Some(task) = self.accept_task.take() {
            task.abort();
        }

        if let Some(endpoint) = self.endpoint.take() {
            // iroh's graceful close waits for peers to ACK QUIC CLOSE frames.
            // With default RTT estimates this can block up to ~30s when a peer
            // is unreachable (network switch, peer offline) — long enough for
            // the user-facing logout/lock to feel hung. Bound it: healthy
            // peers complete in well under a second, dead ones fall through.
            let close_timeout = std::time::Duration::from_secs(2);
            match tokio::time::timeout(close_timeout, endpoint.close()).await {
                Ok(()) => eprintln!("[PeerStorage] Endpoint stopped"),
                Err(_) => eprintln!(
                    "[PeerStorage] Endpoint close exceeded {}s, peer ACKs abandoned",
                    close_timeout.as_secs()
                ),
            }
        }

        Ok(())
    }

    /// Local-only endpoint start for unit tests. Binds with `RelayMode::Disabled`
    /// and no address-lookup service, so the test does not depend on DNS or relay
    /// servers. Spawns the accept loop; omits the production endpoint-closed
    /// watcher (which depends on a Tauri AppHandle).
    #[cfg(test)]
    pub(crate) async fn start_for_test(&mut self) -> Result<EndpointId, PeerStorageError> {
        if self.endpoint.is_some() {
            return Err(PeerStorageError::EndpointAlreadyRunning);
        }

        let endpoint = Endpoint::builder(iroh::endpoint::presets::Minimal)
            .secret_key(self.secret_key.clone())
            .alpns(vec![ALPN.to_vec()])
            .relay_mode(RelayMode::Disabled)
            .bind()
            .await
            .map_err(|e| PeerStorageError::ConnectionFailed {
                reason: format!("Failed to bind test endpoint: {e}"),
            })?;

        let id = endpoint.id();

        let ep = endpoint.clone();
        let state = self.state.clone();
        let own_identity = self.own_identity.clone();
        let accept_task = tokio::spawn(async move {
            accept_loop(ep, state, own_identity).await;
        });

        self.endpoint = Some(endpoint);
        self.accept_task = Some(accept_task);

        Ok(id)
    }

    /// Generate a fresh ed25519 keypair and install it as the endpoint's
    /// own identity. Returns the generated DID so tests can mint UCANs whose
    /// audience matches the verified peer DID checked in handle_stream.
    #[cfg(test)]
    pub(crate) fn set_random_test_identity(&self) -> String {
        let seed: [u8; 32] = rand::random();
        let signing_key = SigningKey::from_bytes(&seed);
        let mut did_bytes = Vec::with_capacity(34);
        did_bytes.extend_from_slice(&[0xed, 0x01]);
        did_bytes.extend_from_slice(signing_key.verifying_key().as_bytes());
        let did = format!("did:key:z{}", bs58::encode(did_bytes).into_string());
        self.set_own_identity(OwnIdentity {
            did: did.clone(),
            signing_key,
        });
        did
    }

    /// Pre-populate the connection cache with a direct-address QUIC connection
    /// to `remote_addr`. After this returns, `open_stream(remote_id, None)` will
    /// reuse the cached connection. Used by tests to bypass the relay /
    /// address-lookup path that production `open_stream` relies on, since unit
    /// tests run with `RelayMode::Disabled` and no DNS publishing.
    ///
    /// Runs the quic_did_auth handshake on the first opened bi-stream so the
    /// cached connection is fully authenticated (matching production
    /// `open_stream`). Callers must have called `set_random_test_identity` (or
    /// `set_own_identity`) on the client side, and the server-side endpoint
    /// must also have a configured identity for the handshake to complete.
    #[cfg(test)]
    pub(crate) async fn connect_for_test(
        &self,
        remote_addr: EndpointAddr,
    ) -> Result<(), PeerStorageError> {
        let endpoint = self
            .endpoint
            .as_ref()
            .ok_or(PeerStorageError::EndpointNotRunning)?;
        let remote_id = remote_addr.id;
        let conn = endpoint.connect(remote_addr, ALPN).await.map_err(|e| {
            PeerStorageError::ConnectionFailed {
                reason: format!("connect_for_test: {e}"),
            }
        })?;

        // Server-initiated auth bi-stream (see open_stream for protocol
        // reasoning). Client awaits on accept_bi, then responds.
        let identity = self
            .own_identity()
            .ok_or_else(|| PeerStorageError::ConnectionFailed {
                reason: "connect_for_test: own identity not configured".into(),
            })?;
        let own_endpoint_id_str = endpoint.id().to_string();
        let (mut auth_send, mut auth_recv) =
            conn.accept_bi()
                .await
                .map_err(|e| PeerStorageError::ConnectionFailed {
                    reason: format!("connect_for_test accept auth stream: {e}"),
                })?;
        crate::quic_did_auth::respond_to_challenge(
            &mut auth_send,
            &mut auth_recv,
            &identity.did,
            &identity.signing_key,
            &own_endpoint_id_str,
        )
        .await
        .map_err(|e| PeerStorageError::ConnectionFailed {
            reason: format!("connect_for_test DID-auth: {e}"),
        })?;
        let _ = auth_send.finish();

        if let Ok(mut cache) = self.connections.lock() {
            cache.insert(remote_id, conn.clone());
        }
        spawn_connection_watcher(remote_id, conn, self.state.clone());
        Ok(())
    }
}

// ============================================================================
// Accept loop — handles incoming connections with access control
// ============================================================================

async fn accept_loop(
    endpoint: Endpoint,
    state: Arc<RwLock<PeerState>>,
    own_identity: Arc<Mutex<Option<OwnIdentity>>>,
) {
    while let Some(incoming) = endpoint.accept().await {
        let state = state.clone();
        let own_identity = own_identity.clone();
        let own_endpoint_id = endpoint.id().to_string();
        tokio::spawn(async move {
            match incoming.await {
                Ok(conn) => {
                    let alpn = conn.alpn();
                    let alpn_bytes: &[u8] = &alpn;
                    let remote = conn.remote_id();

                    // -- L1 Pre-Auth Rate-Limit --
                    //
                    // Drop connections that arrive faster than the configured
                    // per-source or global accept rate. Silent-drop (no log
                    // row, no notification) — the L4 layer owns user-visible
                    // single-source detection once a peer has authenticated;
                    // L1 just keeps server state bounded against pre-auth
                    // floods. See docs/plans/2026-06-13-leader-reject-rate-limit.md §L1.
                    if !pre_auth_accept_check(&state, &remote.to_string()).await {
                        conn.close(8u32.into(), b"rate limited");
                        return;
                    }

                    if alpn_bytes == ALPN {
                        // --- Peer storage protocol ---
                        let remote_str = remote.to_string();

                        let allowed_spaces = {
                            let s = state.read().await;
                            s.allowed_peers.get(&remote_str).cloned()
                        };

                        match allowed_spaces {
                            Some(spaces) if !spaces.is_empty() => {
                                eprintln!(
                                    "[PeerStorage] Accepted connection from {remote} \
                                     (access to {} spaces)",
                                    spaces.len()
                                );
                                // Watch inbound connections too — when a remote
                                // peer reaches out, the UI's online dot should
                                // flip without waiting for an outbound retry.
                                spawn_connection_watcher(remote, conn.clone(), state.clone());
                                handle_connection(conn, state, own_identity, own_endpoint_id).await;
                            }
                            _ => {
                                eprintln!(
                                    "[PeerStorage] Rejected connection from {remote}: \
                                     not registered in any shared space"
                                );
                            }
                        }
                    } else if alpn_bytes == crate::space_delivery::local::protocol::ALPN {
                        // --- Space delivery protocol ---
                        let handler = {
                            let s = state.read().await;
                            s.delivery_handler.clone()
                        };

                        match handler {
                            Some(h) => {
                                eprintln!(
                                    "[SpaceDelivery] Accepted delivery connection from {remote}"
                                );
                                spawn_connection_watcher(remote, conn.clone(), state.clone());
                                h.handle_connection(conn).await;
                            }
                            None => {
                                eprintln!(
                                    "[SpaceDelivery] Rejected delivery connection from {remote}: \
                                     no handler registered"
                                );
                            }
                        }
                    } else {
                        eprintln!(
                            "[Endpoint] Rejected connection from {remote}: unknown ALPN {:?}",
                            String::from_utf8_lossy(&alpn)
                        );
                    }
                }
                Err(e) => {
                    eprintln!("[Endpoint] Failed to accept connection: {e}");
                }
            }
        });
    }
}

async fn handle_connection(
    conn: iroh::endpoint::Connection,
    state: Arc<RwLock<PeerState>>,
    own_identity: Arc<Mutex<Option<OwnIdentity>>>,
    own_endpoint_id: String,
) {
    let remote = conn.remote_id();
    let remote_str = remote.to_string();

    // -- Phase 1: DID challenge --
    //
    // The first accepted bi-stream of every connection is the quic_did_auth
    // handshake. Until it succeeds we hold no state for this peer; on
    // success we cache (endpoint_id -> DID) in PeerState so subsequent
    // request handlers can enforce UCAN audience == this DID.
    let identity_snapshot = own_identity.lock().ok().and_then(|g| g.clone());
    let Some(_own_identity) = identity_snapshot else {
        eprintln!(
            "[PeerStorage] Rejecting connection from {remote}: own identity not configured \
             (set_own_identity must run before start)"
        );
        conn.close(3u32.into(), b"no own identity");
        return;
    };

    // Snapshot the DoS-defence config once per connection. Holding an
    // `Arc<DosDefenceConfig>` avoids re-acquiring the state lock on every
    // stream accept (L2 cap check) and gives a stable timeout value for the
    // life of this session (L3). A config change via `set_dos_config` only
    // applies to *new* connections — matches the "no hot-reload" stance from
    // Phase 1.
    let dos_config = state.read().await.dos_config.clone();

    // The server initiates the auth stream so it can write the Challenge
    // first — `open_bi` materialises the stream on the wire as soon as the
    // server writes, which avoids a both-sides-blocked-on-read deadlock that
    // would otherwise occur if both endpoints tried to read first.
    //
    // L3 wraps the whole sequence (open_bi + challenge_and_verify) in one
    // configurable timeout. `open_bi` itself can block on per-connection
    // stream-credit flow control before the handshake even starts, so a
    // peer that withholds credit would otherwise pin this task outside
    // the handshake timeout window. The inner `read_message` still has
    // its own `CHALLENGE_TIMEOUT`; the outer timeout bounds the total.
    enum HandshakeErr {
        OpenStream(String),
        Auth(String),
    }
    let auth_result = tokio::time::timeout(dos_config.l3_handshake_timeout, async {
        let (mut send, mut recv) = conn
            .open_bi()
            .await
            .map_err(|e| HandshakeErr::OpenStream(e.to_string()))?;
        crate::quic_did_auth::challenge_and_verify(
            &mut send,
            &mut recv,
            &own_endpoint_id,
            &remote_str,
        )
        .await
        .map_err(|e| HandshakeErr::Auth(e.to_string()))
    })
    .await;

    let verified_did = match auth_result {
        Ok(Ok(did)) => did,
        Ok(Err(HandshakeErr::OpenStream(e))) => {
            eprintln!("[PeerStorage] Failed to open auth stream to {remote}: {e}");
            return;
        }
        Ok(Err(HandshakeErr::Auth(e))) => {
            eprintln!("[PeerStorage] DID-auth failed for {remote}: {e}");
            conn.close(2u32.into(), b"did-auth failed");
            return;
        }
        Err(_) => {
            eprintln!(
                "[DosDefence L3] handshake timeout for {remote} after {:?}",
                dos_config.l3_handshake_timeout
            );
            conn.close(7u32.into(), b"handshake timeout");
            return;
        }
    };

    let verified_short =
        crate::logging::log_truncate(&verified_did, crate::logging::LOG_TRUNCATE_DEFAULT);
    eprintln!("[PeerStorage] DID-auth ok: {remote} -> {verified_short}");

    // -- Phase 1.5: defense in depth — cross-check the crypto-verified DID
    // against the (endpoint_id -> owner_did) map we loaded from haex_devices.
    // The handshake alone proves "this peer holds the private key for the
    // DID it claims". The DB-side expectation proves "this DID is the one
    // we recorded as the owner of this endpoint id when the row was synced
    // through CRDT with UCAN audience attribution". Either layer alone is
    // sufficient on the happy path; together they make any single-layer
    // compromise (crypto bug, DB drift, partial sync, schema regression)
    // detectable rather than silent.
    {
        let s = state.read().await;
        match s.peer_owner_dids.get(&remote_str) {
            Some(expected) if expected == &verified_did => {
                // happy path — DB and crypto agree
            }
            Some(expected) => {
                let expected_short =
                    crate::logging::log_truncate(expected, crate::logging::LOG_TRUNCATE_DEFAULT);
                eprintln!(
                    "[PeerStorage] Closing connection to {remote}: verified DID does not match \
                     haex_devices.owner_did (verified={verified_short} db={expected_short})"
                );
                conn.close(4u32.into(), b"did/owner_did mismatch");
                return;
            }
            None => {
                // A peer that cleared allowed_peers must also have an entry
                // here — the two maps are loaded from the same DB pass. A
                // missing entry means inconsistent state and we reject
                // rather than accept the crypto-only proof.
                eprintln!(
                    "[PeerStorage] Closing connection to {remote}: no haex_devices.owner_did \
                     entry for verified DID {verified_short}"
                );
                conn.close(5u32.into(), b"no owner_did mapping");
                return;
            }
        }
    }

    state
        .write()
        .await
        .endpoint_dids
        .insert(remote_str.clone(), verified_did.clone());

    // -- Phase 2: normal request loop --
    //
    // L2 stream-cap: the spec calls for at most `l2_max_streams_per_conn`
    // concurrent in-flight stream tasks per connection. A misbehaving peer
    // that keeps opening bi-streams faster than handlers can drain them is
    // pinning server tasks; over the cap we close the whole connection
    // (rather than reject individual streams) — easier for the client to
    // detect and matches the "drop on overuse" pattern at L1.
    let in_flight_streams = Arc::new(AtomicUsize::new(0));
    let max_streams = dos_config.l2_max_streams_per_conn.max(1) as usize;

    loop {
        match conn.accept_bi().await {
            Ok((send, mut recv)) => {
                if in_flight_streams.load(Ordering::Acquire) >= max_streams {
                    eprintln!(
                        "[DosDefence L2] {remote}: in-flight streams >= {max_streams}, closing connection"
                    );
                    conn.close(9u32.into(), b"stream cap exceeded");
                    break;
                }

                // Re-check access on every request — if peer was removed, close immediately
                let allowed_spaces = {
                    let s = state.read().await;
                    s.allowed_peers.get(&remote_str).cloned()
                };

                let Some(allowed_spaces) = allowed_spaces.filter(|s| !s.is_empty()) else {
                    eprintln!("[PeerStorage] Closing connection to {remote}: access revoked");
                    conn.close(1u32.into(), b"access revoked");
                    break;
                };

                in_flight_streams.fetch_add(1, Ordering::AcqRel);
                let guard = StreamCounterGuard(in_flight_streams.clone());

                let state = state.clone();
                let verified_did = verified_did.clone();
                tokio::spawn(async move {
                    let _guard = guard; // decrements on drop, including panic
                    if let Err(e) = crate::peer_storage::handlers::handle_stream(
                        send,
                        &mut recv,
                        &state,
                        &allowed_spaces,
                        &verified_did,
                    )
                    .await
                    {
                        eprintln!("[PeerStorage] Stream error from {remote}: {e}");
                    }
                });
            }
            Err(_) => {
                eprintln!("[PeerStorage] Connection from {remote} closed");
                break;
            }
        }
    }

    // Drop the cached DID when the connection ends — once the QUIC stream
    // is gone the (endpoint_id -> DID) binding from this handshake no longer
    // applies. A future reconnect repeats the handshake.
    state.write().await.endpoint_dids.remove(&remote_str);
}

// ============================================================================
// DoS-defence Phase 2 helpers
// ============================================================================

/// L1 pre-auth rate-limit check. Returns `true` if the connection should be
/// accepted, `false` if it should be silent-dropped. On `true`, the call
/// has already recorded the accept event against both buckets.
///
/// Reads `dos_config` + `accept_tracker` from `PeerState` once, releases the
/// read lock before doing any counter work — the tracker's internal mutex
/// is short-held and we don't want to block other accept-loop iterations on
/// it.
///
/// Buckets are evaluated against a one-second sliding window
/// (`ACCEPT_TRACKER_WINDOW`), so the configured per-second rates map
/// directly to integer counts.
async fn pre_auth_accept_check(state: &Arc<RwLock<PeerState>>, source_key: &str) -> bool {
    let (cfg, tracker, dos_runtime, owner_did) = {
        let s = state.read().await;
        let owner = s.peer_owner_dids.get(source_key).cloned();
        (
            s.dos_config.clone(),
            s.accept_tracker.clone(),
            s.dos_runtime.clone(),
            owner,
        )
    };

    // Phase 3 Ddos-Mode silent-drop: enforced BEFORE the atomic L1 record
    // step so drops are not counted into the per-source bucket (would
    // otherwise let an attacker rate-limit a contact by pushing the contact
    // bucket past the cap with their own drops). Skipped when no runtime is
    // wired or when the escalation policy is `Off`.
    if let Some(runtime) = dos_runtime.as_ref() {
        let now = Instant::now();
        // Re-evaluate FloodMode BEFORE the silent-drop branch so a
        // sustained DDoS still ticks the state machine forward to
        // `Expired` and the persisted row is updated — otherwise a flood
        // of non-contact drops would short-circuit the loop and the
        // contacts-only escalation could never auto-recover. See
        // CodeRabbit review on PR #562.
        let thresholds = thresholds_from_cfg(&cfg);
        let mode =
            runtime.evaluate_and_persist(&tracker, thresholds, now, ACCEPT_TRACKER_GLOBAL_KEY);

        let policy_active = matches!(cfg.ddos_escalation_policy, EscalationPolicy::ContactsOnly);
        if policy_active && matches!(mode, FloodMode::Ddos { .. }) {
            // We need a DID to consult the contact resolver. `peer_owner_dids`
            // is loaded from `haex_devices` so any endpoint_id with a synced
            // owner row is resolvable. Unknown endpoint_id pre-auth = silent
            // drop, because a fresh attacker connection has no DB entry.
            let allow = match owner_did.as_deref() {
                None => false,
                Some(did) => {
                    // ContactResolver opens its own SELECT — fine to call
                    // outside the (already released) PeerState read guard.
                    let db = runtime.db();
                    runtime.contacts().is_contact(&db, did)
                }
            };
            if !allow {
                // Truncate owner_did in the log line: the raw DID is PII
                // and shouldn't end up in stderr for every dropped accept
                // (potentially many per second during a real DDoS).
                let owner_short = owner_did
                    .as_deref()
                    .map(|d| crate::logging::log_truncate(d, crate::logging::LOG_TRUNCATE_DEFAULT))
                    .unwrap_or_else(|| "<unknown>".to_string());
                eprintln!(
                    "[DosDefence Ddos] non-contact source {source_key} silent-dropped \
                     (owner_did={owner_short})"
                );
                // Counter-policy: we do NOT call try_record_l1_accept so the
                // drop does not pollute the per-source bucket.
                return false;
            }
        }
    }

    accept_decision(&cfg, &tracker, source_key, Instant::now())
}

fn thresholds_from_cfg(cfg: &DosDefenceConfig) -> FloodThresholds {
    FloodThresholds {
        global_rate_per_sec: cfg.l1_global_rate_per_sec as usize,
        distinct_sources_threshold: cfg.ddos_distinct_sources_threshold as usize,
        auto_expiry: cfg.ddos_auto_expiry,
    }
}

/// Pure decision step factored out for tests — does not depend on tokio
/// runtime or the live `PeerState`. Delegates the check-and-record to
/// `RejectRateTracker::try_record_l1_accept` so the count/limit/record
/// sequence happens inside one mutex critical section — without that,
/// concurrent accept tasks could each observe counts under the cap and
/// then each record, producing bursts beyond the configured rate.
fn accept_decision(
    cfg: &DosDefenceConfig,
    tracker: &RejectRateTracker,
    source_key: &str,
    now: Instant,
) -> bool {
    use crate::space_delivery::local::dos_defence::tracker::L1AcceptOutcome;
    match tracker.try_record_l1_accept(
        ACCEPT_TRACKER_GLOBAL_KEY,
        cfg.l1_global_rate_per_sec as usize,
        source_key,
        cfg.l1_per_source_rate_per_sec as usize,
        now,
    ) {
        L1AcceptOutcome::Accepted => true,
        L1AcceptOutcome::RejectedGlobal(count) => {
            eprintln!(
                "[DosDefence L1] global rate {count} >= {} — silent-drop {source_key}",
                cfg.l1_global_rate_per_sec
            );
            false
        }
        L1AcceptOutcome::RejectedPerSource(count) => {
            eprintln!(
                "[DosDefence L1] per-source rate {count} >= {} — silent-drop {source_key}",
                cfg.l1_per_source_rate_per_sec
            );
            false
        }
    }
}

/// RAII guard that decrements an in-flight stream counter on drop. Used by
/// the L2 stream cap so a panicking handler still releases its slot.
struct StreamCounterGuard(Arc<AtomicUsize>);

impl Drop for StreamCounterGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

impl PeerEndpoint {
    /// Replace the DoS-defence config used by the accept loop and handshake
    /// path for *subsequent* accepted connections. Active connections keep
    /// the `Arc<DosDefenceConfig>` snapshot they captured at handshake time
    /// — there is no hot-reload.
    ///
    /// Call once after `start` from the vault-open path, with the config
    /// loaded from `haex_vault_settings`. Without this call peer_storage
    /// runs on `DosDefenceConfig::defaults()`.
    pub async fn set_dos_config(&self, cfg: DosDefenceConfig) {
        self.state.write().await.dos_config = Arc::new(cfg);
    }

    /// Install the Phase 3 runtime (FloodMode statemachine + contacts
    /// resolver + DDoS-episode notifier). Until this is called, the accept
    /// loop runs Phase 2 semantics only.
    ///
    /// Call once from the vault-open path, after `set_dos_config` so the
    /// runtime sees the configured thresholds on its first observation.
    pub async fn set_dos_runtime(&self, runtime: Arc<DosDefenceRuntime>) {
        self.state.write().await.dos_runtime = Some(runtime);
    }

    /// Detach the Phase 3 runtime so subsequent accept-loop iterations
    /// fall back to Phase 2 semantics. Used by the `peer_storage_start`
    /// vault-closed fallback to avoid keeping a stale runtime pointing at
    /// a DB whose `critical_sink` is no longer available.
    pub async fn clear_dos_runtime(&self) {
        self.state.write().await.dos_runtime = None;
    }

    /// Borrow the installed Phase 3 runtime, if any. Used by the
    /// `end_dos_escalation` Tauri command and by tests.
    pub async fn dos_runtime(&self) -> Option<Arc<DosDefenceRuntime>> {
        self.state.read().await.dos_runtime.clone()
    }
}

// ============================================================================
// Phase 2 unit tests
// ============================================================================

#[cfg(test)]
mod phase2_tests {
    use super::*;
    use std::time::Duration;

    fn small_cfg() -> DosDefenceConfig {
        DosDefenceConfig {
            l1_global_rate_per_sec: 3,
            l1_per_source_rate_per_sec: 2,
            ..DosDefenceConfig::defaults()
        }
    }

    fn tracker() -> RejectRateTracker {
        RejectRateTracker::new(Duration::from_secs(1))
    }

    // L1 — single source within both limits is accepted and recorded.
    #[test]
    fn l1_accepts_under_limit() {
        let cfg = small_cfg();
        let tr = tracker();
        let now = Instant::now();

        assert!(accept_decision(&cfg, &tr, "src-A", now));
        assert!(accept_decision(&cfg, &tr, "src-A", now));

        // 2 from src-A → per-source bucket at 2, global at 2
        assert_eq!(tr.count_within_window("src-A", now), 2);
        assert_eq!(tr.count_within_window(ACCEPT_TRACKER_GLOBAL_KEY, now), 2);
    }

    // L1 — third accept from the same source hits the per-source cap (=2)
    // and the decision flips to drop. The drop must NOT record.
    #[test]
    fn l1_drops_when_per_source_exceeded() {
        let cfg = small_cfg();
        let tr = tracker();
        let now = Instant::now();

        assert!(accept_decision(&cfg, &tr, "src-A", now));
        assert!(accept_decision(&cfg, &tr, "src-A", now));
        assert!(!accept_decision(&cfg, &tr, "src-A", now));

        // Dropped accept did not bump the counter past the limit.
        assert_eq!(tr.count_within_window("src-A", now), 2);
    }

    // L1 — global cap dominates across distinct sources. With 3/sec global,
    // three accepts from three distinct sources must succeed; a fourth from
    // any source is dropped even though each per-source bucket is well
    // under its 2/sec limit.
    #[test]
    fn l1_drops_when_global_exceeded_across_sources() {
        let cfg = small_cfg();
        let tr = tracker();
        let now = Instant::now();

        assert!(accept_decision(&cfg, &tr, "src-A", now));
        assert!(accept_decision(&cfg, &tr, "src-B", now));
        assert!(accept_decision(&cfg, &tr, "src-C", now));
        assert!(!accept_decision(&cfg, &tr, "src-D", now));
        assert_eq!(tr.count_within_window(ACCEPT_TRACKER_GLOBAL_KEY, now), 3);
    }

    // L1 — events older than the 1-sec window prune out, freeing up
    // capacity for a fresh accept from the same source.
    #[test]
    fn l1_window_pruning_restores_capacity() {
        let cfg = small_cfg();
        let tr = tracker();
        let t0 = Instant::now();

        assert!(accept_decision(&cfg, &tr, "src-A", t0));
        assert!(accept_decision(&cfg, &tr, "src-A", t0));
        // At t0 we're at the cap.
        assert!(!accept_decision(&cfg, &tr, "src-A", t0));

        // Two seconds later both old hits are outside the window.
        let t1 = t0 + Duration::from_secs(2);
        assert!(accept_decision(&cfg, &tr, "src-A", t1));
    }

    // L2 — RAII guard decrements the counter on drop even when the spawned
    // task panicked (the body of the spawn task simulates a panic-unwind by
    // dropping the guard via `drop()` while still inside an error path).
    #[test]
    fn l2_guard_releases_slot_on_drop() {
        let counter = Arc::new(AtomicUsize::new(0));
        counter.fetch_add(1, Ordering::AcqRel);
        {
            let _guard = StreamCounterGuard(counter.clone());
            assert_eq!(counter.load(Ordering::Acquire), 1);
        }
        assert_eq!(counter.load(Ordering::Acquire), 0);
    }

    // L2 — guard decrements even if the holding scope unwinds (panic-safe).
    #[test]
    fn l2_guard_releases_slot_on_panic() {
        let counter = Arc::new(AtomicUsize::new(0));
        counter.fetch_add(1, Ordering::AcqRel);
        let counter_for_thread = counter.clone();

        let result = std::panic::catch_unwind(move || {
            let _guard = StreamCounterGuard(counter_for_thread);
            panic!("simulated handler panic");
        });

        assert!(result.is_err());
        assert_eq!(counter.load(Ordering::Acquire), 0);
    }
}
