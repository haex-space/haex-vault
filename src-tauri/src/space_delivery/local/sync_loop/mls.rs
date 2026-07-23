//! MLS phases: fetch & process commits, rejoin via External Commit on epoch
//! gap, and KeyPackage refill. Space-scoped mode only.

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use tauri::Emitter;

use super::super::error::DeliveryError;
use super::super::peer::PeerSession;
use super::super::push_cursor::save_last_mls_cursor;
use crate::database::DbConnection;

/// Fetch MLS messages from leader, process them locally, and send ACKs.
pub(super) async fn fetch_and_process_mls_messages(
    db: &DbConnection,
    session: &PeerSession,
    space_id: &str,
    device_id: &str,
    last_mls_message_id: &mut Option<i64>,
    app_handle: &tauri::AppHandle,
) -> Result<(), DeliveryError> {
    let messages = session
        .fetch_mls_messages(space_id, *last_mls_message_id)
        .await?;

    if messages.is_empty() {
        return Ok(());
    }

    eprintln!(
        "[SyncLoop] Processing {} MLS message(s) for space {}",
        messages.len(),
        space_id
    );

    let mut acked_ids = Vec::new();

    for msg in &messages {
        let blob = match BASE64.decode(&msg.message) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("[SyncLoop] Failed to decode MLS message {}: {e}", msg.id);
                continue;
            }
        };

        match crate::mls::blocking::process_message(db.0.clone(), space_id.to_string(), blob).await
        {
            Ok(_) => {
                acked_ids.push(msg.id);
                *last_mls_message_id = Some(msg.id);
                save_last_mls_cursor(db, space_id, device_id, msg.id);
                eprintln!(
                    "[SyncLoop] Processed MLS {} message (id={})",
                    msg.message_type, msg.id
                );
            }
            Err(e) => {
                eprintln!("[SyncLoop] Failed to process MLS message {}: {e}", msg.id);

                // Detect epoch gap — attempt rejoin via External Commit
                if e.contains("epoch") || e.contains("Welcome") || e.contains("group") {
                    eprintln!("[SyncLoop] Possible epoch gap detected, attempting rejoin for space {space_id}");
                    match attempt_rejoin(db, session, space_id, app_handle).await {
                        Ok(ec_msg_id) => {
                            // After External Commit our local epoch jumped to
                            // the leader's current epoch. Advance the cursor
                            // to the max of:
                            //   (a) the highest id in the current batch — skips
                            //       all stale historical commits in this fetch.
                            //   (b) the msg_id of the External Commit just
                            //       stored by the leader — skips the EC itself
                            //       so the next cycle doesn't re-fetch it and
                            //       trip on its old epoch number. Without this,
                            //       every EC stored in the buffer triggers
                            //       another rejoin in an infinite loop.
                            let batch_max = messages.iter().map(|m| m.id).max().unwrap_or(msg.id);
                            let skip_to = batch_max.max(ec_msg_id);
                            eprintln!(
                                "[SyncLoop] Rejoin successful, advancing cursor past msg {} (skipping {} stale message(s)) for space {space_id}",
                                skip_to,
                                messages.len() - acked_ids.len(),
                            );
                            *last_mls_message_id = Some(skip_to);
                            save_last_mls_cursor(db, space_id, device_id, skip_to);
                        }
                        Err(rejoin_err) => {
                            eprintln!("[SyncLoop] Rejoin failed: {rejoin_err}");
                        }
                    }
                }

                break;
            }
        }
    }

    // ACK successfully processed messages
    if !acked_ids.is_empty() {
        let count = acked_ids.len();
        session.ack_commits(space_id, acked_ids).await?;

        // Emit event for frontend (main window only).
        let _ = app_handle.emit_to(
            "main",
            "local-mls-commit-processed",
            serde_json::json!({
                "spaceId": space_id,
                "processedCount": count,
            }),
        );
    }

    Ok(())
}

/// Attempt to rejoin an MLS group via External Commit after detecting an epoch gap.
/// Returns the message ID of the stored External Commit so the caller can advance
/// the MLS cursor past it (preventing the next fetch from re-tripping on it).
async fn attempt_rejoin(
    db: &DbConnection,
    session: &PeerSession,
    space_id: &str,
    app_handle: &tauri::AppHandle,
) -> Result<i64, DeliveryError> {
    // 1. Request GroupInfo from leader
    let group_info_b64 = session.request_rejoin(space_id).await?;

    let group_info_bytes =
        BASE64
            .decode(&group_info_b64)
            .map_err(|e| DeliveryError::ProtocolError {
                reason: format!("Failed to decode GroupInfo: {e}"),
            })?;

    // 2. Create External Commit
    let (commit_bytes, epoch_key) = crate::mls::blocking::join_by_external_commit(
        db.0.clone(),
        space_id.to_string(),
        group_info_bytes,
    )
    .await
    .map_err(|e| DeliveryError::ProtocolError {
        reason: format!("External commit failed: {e}"),
    })?;

    let commit_b64 = BASE64.encode(&commit_bytes);

    // 3. Submit the External Commit to the leader for distribution.
    //    The returned msg_id lets the caller advance the MLS cursor past the
    //    EC so the next fetch doesn't re-process it as a stale epoch-N message.
    let ec_msg_id = session
        .submit_external_commit(space_id, &commit_b64)
        .await?;

    // 4. Emit event so frontend can update the epoch key (main window only).
    let _ = app_handle.emit_to(
        "main",
        "local-mls-rejoin-completed",
        serde_json::json!({
            "spaceId": space_id,
            "newEpoch": epoch_key.epoch,
        }),
    );

    eprintln!(
        "[SyncLoop] Rejoin completed for space {space_id}, new epoch: {}",
        epoch_key.epoch
    );

    Ok(ec_msg_id)
}

/// Query the leader for key package status and upload more if requested.
pub(super) async fn refill_key_packages_if_needed(
    db: &DbConnection,
    session: &PeerSession,
    space_id: &str,
) -> Result<(), DeliveryError> {
    let (available, needed) = session.query_key_package_status(space_id).await?;

    if needed == 0 {
        return Ok(());
    }

    eprintln!("[SyncLoop] KeyPackage refill: {available} on leader, {needed} more requested");

    let own_did = crate::mls::manager::MlsManager::new(db.0.clone())
        .get_own_did()
        .map_err(|e| DeliveryError::ProtocolError {
            reason: format!("Failed to load own DID for key package refill: {e}"),
        })?;
    let identity =
        super::super::quic_retry::load_signing_identity_for_did(db, &own_did).map_err(|e| {
            DeliveryError::ProtocolError {
                reason: format!("Failed to load identity for proof-of-possession: {e}"),
            }
        })?;

    let packages =
        crate::mls::blocking::generate_key_packages(db.0.clone(), needed, identity.signing_key)
            .await
            .map_err(|e| DeliveryError::ProtocolError {
                reason: format!("Failed to generate key packages: {e}"),
            })?;

    let packages_b64: Vec<String> = packages.iter().map(|(kp, _)| BASE64.encode(kp)).collect();
    let pops_b64: Vec<String> = packages.iter().map(|(_, pop)| BASE64.encode(pop)).collect();

    session
        .upload_key_packages(space_id, packages_b64, pops_b64)
        .await?;

    eprintln!("[SyncLoop] Uploaded {needed} key packages for space {space_id}");

    Ok(())
}
