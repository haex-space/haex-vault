//! Per-cycle orchestrator: runs push, pull, owner pending-column recovery, and
//! the MLS phases in the right order with the cycle's failure-isolation rules.

use super::super::error::DeliveryError;
use super::super::peer::PeerSession;
use super::{mls, pending_columns, pull, push, SyncMode};
use crate::database::DbConnection;

/// Execute a single push+pull sync cycle.
///
/// Push and pull are independent phases: a failing push (e.g. insufficient
/// UCAN capability, transient protocol error) is logged but does not abort
/// the pull. Only pull failures propagate as `Err` and trigger reconnect,
/// because those are the signal that the session is actually broken.
#[allow(clippy::too_many_arguments)]
pub(super) async fn run_sync_cycle(
    db: &DbConnection,
    session: &PeerSession,
    mode: &SyncMode,
    space_id: &str,
    device_id: &str,
    our_node: Option<u128>,
    can_push_user_content: bool,
    our_identity_id: Option<&str>,
    our_endpoint_id: &str,
    app_handle: &tauri::AppHandle,
    last_push_hlc: &mut Option<String>,
    last_pull_timestamp: &mut Option<String>,
    last_mls_message_id: &mut Option<i64>,
    key_packages_refilled: &mut bool,
) -> Result<(), DeliveryError> {
    // 1. PUSH (best-effort) — never blocks the pull below.
    if let Err(e) = push::run_push_phase(
        db,
        session,
        mode,
        space_id,
        device_id,
        our_node,
        can_push_user_content,
        our_identity_id,
        our_endpoint_id,
        last_push_hlc,
    )
    .await
    {
        eprintln!("[SyncLoop] Push phase failed (pull continues): {}", e);
    }

    // 2. PULL: paginated by transaction-HLC group.
    pull::run_pull_phase(db, session, space_id, app_handle, last_pull_timestamp).await?;

    // 2b. OWNER-VAULT pending-column recovery (best-effort, owner mode only).
    if let SyncMode::OwnerVault { .. } = mode {
        if let Err(e) =
            pending_columns::run_owner_pending_column_recovery(db, session, space_id, app_handle)
                .await
        {
            eprintln!("[SyncLoop] Owner pending-column recovery failed (cycle continues): {e}");
        }
    }

    // 3 + 4. MLS phases run only in space-scoped mode. The owner's own device
    // mesh has no MLS group: there is no leader distributing commits and no
    // KeyPackage exchange, so both phases are skipped. The CRDT push+pull above
    // run in BOTH modes.
    if matches!(mode, SyncMode::SpaceScoped) {
        // 3. MLS: Fetch commits from leader, process, and ACK
        if let Err(e) = mls::fetch_and_process_mls_messages(
            db,
            session,
            space_id,
            device_id,
            last_mls_message_id,
            app_handle,
        )
        .await
        {
            eprintln!("[SyncLoop] MLS message processing failed: {e}");
            // Non-fatal: CRDT sync still worked, MLS will retry next cycle
        }

        // 4. KeyPackage refill: run once per session (ClaimInvite already uploads 10)
        if !*key_packages_refilled {
            match mls::refill_key_packages_if_needed(db, session, space_id).await {
                Ok(()) => *key_packages_refilled = true,
                Err(e) => {
                    eprintln!("[SyncLoop] KeyPackage refill failed (will retry next cycle): {e}")
                }
            }
        }
    }

    Ok(())
}
