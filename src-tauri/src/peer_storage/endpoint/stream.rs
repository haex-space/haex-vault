//! Outbound QUIC stream lifecycle — open, authenticate, send, and inspect.

use iroh::{EndpointAddr, EndpointId, RelayUrl};

use crate::peer_storage::endpoint::diagnostics::{compute_diagnostics, spawn_connection_watcher};
use crate::peer_storage::endpoint::{ConnectionDiagnostics, PeerEndpoint};
use crate::peer_storage::error::PeerStorageError;
use crate::peer_storage::protocol::{self, Request, Response, ALPN};

impl PeerEndpoint {
    /// Inspect the cached connection to a peer and report whether it currently
    /// runs over a direct LAN/WAN path or via the relay. Returns `None` if
    /// there is no live connection — call only after the engine has issued at
    /// least one stream against the peer.
    ///
    /// This exists primarily to debug the "iroh fell back to relay" failure
    /// mode, which presents as a steady ~1 MB/s ceiling per stream and looks
    /// like a code-tuning problem until you check the path type.
    ///
    /// For push updates when the path changes, the frontend should listen for
    /// `peer-storage:connection-changed` events emitted by
    /// `spawn_connection_watcher` (started for every connection that lands in
    /// the cache).
    pub fn diagnose_connection(&self, remote_id: EndpointId) -> Option<ConnectionDiagnostics> {
        self.connections
            .lock()
            .ok()
            .and_then(|cache| cache.get(&remote_id).cloned())
            .map(|conn| compute_diagnostics(&conn))
    }

    /// Get a cached QUIC connection or establish a new one, then open a
    /// bidirectional stream. If a cached connection is stale, it is evicted
    /// and a fresh one is created automatically.
    pub(crate) async fn open_stream(
        &self,
        remote_id: EndpointId,
        relay_url: Option<RelayUrl>,
    ) -> Result<(iroh::endpoint::SendStream, iroh::endpoint::RecvStream), PeerStorageError> {
        let endpoint = self
            .endpoint
            .as_ref()
            .ok_or(PeerStorageError::EndpointNotRunning)?;

        // Try the cached connection first
        let cached = self
            .connections
            .lock()
            .ok()
            .and_then(|cache| cache.get(&remote_id).cloned());

        if let Some(conn) = cached {
            // A cached connection can be half-closed after the remote revokes
            // authorization: open_bi() may optimistically succeed, then the
            // subsequent read hangs until QUIC's idle timeout (~41s). Detect
            // stale connections via close_reason(), and bound open_bi() so a
            // connection that has silently died cannot stall the caller.
            if conn.close_reason().is_none() {
                if let Ok(Ok(streams)) =
                    tokio::time::timeout(std::time::Duration::from_secs(3), conn.open_bi()).await
                {
                    return Ok(streams);
                }
            }
            // Stale or corrupted — evict from cache. Do NOT call .close() here:
            // parallel tasks may still hold streams on this connection, and an
            // explicit close would tear them down mid-transfer.
            if let Ok(mut cache) = self.connections.lock() {
                cache.remove(&remote_id);
            }
        }

        // Establish a new connection
        let addr = match relay_url {
            Some(url) => EndpointAddr::new(remote_id).with_relay_url(url),
            None => EndpointAddr::new(remote_id),
        };

        // iroh's connect() has no caller-visible timeout; if the peer is
        // unreachable the QUIC handshake hangs ~30s before failing. That
        // makes file-sync feel hung when a peer just isn't online. 8s is
        // generous for LAN (<100ms), hole-punched WAN (~1-3s), and relay
        // (~1-2s) paths while failing fast on truly dead peers.
        let conn = match tokio::time::timeout(
            std::time::Duration::from_secs(8),
            endpoint.connect(addr, ALPN),
        )
        .await
        {
            Ok(Ok(conn)) => conn,
            Ok(Err(e)) => {
                return Err(PeerStorageError::ConnectionFailed {
                    reason: e.to_string(),
                });
            }
            Err(_) => {
                return Err(PeerStorageError::ConnectionFailed {
                    reason: "connect handshake timed out after 8s".to_string(),
                });
            }
        };

        // -- Phase 1: DID handshake on a server-initiated bi-stream --
        //
        // Server `handle_connection` calls `open_bi` and writes the Challenge
        // first; we await it here with `accept_bi`. Doing it that direction
        // avoids a both-sides-blocked-on-read deadlock — `open_bi` alone does
        // not materialise the stream on the wire, so client-initiated +
        // server-initiated reads would both block forever.
        let identity = self
            .own_identity()
            .ok_or_else(|| PeerStorageError::ConnectionFailed {
                reason: "own identity not configured — call set_own_identity before open_stream"
                    .into(),
            })?;
        let own_endpoint_id_str = endpoint.id().to_string();

        match tokio::time::timeout(std::time::Duration::from_secs(5), conn.accept_bi()).await {
            Ok(Ok((mut auth_send, mut auth_recv))) => {
                if let Err(e) = crate::quic_did_auth::respond_to_challenge(
                    &mut auth_send,
                    &mut auth_recv,
                    &identity.did,
                    &identity.signing_key,
                    &own_endpoint_id_str,
                )
                .await
                {
                    return Err(PeerStorageError::ConnectionFailed {
                        reason: format!("DID-auth handshake failed: {e}"),
                    });
                }
                // Close the auth stream cleanly so the server sees end-of-send
                // and can hand off to its Phase 2 accept loop without delay.
                let _ = auth_send.finish();
            }
            Ok(Err(e)) => {
                return Err(PeerStorageError::ConnectionFailed {
                    reason: format!("accept auth stream: {e}"),
                });
            }
            Err(_) => {
                return Err(PeerStorageError::ConnectionFailed {
                    reason: "accept auth stream timed out after 5s".to_string(),
                });
            }
        };

        // -- Phase 2: open the actual request stream --
        //
        // Same rationale as the cached-path open_bi: never let stream open
        // outlast the connect bound.
        let streams =
            match tokio::time::timeout(std::time::Duration::from_secs(3), conn.open_bi()).await {
                Ok(Ok(streams)) => streams,
                Ok(Err(e)) => {
                    return Err(PeerStorageError::ConnectionFailed {
                        reason: e.to_string(),
                    });
                }
                Err(_) => {
                    return Err(PeerStorageError::ConnectionFailed {
                        reason: "open_bi timed out after 3s".to_string(),
                    });
                }
            };

        if let Ok(mut cache) = self.connections.lock() {
            cache.insert(remote_id, conn.clone());
        }
        // Push connection-changed events on path switches (direct↔relay) and
        // drop. Cheaper than periodic polling from the frontend and gives the
        // UI a real-time signal without a setInterval.
        spawn_connection_watcher(remote_id, conn, self.state.clone());
        Ok(streams)
    }

    /// Encode a request, send it on the stream, signal end-of-send, and read the response.
    pub(crate) async fn send_request(
        send: &mut iroh::endpoint::SendStream,
        recv: &mut iroh::endpoint::RecvStream,
        req: &Request,
    ) -> Result<Response, PeerStorageError> {
        let req_bytes =
            protocol::encode_request(req).map_err(|e| PeerStorageError::ProtocolError {
                reason: e.to_string(),
            })?;
        send.write_all(&req_bytes)
            .await
            .map_err(|e| PeerStorageError::ConnectionFailed {
                reason: e.to_string(),
            })?;
        send.finish()
            .map_err(|e| PeerStorageError::ConnectionFailed {
                reason: e.to_string(),
            })?;

        protocol::read_response(recv)
            .await
            .map_err(|e| PeerStorageError::ProtocolError {
                reason: e.to_string(),
            })
    }

    /// Send a request header without finishing the send side (caller will stream more data).
    pub(crate) async fn send_request_header(
        send: &mut iroh::endpoint::SendStream,
        req: &Request,
    ) -> Result<(), PeerStorageError> {
        let req_bytes =
            protocol::encode_request(req).map_err(|e| PeerStorageError::ProtocolError {
                reason: e.to_string(),
            })?;
        send.write_all(&req_bytes)
            .await
            .map_err(|e| PeerStorageError::ConnectionFailed {
                reason: e.to_string(),
            })?;
        Ok(())
    }
}
