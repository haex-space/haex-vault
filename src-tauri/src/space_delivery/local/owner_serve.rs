//! Owner-vault serving path.
//!
//! Reached only after [`crate::owner_sync::scope::owner_route_decision`] has
//! proven — from the `quic_did_auth`-verified DID and the resolved vault
//! owner/space — that the connecting peer is ANOTHER DEVICE OF THIS VAULT'S
//! OWNER targeting the vault space. For such a peer the full vault is
//! replicated (every `haex_*` CRDT table, including vault-private ones), so
//! this path deliberately does NOT run the space-scoped UCAN/membership
//! pipeline (`authorize_inbound_sync_push`) — that pipeline enforces per-row
//! `space_id` ownership which is meaningless owner-to-owner and would corrupt
//! full-table rows.
//!
//! # Security
//!
//! The full-vault scope produced here must NEVER reach a non-owner peer. The
//! ONLY guard is the caller's `owner_route_decision` gate; this module trusts
//! that it has been checked and performs no further identity decision. It must
//! therefore only ever be invoked from the owner branch in
//! `multi_leader::handle_stream`.

use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Manager};

use crate::crdt::commands::{apply_remote_changes_to_db, RemoteColumnChange};
use crate::crdt::hlc::HlcService;
use crate::crdt::scanner::{scan_all_crdt_tables_for_owner, LocalColumnChange};
use crate::critical::CriticalFailureCode;
use crate::database::core::with_connection;
use crate::database::init::discover_crdt_tables;
use crate::database::DbConnection;

use super::protocol::{Request, Response};
use super::sync_loop::local_to_remote_change;

/// The action the owner-serve handler will take for a given request. Extracted
/// as a pure classifier so the request-variant routing is unit-testable
/// without a live QUIC endpoint or database.
///
/// Only `SyncPull` and `SyncPush` are valid owner-sync operations; everything
/// else is rejected so an owner-classified connection can never fall through
/// into space-scoped logic via this handler.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum OwnerRequestAction {
    Pull,
    Push,
    Reject,
}

/// Classify an owner-peer request into the action this handler supports.
pub(super) fn owner_request_action(request: &Request) -> OwnerRequestAction {
    match request {
        Request::SyncPull { .. } => OwnerRequestAction::Pull,
        Request::SyncPush { .. } => OwnerRequestAction::Push,
        _ => OwnerRequestAction::Reject,
    }
}

/// Serve a single request from a peer already classified as this vault's
/// owner-on-another-device (see module docs).
///
/// - `SyncPull` → scan the FULL CRDT table set (unscoped) and return the
///   changes the same way the space `SyncPull` handler serializes them.
/// - `SyncPush` → apply the incoming changes RAW (no UCAN/membership check),
///   advancing the HLC clock via `lock_or_fail` exactly like the space path.
/// - anything else → `Response::Error` (never falls through to space logic).
///
/// `ucan_token` on the wire is ignored entirely — owner peers send `None`.
pub(super) fn handle_owner_sync_request(
    request: Request,
    db: &DbConnection,
    hlc: &Arc<Mutex<HlcService>>,
    app_handle: &AppHandle,
) -> Response {
    match owner_request_action(&request) {
        OwnerRequestAction::Pull => {
            // Destructure only the fields we use; `ucan_token` is ignored.
            let after_timestamp = match request {
                Request::SyncPull {
                    after_timestamp, ..
                } => after_timestamp,
                _ => unreachable!("owner_request_action returned Pull for a non-SyncPull request"),
            };
            handle_owner_pull(after_timestamp.as_deref(), db)
        }
        OwnerRequestAction::Push => {
            let changes = match request {
                Request::SyncPush { changes, .. } => changes,
                _ => unreachable!("owner_request_action returned Push for a non-SyncPush request"),
            };
            handle_owner_push(changes, db, hlc, app_handle)
        }
        OwnerRequestAction::Reject => Response::Error {
            message: "Owner-sync connection only serves SyncPull/SyncPush".to_string(),
        },
    }
}

/// Owner `SyncPull`: scan EVERY CRDT table (no space filter) for changes after
/// `after_timestamp` and return them. Mirrors the serialization of the
/// space-scoped `SyncPull` handler in `leader.rs`.
///
/// `pub(super)` so the real-QUIC owner-sync integration capstone
/// (`owner_sync_integration_tests.rs`) can drive the genuine pull handler from
/// its reconstructed accept loop without an `AppHandle` — the full
/// `handle_owner_sync_request` requires one only for the push path.
pub(super) fn handle_owner_pull(after_timestamp: Option<&str>, db: &DbConnection) -> Response {
    // Origin filter is push-only; when serving a pull this device is a source
    // of truth and hands out every row it has, regardless of who wrote it.
    let device_id = "leader";

    let scan_result = with_connection(db, |conn| {
        let tables = discover_crdt_tables(conn)?;
        scan_all_crdt_tables_for_owner(conn, &tables, after_timestamp, device_id, None)
    });

    match scan_result {
        Ok(changes) => match serde_json::to_value(&changes) {
            Ok(json) => Response::SyncChanges { changes: json },
            Err(e) => {
                eprintln!("[OwnerSync] SyncPull: failed to serialize changes: {e}");
                Response::Error {
                    message: format!("Failed to serialize changes: {e}"),
                }
            }
        },
        Err(e) => {
            eprintln!("[OwnerSync] SyncPull: failed to scan changes: {e}");
            Response::Error {
                message: format!("Failed to scan changes: {e}"),
            }
        }
    }
}

/// Owner `SyncPush`: apply the incoming changes RAW — no UCAN, no membership,
/// no per-row `space_id`/ownership pipeline. The HLC clock is advanced through
/// `lock_or_fail` exactly like the space `SyncPush` handler.
fn handle_owner_push(
    changes: serde_json::Value,
    db: &DbConnection,
    hlc: &Arc<Mutex<HlcService>>,
    app_handle: &AppHandle,
) -> Response {
    let local_changes: Vec<LocalColumnChange> = match serde_json::from_value(changes) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[OwnerSync] SyncPush: failed to parse changes: {e}");
            return Response::Error {
                message: format!("Invalid changes JSON: {e}"),
            };
        }
    };

    if local_changes.is_empty() {
        return Response::Ok;
    }

    let remote_changes: Vec<RemoteColumnChange> =
        local_changes.iter().map(local_to_remote_change).collect();

    // Advance the local HLC clock as part of the apply. `lock_or_fail` turns a
    // poisoned mutex into a banner-visible critical failure instead of
    // silently applying without advancing the clock.
    let app_state = app_handle.state::<crate::AppState>();
    let hlc_service = match app_state.lock_or_fail(
        hlc,
        CriticalFailureCode::HlcMutexPoisoned,
        "space_delivery::local::owner_serve::handle_owner_push",
        serde_json::json!({}),
    ) {
        Ok(g) => g.clone(),
        Err(e) => {
            return Response::Error {
                message: format!("Failed to lock HLC for owner SyncPush apply: {e}"),
            };
        }
    };

    if let Err(e) = apply_remote_changes_to_db(db, remote_changes, None, Some(&hlc_service)) {
        eprintln!("[OwnerSync] SyncPush: failed to apply changes: {e}");
        return Response::Error {
            message: format!("Failed to apply changes: {e}"),
        };
    }

    Response::Ok
}

#[cfg(test)]
#[path = "owner_serve_tests.rs"]
mod tests;
