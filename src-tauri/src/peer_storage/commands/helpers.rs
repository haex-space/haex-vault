//! DB loaders + endpoint state reload helpers.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::peer_storage::endpoint::is_content_uri;
use crate::peer_storage::error::PeerStorageError;
use crate::AppState;

/// Load shares for the current device from the database.
/// Returns a list of (id, name, local_path, space_id) tuples.
fn load_shares_from_db(
    state: &AppState,
    endpoint_id: &str,
) -> Result<Vec<(String, String, String, String)>, PeerStorageError> {
    let sql = "SELECT id, name, local_path, space_id FROM haex_peer_shares WHERE endpoint_id = ?1"
        .to_string();
    let params = vec![serde_json::Value::String(endpoint_id.to_string())];

    let rows = crate::database::core::select_with_crdt(sql, params, &state.db).map_err(|e| {
        PeerStorageError::Database {
            reason: e.to_string(),
        }
    })?;

    let shares = rows
        .iter()
        .map(|row| {
            let id = row
                .get(0)
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let name = row
                .get(1)
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let path = row
                .get(2)
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let space_id = row
                .get(3)
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            (id, name, path, space_id)
        })
        .collect();

    Ok(shares)
}

/// Load the device's own (DID, signing key) for the quic_did_auth handshake.
/// Joins `haex_devices` (filtered to this endpoint's row) against
/// `haex_identities` to fetch the PKCS8-base64 private key for the identity
/// pinned in `owner_did`.
pub(super) fn load_own_identity_for_device(
    state: &AppState,
    own_endpoint_id: &str,
) -> Result<crate::peer_storage::endpoint::OwnIdentity, PeerStorageError> {
    let sql = "SELECT i.did, i.private_key \
               FROM haex_devices d \
               JOIN haex_identities i ON i.did = d.owner_did \
               WHERE d.endpoint_id = ?1 \
               LIMIT 1"
        .to_string();
    let params = vec![serde_json::Value::String(own_endpoint_id.to_string())];

    let rows = crate::database::core::select_with_crdt(sql, params, &state.db).map_err(|e| {
        PeerStorageError::Database {
            reason: e.to_string(),
        }
    })?;

    let row = rows.first().ok_or_else(|| PeerStorageError::Database {
        reason: format!("no haex_devices row for endpoint_id {own_endpoint_id}"),
    })?;

    let did = row
        .first()
        .and_then(|v| v.as_str())
        .ok_or_else(|| PeerStorageError::Database {
            reason: "missing did column".into(),
        })?
        .to_string();

    let private_key_b64 = row
        .get(1)
        .and_then(|v| v.as_str())
        .ok_or_else(|| PeerStorageError::Database {
            reason: format!("identity row for {did} has no private_key — cannot sign DID-auth"),
        })?
        .to_string();

    let signing_key =
        crate::ucan::signing_key_from_pkcs8_base64(&private_key_b64).map_err(|e| {
            PeerStorageError::Database {
                reason: format!("decoding identity private_key for {did}: {e}"),
            }
        })?;

    // Refuse to load a (did, private_key) pair whose halves don't match:
    // a drifted row would otherwise authenticate as `did` to peers but be
    // unable to sign for it, surfacing only later as silent handshake
    // failures. Encode the public key derived from the private key as a
    // did:key and compare against the row's stored DID — they must be byte
    // identical.
    let derived_did = crate::ucan::did_key_from_public_key(&signing_key.verifying_key());
    if derived_did != did {
        return Err(PeerStorageError::Database {
            reason: format!(
                "identity drift for endpoint_id={own_endpoint_id}: row says did={did} but private_key encodes did={derived_did}"
            ),
        });
    }

    Ok(crate::peer_storage::endpoint::OwnIdentity { did, signing_key })
}

/// Load the expected `(endpoint_id -> owner_did)` map for every peer we
/// could legitimately accept connections from. The query joins
/// `haex_space_devices` (cross-vault, UCAN-attributed) against
/// `haex_devices` (vault-private, populated by the
/// `haex_space_devices_ensure_refs` trigger from `authored_by_did`). The
/// result is the DB-side ground truth that the quic_did_auth handshake's
/// crypto-verified DID is cross-checked against in `handle_connection`.
///
/// Excludes our own endpoint and skips rows whose `owner_did` is NULL —
/// the latter only happens transiently during a partial sync and we'd
/// rather reject the peer than risk admitting an unverifiable one.
fn load_peer_owner_dids(
    state: &AppState,
    own_endpoint_id: &str,
) -> Result<HashMap<String, String>, PeerStorageError> {
    let sql = "SELECT DISTINCT sd.endpoint_id, d.owner_did \
               FROM haex_space_devices sd \
               JOIN haex_devices d ON d.id = sd.device_id \
               WHERE sd.endpoint_id != ?1 \
                 AND d.owner_did IS NOT NULL"
        .to_string();
    let params = vec![serde_json::Value::String(own_endpoint_id.to_string())];

    let rows = crate::database::core::select_with_crdt(sql, params, &state.db).map_err(|e| {
        PeerStorageError::Database {
            reason: e.to_string(),
        }
    })?;

    // Two passes: first gather every distinct (endpoint_id, owner_did)
    // pair, then accept only endpoint_ids that map to exactly one DID.
    // A single-pass loop that removed on conflict would silently let a
    // later row reinstate a conflicted endpoint, making acceptance depend
    // on SQL row order.
    use std::collections::HashSet as StdHashSet;
    let mut candidates: HashMap<String, StdHashSet<String>> = HashMap::new();
    for row in &rows {
        let endpoint_id = row
            .first()
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let owner_did = row
            .get(1)
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        if endpoint_id.is_empty() || owner_did.is_empty() {
            continue;
        }
        candidates.entry(endpoint_id).or_default().insert(owner_did);
    }

    let mut map: HashMap<String, String> = HashMap::new();
    for (endpoint_id, dids) in candidates {
        if dids.len() == 1 {
            map.insert(
                endpoint_id,
                dids.into_iter()
                    .next()
                    .expect("invariant: dids.len() == 1 checked above"),
            );
        } else {
            // Multiple distinct owner_dids for the same endpoint_id: cannot
            // pick a side safely. Drop permanently so handle_connection
            // rejects rather than admit an arbitrary one.
            let did_list: Vec<String> = dids.into_iter().collect();
            eprintln!(
                "[PeerStorage] Ambiguous owner_did for endpoint_id {endpoint_id}: \
                 {did_list:?} — dropping from peer_owner_dids map"
            );
        }
    }
    Ok(map)
}

/// Load allowed peers from haex_space_devices.
/// Returns a map: remote EndpointId (string) -> set of space_ids they may access.
/// Excludes our own endpoint ID.
fn load_allowed_peers_from_db(
    state: &AppState,
    own_endpoint_id: &str,
) -> Result<HashMap<String, HashSet<String>>, PeerStorageError> {
    let sql =
        "SELECT endpoint_id, space_id FROM haex_space_devices WHERE endpoint_id != ?1".to_string();
    let params = vec![serde_json::Value::String(own_endpoint_id.to_string())];

    let rows = crate::database::core::select_with_crdt(sql, params, &state.db).map_err(|e| {
        PeerStorageError::Database {
            reason: e.to_string(),
        }
    })?;

    let mut allowed: HashMap<String, HashSet<String>> = HashMap::new();
    for row in &rows {
        let endpoint_id = row
            .get(0)
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let space_id = row
            .get(1)
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        allowed.entry(endpoint_id).or_default().insert(space_id);
    }

    Ok(allowed)
}

/// Reload only the allowed-peers map from haex_space_devices into the running endpoint.
///
/// Cheaper than reload_state_from_db (skips share path validation). Called from the
/// space-delivery leader after it receives a SyncPush that touches haex_space_devices,
/// so the new peer is authorized before Response::Ok is returned.
pub(crate) async fn reload_allowed_peers(
    state: &AppState,
    endpoint: &crate::peer_storage::endpoint::PeerEndpoint,
) -> Result<(), PeerStorageError> {
    let endpoint_id = endpoint.endpoint_id().to_string();
    let allowed_peers = load_allowed_peers_from_db(state, &endpoint_id)?;
    let peer_count: usize = allowed_peers.values().map(|s| s.len()).sum();
    endpoint.set_allowed_peers(allowed_peers).await;
    // Keep the DID cross-check map in lock-step: a peer that just appeared
    // in allowed_peers but not yet in peer_owner_dids would be rejected by
    // handle_connection. Same SQL pass updates both.
    let owner_dids = load_peer_owner_dids(state, &endpoint_id)?;
    endpoint.set_peer_owner_dids(owner_dids).await;
    eprintln!("[PeerStorage] Updated allowed peers: {peer_count} peers across spaces");
    Ok(())
}

/// Reload shares and allowed peers into the endpoint from DB.
pub(super) async fn reload_state_from_db(
    state: &AppState,
    endpoint: &crate::peer_storage::endpoint::PeerEndpoint,
) -> Result<usize, PeerStorageError> {
    let endpoint_id = endpoint.endpoint_id().to_string();

    let shares = load_shares_from_db(state, &endpoint_id)?;
    let allowed_peers = load_allowed_peers_from_db(state, &endpoint_id)?;
    let peer_owner_dids = load_peer_owner_dids(state, &endpoint_id)?;

    endpoint.clear_shares().await;
    let mut loaded = 0;
    for (id, name, local_path, space_id) in &shares {
        if is_content_uri(local_path) {
            // Android Content URI — cannot validate with std::fs, always load.
            // The android_fs plugin handles validation when actually serving files.
            endpoint
                .add_share(
                    id.clone(),
                    name.clone(),
                    local_path.clone(),
                    space_id.clone(),
                )
                .await;
            loaded += 1;
        } else {
            let path = PathBuf::from(local_path);
            if path.exists() && path.is_dir() {
                endpoint
                    .add_share(
                        id.clone(),
                        name.clone(),
                        local_path.clone(),
                        space_id.clone(),
                    )
                    .await;
                loaded += 1;
            } else {
                eprintln!(
                    "[PeerStorage] Skipping share '{}': path does not exist: {}",
                    name, local_path
                );
            }
        }
    }

    endpoint.set_allowed_peers(allowed_peers).await;
    endpoint.set_peer_owner_dids(peer_owner_dids).await;

    eprintln!(
        "[PeerStorage] Loaded {loaded}/{} shares from DB",
        shares.len()
    );
    Ok(loaded)
}
