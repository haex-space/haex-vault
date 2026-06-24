//! Owner-vault sync commands: serverless full-vault sync across the owner's
//! own devices.

use tauri::State;

use crate::database::DbConnection;
use crate::AppState;

/// Filter `discovered` peer endpoints down to those not already running.
///
/// Pure decision function — the idempotency core of [`owner_sync_start`].
/// Returns the discovered endpoints, in order, that are absent from `running`.
/// Extracted so the "skip peers already running" logic is unit-testable
/// without an `AppHandle`, QUIC endpoint, or DB.
pub(super) fn peers_to_start(
    discovered: &[String],
    running: &std::collections::HashSet<String>,
) -> Vec<String> {
    discovered
        .iter()
        .filter(|ep| !running.contains(*ep))
        .cloned()
        .collect()
}

/// Start owner-vault sync loops to every OTHER device of this vault's owner.
///
/// Serverless full-vault sync across the owner's own devices. For each owner
/// device endpoint not already running, starts an [`SyncMode::OwnerVault`] loop
/// that pushes/pulls the FULL CRDT table set (DID-auth only, no UCAN).
///
/// Graceful no-op (returns `Ok`) when there is no vault owner DID or no vault
/// space — there is nothing to sync. Idempotent: peers already running are
/// skipped, so repeated calls only start loops for newly-discovered devices.
#[tauri::command]
pub async fn owner_sync_start(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let db = DbConnection(state.db.0.clone());

    // 1+2. Resolve the vault owner DID and vault space id in a single DB pass.
    // Either being absent means there is nothing to sync — return Ok gracefully.
    let (owner_did, vault_space_id) = crate::database::core::with_connection(&db, |conn| {
        let map_query_err =
            |e: rusqlite::Error| crate::database::error::DatabaseError::QueryError {
                reason: e.to_string(),
            };
        let owner_did =
            crate::owner_sync::scope::resolve_vault_owner_did(conn).map_err(map_query_err)?;
        let vault_space_id =
            crate::owner_sync::scope::resolve_vault_space_id(conn).map_err(map_query_err)?;
        Ok((owner_did, vault_space_id))
    })
    .map_err(|e| e.to_string())?;
    let owner_did = match owner_did {
        Some(d) => d,
        None => return Ok(()),
    };
    let vault_space_id = match vault_space_id {
        Some(s) => s,
        None => return Ok(()),
    };

    // 3. Resolve our own endpoint info + iroh endpoint + the relay the
    // endpoint is currently on (default-chain resolved at peer_storage start).
    let endpoint = state.peer_storage.read().await;
    if !endpoint.is_running() {
        return Err("Peer storage endpoint not running".to_string());
    }
    let our_endpoint_id = endpoint.endpoint_id().to_string();
    let iroh_endpoint = endpoint
        .endpoint_ref()
        .ok_or("Endpoint not running")?
        .clone();
    let relay_url: Option<String> = endpoint.configured_relay_url().map(|r| r.to_string());
    drop(endpoint);

    // Resolve the HLC device UUID — the same UUID embedded in every row's HLC
    // timestamp, so the push-scanner origin filter distinguishes locally-
    // authored rows from pulled rows and avoids ping-pong.
    let device_id = crate::crdt::hlc::HlcService::get_or_create_device_id(&app)
        .map_err(|e| format!("Failed to read device UUID: {e}"))?;

    // 4+5. Resolve the full CRDT table list for the owner-vault scan and
    // enumerate the owner's OTHER device endpoints in a single DB pass.
    let (tables, peers) = crate::database::core::with_connection(&db, |conn| {
        let tables = crate::database::init::discover_crdt_tables(conn)?;
        let peers = crate::owner_sync::scope::resolve_owner_device_endpoints(
            conn,
            &owner_did,
            &our_endpoint_id,
        )
        .map_err(|e| crate::database::error::DatabaseError::QueryError {
            reason: e.to_string(),
        })?;
        Ok((tables, peers))
    })
    .map_err(|e| e.to_string())?;

    // 6. Start a loop per not-yet-running peer (idempotent skip).
    let mut loops = state.owner_sync_loops.lock().await;
    let running: std::collections::HashSet<String> = loops.keys().cloned().collect();
    eprintln!(
        "[OWNER_SYNC_DIAG] start_discovery vault_space_id={vault_space_id} \
         own_endpoint={our_endpoint_id} owner_did={owner_did} \
         discovered_peers={peers:?} already_running={:?}",
        running.iter().collect::<Vec<_>>(),
    );
    let to_start = peers_to_start(&peers, &running);

    eprintln!(
        "[OWNER_SYNC_DIAG] starting_loops own_endpoint={our_endpoint_id} \
         to_start={to_start:?}",
    );
    for peer_endpoint in to_start {
        let handle = match super::super::sync_loop::start_peer_sync_loop(
            DbConnection(state.db.0.clone()),
            iroh_endpoint.clone(),
            super::super::sync_loop::SyncMode::OwnerVault {
                tables: tables.clone(),
            },
            peer_endpoint.clone(),
            relay_url.clone(),
            // The vault space id doubles as the cursor namespace AND the
            // SyncPull/SyncPush space_id the serving gate matches against.
            //
            // PUSH-CURSOR SHARING CAVEAT: because every owner loop on this
            // device passes the same `space_id` (= vault_space_id) and the same
            // `device_id`, they all share ONE push cursor row
            // (`local_sync_push_hlc:<vault_space_id>`, device_id). With 3+ owner
            // devices the per-peer push cursor is therefore last-writer-wins, so
            // push-side delivery is NOT per-peer-correct (one peer's advance can
            // hide rows from another peer). Convergence is still guaranteed: the
            // symmetric FULL pull (`handle_owner_pull`, `origin_node = None`,
            // serves everything) closes any push gap on the next pull.
            // FOLLOW-UP: to make push-side per-peer-correct, decouple the cursor
            // namespace from the request `space_id` (which the serving gate
            // matches against `vault_space_id`) and key the owner push cursor
            // per peer endpoint — deferred to a later phase.
            vault_space_id.clone(),
            // our_did = the vault-owner DID; the loop loads this identity's
            // signing key, so DID-auth proves the owner DID.
            owner_did.clone(),
            our_endpoint_id.clone(),
            // Our real device UUID for the scanner origin filter.
            device_id.clone(),
            app.clone(),
        )
        .await
        {
            Ok(h) => h,
            Err(e) => {
                // Best-effort: a single unreachable owner device must not
                // abandon the whole batch. Successfully-started loops have
                // already been inserted into `loops` and continue running; on
                // the next `owner_sync_start` call `peers_to_start` will skip
                // them and retry only the peers that failed here.
                eprintln!("[OwnerSync] start_peer_sync_loop failed for peer {peer_endpoint}: {e}");
                continue;
            }
        };
        loops.insert(peer_endpoint, handle);
    }

    Ok(())
}

/// Stop all owner-vault sync loops and clear the map.
#[tauri::command]
pub async fn owner_sync_stop(state: State<'_, AppState>) -> Result<(), String> {
    let mut loops = state.owner_sync_loops.lock().await;
    for (_, handle) in loops.drain() {
        handle.stop();
    }
    Ok(())
}

/// Cut the current poll sleep short for every running owner-vault loop so the
/// next sync cycle starts immediately. No-op when none are running.
#[tauri::command]
pub async fn owner_sync_force(state: State<'_, AppState>) -> Result<(), String> {
    let loops = state.owner_sync_loops.lock().await;
    for (_, handle) in loops.iter() {
        handle.wakeup();
    }
    Ok(())
}
