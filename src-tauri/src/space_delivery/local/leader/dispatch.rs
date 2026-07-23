//! Request dispatcher: routes parsed requests to the per-variant handler.

use time::OffsetDateTime;

use super::super::buffer;
use super::super::error::DeliveryError;
use super::super::protocol::{self, MlsMessageEntry, Notification, Request, Response};
use super::super::push_invite;
use super::super::types::{ConnectedPeer, PeerClaim};
use super::auth::{require_ucan_capability, require_valid_ucan};
use super::claim::handle_claim_invite;
use super::notify::{notify_all_mls, notify_others_sync};
use super::util::{base64_decode, base64_encode};
use super::LeaderState;
use crate::crdt::commands::{apply_remote_changes_to_db, RemoteColumnChange};
use crate::crdt::scanner::{
    paginate_changes, scan_space_scoped_tables_for_local_changes, LocalColumnChange,
    PULL_PAGE_BUDGET,
};
use crate::critical::CriticalFailureCode;
use crate::ucan::CapabilityLevel;
use tauri::{Emitter, Manager};

/// Target number of key packages the leader wants each peer to maintain.
const TARGET_KEY_PACKAGES_PER_PEER: u32 = 10;

// ============================================================================
// Request dispatcher
// ============================================================================

/// Dispatch an already-parsed request to the appropriate handler and return the response.
/// Called by `MultiSpaceLeaderHandler` after routing to the correct `LeaderState` by space_id.
pub(crate) async fn handle_delivery_request(
    state: &LeaderState,
    request: Request,
    peer_endpoint_id: &str,
    verified_did: &str,
) -> Response {
    // Unified auth choke point. Bypass requests (Announce, ClaimInvite,
    // PushInvite) return `Ok(None)` and proceed unchanged; every other
    // variant must come from a peer that already Announced on this
    // connection, carry a UCAN whose audience matches the
    // connection-authenticated DID, grant at least the per-request minimum
    // capability, and still resolve to an active member.
    //
    // For non-bypass arms the gate's `ValidatedUcan` is the single source of
    // UCAN truth — those arms read it via
    // `gate_ucan.as_ref().expect("non-bypass <arm> must have ValidatedUcan from gate")`
    // and **must not** re-validate the request's `ucan_token` field. The
    // wire-format `ucan_token` is now redundant for non-bypass requests;
    // removing it is left to a follow-up so this PR avoids a protocol break.
    //
    // Bypass arms (Announce, ClaimInvite, PushInvite) see `gate_ucan = None`
    // and still run their own UCAN handling — Announce in particular must
    // validate + cache the UCAN it just received before subsequent requests
    // on this connection can pass the gate.
    //
    // Audit logging: the gate writes a `warn` row to `haex_logs_no_sync`
    // (via `log_to_db` → the dedicated `LogSink`, intentionally NOT
    // CRDT-synced) from every reject branch with `source = Request::op_name`,
    // restoring the in-app log visibility the pre-T6 SyncPush / SyncPull arms
    // used to emit directly.
    let gate_ucan = match super::super::auth_gate::authorize_request(
        &request,
        verified_did,
        peer_endpoint_id,
        &state.connected_peers,
        &state.db,
        &state.reject_tracker,
        &state.dos_config,
        &state.flood_notifier,
        state.critical_sink.as_ref(),
        state.log_sink.as_ref(),
    )
    .await
    {
        Ok(maybe) => maybe,
        Err(response) => return response,
    };

    match request {
        Request::Announce {
            endpoint_id,
            space_id,
            label,
            claims,
            ucan_token,
        } => {
            // The connection-bound DID from the quic_did_auth handshake is
            // the only identity we trust for this announce. The payload `did`
            // field has been removed from the wire in C10 — see plan §1.3 +
            // §4.2 for the spoofing vector that carrying it would re-enable.
            let did: String = verified_did.to_string();
            // Announce is the first authenticated boundary of a peer session.
            // Anyone can open a QUIC stream with the ALPN and claim a DID, so
            // we must verify the UCAN before trusting `did` and before
            // populating `connected_peers` (which later handlers rely on).
            crate::logging::log_to_db(
                state.log_sink.as_ref(),
                "info",
                "Announce",
                &format!(
                    "received: space={} did={} peer={}",
                    &space_id[..8.min(space_id.len())],
                    &did[..24.min(did.len())],
                    peer_endpoint_id,
                ),
                None,
            );
            // Announce bootstraps the AuthGate cache, so its `ucan_token`
            // must be present even though the wire field is now
            // `Option<String>` (forward-compat shape for the other request
            // variants; see protocol.rs for the rationale).
            let ucan_token_str = match ucan_token.as_deref() {
                Some(t) => t,
                None => {
                    crate::logging::log_to_db(
                        state.log_sink.as_ref(),
                        "warn",
                        "Announce",
                        &format!(
                            "missing ucan_token: space={} did={}",
                            &space_id[..8.min(space_id.len())],
                            &did[..24.min(did.len())]
                        ),
                        None,
                    );
                    return Response::Error {
                        message: "Announce requires ucan_token".to_string(),
                    };
                }
            };
            let validated = match require_valid_ucan(ucan_token_str, "Announce") {
                Ok(v) => v,
                Err(r) => {
                    crate::logging::log_to_db(
                        state.log_sink.as_ref(),
                        "warn",
                        "Announce",
                        &format!(
                            "UCAN validation failed: space={} did={}",
                            &space_id[..8.min(space_id.len())],
                            &did[..24.min(did.len())]
                        ),
                        None,
                    );
                    return r;
                }
            };
            // Audience-vs-announced-DID is now enforced inside
            // require_ucan_capability via require_audience; no separate
            // pre-check needed.
            if let Err(r) = require_ucan_capability(
                &validated,
                &space_id,
                CapabilityLevel::Read,
                &did,
                "Announce",
                &state.db,
            ) {
                crate::logging::log_to_db(
                    state.log_sink.as_ref(),
                    "warn",
                    "Announce",
                    &format!(
                        "capability/membership rejected: space={} audience={}",
                        &space_id[..8.min(space_id.len())],
                        &validated.audience[..24.min(validated.audience.len())]
                    ),
                    None,
                );
                return r;
            }
            crate::logging::log_to_db(
                state.log_sink.as_ref(),
                "info",
                "Announce",
                &format!(
                    "accepted: space={} audience={}",
                    &space_id[..8.min(space_id.len())],
                    &validated.audience[..24.min(validated.audience.len())]
                ),
                None,
            );

            let did_clone = did.clone();
            let peer = ConnectedPeer {
                endpoint_id: endpoint_id.clone(),
                did,
                label,
                claims: claims
                    .unwrap_or_default()
                    .into_iter()
                    .map(|c| PeerClaim {
                        claim_type: c.claim_type,
                        value: c.value,
                    })
                    .collect(),
                connected_at: OffsetDateTime::now_utc()
                    .format(&time::format_description::well_known::Rfc3339)
                    .unwrap_or_default(),
                validated_ucan: Some(validated.clone()),
            };
            state
                .connected_peers
                .write()
                .await
                .insert(endpoint_id.clone(), peer);

            // Re-notify about unacked commits for this peer
            let unacked =
                buffer::get_unacked_message_ids_for_member(&state.db, &state.space_id, &did_clone)
                    .unwrap_or_default();

            if !unacked.is_empty() {
                eprintln!(
                    "[SpaceDelivery] Peer {} has {} unacked commits, re-notifying",
                    &did_clone[..20.min(did_clone.len())],
                    unacked.len(),
                );
                let senders = state.notification_senders.read().await;
                if let Some(sender) = senders.get(&endpoint_id) {
                    let _ = sender.try_send(Notification::Mls {
                        space_id: state.space_id.clone(),
                        message_type: "commit".to_string(),
                    });
                }
            }

            Response::Ok
        }

        // -- MLS Key Packages --
        Request::MlsUploadKeyPackages {
            space_id,
            packages,
            pops,
        } => {
            let did = verified_did.to_string();
            for (pkg_b64, pop_b64) in packages.iter().zip(pops.iter()) {
                if let (Ok(blob), Ok(pop_blob)) = (base64_decode(pkg_b64), base64_decode(pop_b64)) {
                    let _ = buffer::store_key_package(&state.db, &space_id, &did, &blob, &pop_blob);
                }
            }
            // Trim excess packages — keep only the target amount, discard oldest
            let _ =
                buffer::trim_key_packages(&state.db, &space_id, &did, TARGET_KEY_PACKAGES_PER_PEER);
            Response::Ok
        }

        Request::MlsFetchKeyPackage {
            space_id,
            target_did,
        } => match buffer::consume_key_package(&state.db, &space_id, &target_did) {
            Ok(Some((blob, pop))) => Response::KeyPackage {
                package: base64_encode(&blob),
                pop: base64_encode(&pop),
            },
            Ok(None) => Response::Error {
                message: format!("No key package for {target_did}"),
            },
            Err(e) => Response::Error {
                message: e.to_string(),
            },
        },

        // -- MLS Messages --
        Request::MlsSendMessage {
            space_id,
            message,
            message_type,
        } => {
            let did = verified_did.to_string();
            match base64_decode(&message) {
                Ok(blob) => {
                    match buffer::store_message(&state.db, &space_id, &did, &message_type, &blob) {
                        Ok(id) => {
                            // Track pending ACKs for commits
                            if message_type == "commit" {
                                let expected_dids: Vec<String> =
                                    buffer::get_space_member_dids(&state.db, &space_id)
                                        .unwrap_or_default()
                                        .into_iter()
                                        .filter(|d| d != &did) // exclude sender
                                        .collect();
                                if !expected_dids.is_empty() {
                                    let _ = buffer::store_pending_commit(
                                        &state.db,
                                        &space_id,
                                        id,
                                        &expected_dids,
                                    );
                                }
                            }

                            notify_all_mls(state, &space_id, &message_type).await;
                            Response::MessageStored { message_id: id }
                        }
                        Err(e) => Response::Error {
                            message: e.to_string(),
                        },
                    }
                }
                Err(e) => Response::Error { message: e },
            }
        }

        Request::MlsFetchMessages { space_id, after_id } => {
            match buffer::fetch_messages(&state.db, &space_id, after_id) {
                Ok(msgs) => {
                    let entries: Vec<MlsMessageEntry> = msgs
                        .into_iter()
                        .map(
                            |(id, sender_did, msg_type, blob, created_at)| MlsMessageEntry {
                                id,
                                sender_did,
                                message_type: msg_type,
                                message: base64_encode(&blob),
                                created_at,
                            },
                        )
                        .collect();
                    Response::Messages { messages: entries }
                }
                Err(e) => Response::Error {
                    message: e.to_string(),
                },
            }
        }

        // -- MLS Welcomes --
        Request::MlsSendWelcome {
            space_id,
            recipient_did,
            welcome,
        } => match base64_decode(&welcome) {
            Ok(blob) => match buffer::store_welcome(&state.db, &space_id, &recipient_did, &blob) {
                Ok(_) => Response::Ok,
                Err(e) => Response::Error {
                    message: e.to_string(),
                },
            },
            Err(e) => Response::Error { message: e },
        },

        Request::MlsFetchWelcomes { space_id } => {
            let did = verified_did.to_string();
            match buffer::fetch_welcomes(&state.db, &space_id, &did) {
                Ok(entries) => {
                    let encoded: Vec<String> = entries
                        .iter()
                        .map(|(_, blob)| base64_encode(blob))
                        .collect();
                    for (id, _) in &entries {
                        let _ = buffer::mark_welcome_consumed(&state.db, id);
                    }
                    Response::Welcomes { welcomes: encoded }
                }
                Err(e) => Response::Error {
                    message: e.to_string(),
                },
            }
        }

        // -- CRDT Sync --
        Request::SyncPush {
            space_id,
            changes,
            // `ucan_token` is now dead on the wire for SyncPush — the gate
            // authenticated this request against the cached UCAN from
            // Announce. Keeping the destructure-ignore avoids a protocol
            // break; a follow-up removes the field from `Request::SyncPush`.
            ..
        } => {
            // The gate proved this peer Announced, holds a valid UCAN whose
            // audience matches `verified_did`, has at least SyncPush's
            // capability for this space, and is still an active member.
            let validated = gate_ucan
                .as_ref()
                .expect("non-bypass SyncPush must have ValidatedUcan from gate");

            // Parse changes JSON into Vec<LocalColumnChange>
            let local_changes: Vec<LocalColumnChange> = match serde_json::from_value(changes) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("[SpaceDelivery] SyncPush: failed to parse changes: {e}");
                    return Response::Error {
                        message: format!("Invalid changes JSON: {e}"),
                    };
                }
            };

            // Single authorisation entry point — handles capability,
            // membership, payload validation, origin attribution and
            // per-row ownership in one place. See
            // `super::inbound_sync::authorize_inbound_sync_push` for the
            // full pipeline.
            let local_changes = match super::super::inbound_sync::authorize_inbound_sync_push(
                &state.db,
                &space_id,
                peer_endpoint_id,
                validated,
                local_changes,
            ) {
                super::super::inbound_sync::InboundSyncPushOutcome::Accepted { changes } => changes,
                super::super::inbound_sync::InboundSyncPushOutcome::Rejected { reason } => {
                    eprintln!("[SpaceDelivery] SyncPush REJECTED: {reason}");
                    return Response::Error { message: reason };
                }
            };

            // Post-validation no-op: payload was empty (or contained only
            // client-supplied authored_by_did claims that the validator
            // strips). Nothing to apply, nothing to notify.
            if local_changes.is_empty() {
                return Response::Ok;
            }

            // 2. Convert to RemoteColumnChange (HLC is the grouping key)
            let remote_changes: Vec<RemoteColumnChange> = local_changes
                .iter()
                .map(super::super::sync_loop::local_to_remote_change)
                .collect();

            // Collect affected table names and max HLC before applying
            let affected_tables: Vec<String> = local_changes
                .iter()
                .map(|c| c.table_name.clone())
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .collect();

            // 3. Apply changes to DB (HLC clock is advanced internally).
            //    Previous code locked HLC with `.lock().ok().map(...)` and
            //    passed `None` on poison — that would apply remote changes
            //    WITHOUT advancing the LEADER-LOCAL HLC clock at all.
            //    `lock_or_fail` propagates a banner-visible failure instead.
            //
            //    NOTE: `state.hlc` here is the LeaderState's clone of
            //    `HlcService` (see commands.rs::local_delivery_start —
            //    `LeaderState { hlc: Arc::new(Mutex::new(hlc_clone)), ... }`).
            //    `advance_past_remote` therefore only updates the leader's
            //    local copy; the global `AppState.hlc` consumed by ordinary
            //    Tauri commands is advanced separately through the per-pull
            //    paths (see crdt::commands::apply_remote_changes_in_transaction
            //    and sync_loop::run_sync_cycle, which both lock AppState.hlc
            //    directly). This split is pre-existing architecture and is
            //    why a poison on EITHER clock independently produces a
            //    banner row at its own location.
            let app_state = state.app_handle.state::<crate::AppState>();
            // Clone the HlcService out under the lock so the guard is
            // dropped before the `.await` below — MutexGuard is `!Send`
            // and would otherwise break the `tokio::spawn` Send bound.
            let hlc_service = match app_state.lock_or_fail(
                &state.hlc,
                CriticalFailureCode::HlcMutexPoisoned,
                "space_delivery::local::leader::handle_delivery_request::sync_push_apply",
                serde_json::json!({}),
            ) {
                Ok(g) => g.clone(),
                Err(e) => {
                    return Response::Error {
                        message: format!("Failed to lock HLC for SyncPush apply: {e}"),
                    };
                }
            };
            if let Err(e) =
                apply_remote_changes_to_db(&state.db, remote_changes, None, Some(&hlc_service))
            {
                eprintln!("[SpaceDelivery] SyncPush: failed to apply changes: {e}");
                return Response::Error {
                    message: format!("Failed to apply changes: {e}"),
                };
            }

            notify_others_sync(state, &space_id, &affected_tables, peer_endpoint_id).await;

            // If the push touched haex_space_devices, reload allowed_peers now —
            // synchronously, before returning Ok. This ensures the new peer is
            // authorized before it can issue any peer-storage requests. The async
            // TS event chain (local-sync-completed → peer_storage_reload_shares)
            // runs in parallel but this Rust-side reload is the authoritative gate.
            if affected_tables.iter().any(|t| t == "haex_space_devices") {
                let app_state: tauri::State<'_, crate::AppState> = state.app_handle.state();
                let endpoint = app_state.peer_storage.read().await;
                if let Err(e) =
                    crate::peer_storage::commands::reload_allowed_peers(&app_state, &endpoint).await
                {
                    eprintln!("[SpaceDelivery] Failed to reload allowed_peers after space_devices push: {e}");
                    return Response::Error {
                        message: format!("Failed to reload allowed_peers: {e}"),
                    };
                }
            }

            // Notify the leader's own frontend so UI stores (file browser peer
            // list, space devices) reload without waiting for the next cloud pull.
            // emit_to(label, …) keeps the event out of extension webviews.
            let _ = state.app_handle.emit_to(
                "main",
                "local-sync-completed",
                serde_json::json!({
                    "spaceId": &space_id,
                    "tables": &affected_tables,
                }),
            );

            Response::Ok
        }

        Request::SyncPull {
            space_id,
            after_timestamp,
            // `ucan_token` is now redundant on the wire — the gate
            // authenticated this request against the cached UCAN. Kept
            // as `..` to avoid a protocol break this PR.
            ..
        } => {
            // The gate proved Read+ capability and active membership.
            // `validated` is held only for the success-path audit log below
            // (`audience=…`); no further auth decision is made here.
            let validated = gate_ucan
                .as_ref()
                .expect("non-bypass SyncPull must have ValidatedUcan from gate");

            let device_id = "leader";
            // Origin filter is push-only (sync_loop). When *serving* a pull
            // the leader is the source of truth and must hand out every row
            // it has for this space, regardless of who originally wrote it.
            match scan_space_scoped_tables_for_local_changes(
                &state.db,
                &space_id,
                after_timestamp.as_deref(),
                device_id,
                None,
            ) {
                Ok(changes) => {
                    // Paginate at whole-HLC-group boundaries (uniform with the
                    // owner path) so a transaction larger than the legacy 10 MB
                    // wire cap still traverses the wire, one page per cycle. The
                    // cursor stays HLC-only: the client resumes at the page's MAX
                    // HLC, and HLC is unique per source transaction.
                    let (page, has_more) = paginate_changes(changes, PULL_PAGE_BUDGET);
                    let by_table: std::collections::BTreeMap<&str, usize> =
                        page.iter()
                            .fold(std::collections::BTreeMap::new(), |mut acc, c| {
                                *acc.entry(c.table_name.as_str()).or_insert(0) += 1;
                                acc
                            });
                    // Per-cycle telemetry — stderr-only to avoid the same
                    // haex_logs feedback loop fixed for log_sync in 8b5664db.
                    // Every served SyncPull would otherwise write one new
                    // haex_logs row, which the next cycle pushes back through
                    // CRDT sync — growing the batch monotonically across cycles.
                    eprintln!(
                        "[SyncPull] served: space={} audience={} count={} has_more={} after={:?} tables={:?}",
                        &space_id[..8.min(space_id.len())],
                        &validated.audience[..24.min(validated.audience.len())],
                        page.len(),
                        has_more,
                        after_timestamp.as_deref(),
                        by_table,
                    );
                    match serde_json::to_value(&page) {
                        Ok(json) => Response::SyncChanges {
                            changes: json,
                            has_more,
                        },
                        Err(e) => {
                            eprintln!("[SpaceDelivery] SyncPull: failed to serialize changes: {e}");
                            Response::Error {
                                message: format!("Failed to serialize changes: {e}"),
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("[SpaceDelivery] SyncPull: failed to scan changes: {e}");
                    crate::logging::log_to_db(
                        state.log_sink.as_ref(),
                        "error",
                        "SyncPull",
                        &format!(
                            "scan failed: space={} err={}",
                            &space_id[..8.min(space_id.len())],
                            e
                        ),
                        None,
                    );
                    Response::Error {
                        message: format!("Failed to scan changes: {e}"),
                    }
                }
            }
        }

        // Owner-mesh-only pending-column recovery. No serving handler exists
        // on the space path yet — it lands in a later task that wires it
        // exclusively behind the owner gate (full-vault exposure). A foreign
        // peer that reaches this arm must get the "no handler" Error, not a
        // partial response.
        Request::SyncPullColumns { .. } => Response::Error {
            message: "SyncPullColumns is not served on the space path".to_string(),
        },

        // -- Invites (ClaimInvite) --
        req @ Request::ClaimInvite { .. } => handle_claim_invite(state, req, verified_did).await,

        // -- Push Invites (peer-to-peer, invitee side) --
        Request::PushInvite {
            space_id,
            space_name,
            space_type,
            token_id,
            capabilities,
            include_history,
            inviter_did,
            inviter_label,
            inviter_avatar,
            inviter_avatar_options,
            space_endpoints,
            origin_url,
            expires_at: _,
            inviter_relay_url,
        } => push_invite::handle_push_invite(
            &state.db,
            &state.hlc,
            &state.app_handle,
            &space_id,
            &space_name,
            &space_type,
            &token_id,
            &capabilities,
            include_history,
            &inviter_did,
            inviter_label.as_deref(),
            inviter_avatar.as_deref(),
            inviter_avatar_options.as_deref(),
            &space_endpoints,
            origin_url.as_deref(),
            inviter_relay_url.as_deref(),
            verified_did,
        ),
        Request::MlsAckCommit {
            space_id,
            message_ids,
        } => {
            let did = verified_did.to_string();

            match buffer::ack_commits(&state.db, &space_id, &did, &message_ids) {
                Ok(fully_acked) => {
                    if !fully_acked.is_empty() {
                        eprintln!(
                            "[SpaceDelivery] Commits fully acked, cleaning up {} messages",
                            fully_acked.len()
                        );
                        let _ = buffer::cleanup_acked_commits(&state.db, &space_id, &fully_acked);
                    }
                    Response::Ok
                }
                Err(e) => Response::Error {
                    message: e.to_string(),
                },
            }
        }

        Request::RequestRejoin {
            space_id,
            // `ucan_token` is now redundant on the wire — the gate
            // authenticated this request against the cached UCAN.
            ..
        } => {
            // Gate-wire-up regression guard: this arm has no downstream
            // consumer of `validated_ucan`, but we still assert the gate
            // produced one so a future refactor that loses the dispatcher's
            // gate call panics loudly here instead of silently leaking
            // GroupInfo to unauthenticated peers.
            let _ = gate_ucan
                .as_ref()
                .expect("non-bypass RequestRejoin must have ValidatedUcan from gate");

            // Export current GroupInfo with ratchet tree for External Commit
            match crate::mls::blocking::get_group_info(state.db.0.clone(), space_id.clone()).await {
                Ok(group_info_bytes) => Response::GroupInfo {
                    group_info: base64_encode(&group_info_bytes),
                },
                Err(e) => Response::Error {
                    message: format!("Failed to export GroupInfo: {e}"),
                },
            }
        }

        Request::SubmitExternalCommit {
            space_id,
            commit,
            // `ucan_token` is now redundant on the wire — the gate
            // authenticated this request against the cached UCAN.
            ..
        } => {
            // Gate-wire-up regression guard: this arm has no downstream
            // consumer of `validated_ucan`, but we still assert the gate
            // produced one so a future refactor that loses the dispatcher's
            // gate call panics loudly here instead of silently storing an
            // MLS commit attributed to an unauthenticated DID.
            let _ = gate_ucan
                .as_ref()
                .expect("non-bypass SubmitExternalCommit must have ValidatedUcan from gate");
            // `peer_did` is sourced from the connection-bound verified_did,
            // not from the UCAN audience. The gate guarantees they're equal.
            let peer_did = verified_did.to_string();

            let commit_blob = match base64_decode(&commit) {
                Ok(b) => b,
                Err(_) => {
                    return Response::Error {
                        message: "Invalid base64 in commit".to_string(),
                    };
                }
            };

            // Apply the External Commit to the leader's own MLS group so the
            // leader advances to the new epoch. Without this the leader stays
            // at the old epoch permanently and every subsequent RequestRejoin
            // hands out a GroupInfo for the stale epoch, causing the peer to
            // loop: rejoin → new epoch-N message stored → can't process → rejoin…
            if let Err(e) = crate::mls::blocking::process_message(
                state.db.0.clone(),
                space_id.clone(),
                commit_blob.clone(),
            )
            .await
            {
                eprintln!(
                    "[SpaceDelivery] External commit: leader MLS process failed for space {space_id}: {e}"
                );
                // Non-fatal: still store and distribute; the leader's local MLS
                // state may already be ahead (duplicate submit) or the commit may
                // be for a newer epoch the leader hasn't reached yet.
            }

            // Store the external commit as a regular MLS message
            match buffer::store_message(&state.db, &space_id, &peer_did, "commit", &commit_blob) {
                Ok(msg_id) => {
                    // Track pending ACKs from all space members
                    let expected_dids =
                        buffer::get_space_member_dids(&state.db, &space_id).unwrap_or_default();
                    if !expected_dids.is_empty() {
                        let _ = buffer::store_pending_commit(
                            &state.db,
                            &space_id,
                            msg_id,
                            &expected_dids,
                        );
                    }

                    notify_all_mls(state, &space_id, "commit").await;

                    eprintln!(
                        "[SpaceDelivery] External commit accepted for space {space_id} (msg_id={msg_id})"
                    );
                    // Return the stored message ID so the peer can advance its
                    // MLS cursor past the External Commit itself, preventing the
                    // next cycle from fetching and failing to process it.
                    Response::MessageStored { message_id: msg_id }
                }
                Err(e) => Response::Error {
                    message: format!("Failed to store external commit: {e}"),
                },
            }
        }

        Request::MlsKeyPackageCount { space_id } => {
            let did = verified_did.to_string();
            match buffer::count_key_packages_for_did(&state.db, &space_id, &did) {
                Ok(available) => {
                    let needed = TARGET_KEY_PACKAGES_PER_PEER.saturating_sub(available);
                    Response::KeyPackageCount { available, needed }
                }
                Err(e) => Response::Error {
                    message: e.to_string(),
                },
            }
        }
    }
}

/// Encode and send a response on the QUIC send stream, then finish.
pub(crate) async fn send_response(
    send: &mut iroh::endpoint::SendStream,
    response: &Response,
) -> Result<(), DeliveryError> {
    let bytes = protocol::encode(response).map_err(|e| DeliveryError::ProtocolError {
        reason: format!("Failed to encode response: {e}"),
    })?;
    send.write_all(&bytes)
        .await
        .map_err(|e| DeliveryError::ProtocolError {
            reason: format!("Failed to send response: {e}"),
        })?;
    send.finish().map_err(|e| DeliveryError::ProtocolError {
        reason: format!("Failed to finish send: {e}"),
    })?;
    Ok(())
}
