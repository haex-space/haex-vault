//! PushInvite handler: receives push invites on the invitee side.
//!
//! Creates a pending invite entry with embedded space metadata.
//! Uses `execute_with_crdt` / `select_with_crdt` since the table is CRDT-synced.

use std::sync::Arc;
use std::sync::Mutex;

use tauri::{AppHandle, Emitter};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::crdt::hlc::HlcService;
use crate::database::core;
use crate::database::DbConnection;
use crate::logging;

use super::protocol::Response;

const LOG_SOURCE: &str = "PushInvite";

const VALID_CAPABILITIES: &[&str] = &["space/read", "space/write", "space/invite"];

/// Handle an incoming PushInvite request on the invitee's device.
///
/// Checks invite policy, validates capabilities, creates dummy space + pending invite, returns ack.
pub fn handle_push_invite(
    db: &DbConnection,
    hlc: &Arc<Mutex<HlcService>>,
    app_handle: &AppHandle,
    space_id: &str,
    space_name: &str,
    space_type: &str,
    token_id: &str,
    capabilities: &[String],
    include_history: bool,
    inviter_did: &str,
    inviter_label: Option<&str>,
    space_endpoints: &[String],
    origin_url: Option<&str>,
) -> Response {
    logging::log_to_db(db, hlc, "info", LOG_SOURCE, &format!(
        "Received invite for space {space_id} ({space_name}) from {inviter_did}, token={token_id}"
    ));

    // 1. Validate capabilities — reject if empty or containing unknown values
    if capabilities.is_empty() {
        logging::log_to_db(db, hlc, "warn", LOG_SOURCE, "REJECTED: no capabilities");
        return Response::Error {
            message: "Invite has no capabilities".to_string(),
        };
    }
    for cap in capabilities {
        if !VALID_CAPABILITIES.contains(&cap.as_str()) {
            logging::log_to_db(db, hlc, "warn", LOG_SOURCE, &format!("REJECTED: unknown capability {cap}"));
            return Response::Error {
                message: format!("Unknown capability: {cap}"),
            };
        }
    }

    // 2. Reject if this device already has the space as 'active' (prevents self-invites
    //    when CRDT-synced outbox entries are processed by the sender's other devices)
    let already_active = core::select_with_crdt(
        "SELECT COUNT(*) FROM haex_spaces WHERE id = ?1 AND status = 'active'".to_string(),
        vec![serde_json::Value::String(space_id.to_string())],
        db,
    )
    .ok()
    .and_then(|rows| rows.first()?.first()?.as_i64())
    .unwrap_or(0);

    if already_active > 0 {
        logging::log_to_db(db, hlc, "info", LOG_SOURCE, &format!("SKIPPED: space {space_id} already active on this device"));
        return Response::PushInviteAck { accepted: true };
    }

    // 3. Check invite policy
    if !check_invite_policy(db, inviter_did) {
        logging::log_to_db(db, hlc, "warn", LOG_SOURCE, &format!("REJECTED: invite policy blocked inviter {inviter_did}"));
        return Response::PushInviteAck { accepted: false };
    }

    // 4. Lock HLC for CRDT-synced writes
    let hlc_guard = match hlc.lock() {
        Ok(guard) => guard,
        Err(_) => {
            eprintln!("[{LOG_SOURCE}] ERROR: HLC lock poisoned");
            return Response::PushInviteAck { accepted: false };
        }
    };

    // 5. Remove older pending invites from the same inviter for this space
    let _ = core::execute_with_crdt(
        "DELETE FROM haex_pending_invites \
         WHERE space_id = ?1 AND inviter_did = ?2 AND status = 'pending'"
            .to_string(),
        vec![
            serde_json::Value::String(space_id.to_string()),
            serde_json::Value::String(inviter_did.to_string()),
        ],
        db,
        &hlc_guard,
    );

    // 6. Create pending invite with embedded space metadata.
    //    No dummy space in haex_spaces — that table is CRDT-synced and shares
    //    the same PK as the inviter's active space. Any delete/tombstone on a
    //    dummy entry would propagate and destroy the inviter's real space.
    let invite_id = Uuid::new_v4().to_string();
    let now = OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default();
    let capabilities_json =
        serde_json::to_string(capabilities).unwrap_or_else(|_| "[]".to_string());
    let endpoints_json =
        serde_json::to_string(space_endpoints).unwrap_or_else(|_| "[]".to_string());

    eprintln!("[{LOG_SOURCE}] [info] Inserting pending invite {invite_id} for space {space_id}");

    match core::execute_with_crdt(
        "INSERT OR IGNORE INTO haex_pending_invites \
         (id, space_id, space_name, space_type, origin_url, inviter_did, inviter_label, \
          capabilities, include_history, token_id, space_endpoints, status, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 'pending', ?12)"
            .to_string(),
        vec![
            serde_json::Value::String(invite_id.clone()),
            serde_json::Value::String(space_id.to_string()),
            serde_json::Value::String(space_name.to_string()),
            serde_json::Value::String(space_type.to_string()),
            origin_url.map_or(serde_json::Value::Null, |u| {
                serde_json::Value::String(u.to_string())
            }),
            serde_json::Value::String(inviter_did.to_string()),
            inviter_label.map_or(serde_json::Value::Null, |l| {
                serde_json::Value::String(l.to_string())
            }),
            serde_json::Value::String(capabilities_json),
            serde_json::Value::Number(serde_json::Number::from(include_history as i32)),
            serde_json::Value::String(token_id.to_string()),
            serde_json::Value::String(endpoints_json),
            serde_json::Value::String(now),
        ],
        db,
        &hlc_guard,
    ) {
        Ok(_) => eprintln!("[{LOG_SOURCE}] [info] SUCCESS: pending invite {invite_id} created"),
        Err(e) => eprintln!("[{LOG_SOURCE}] [error] FAILED to insert pending invite: {e}"),
    }

    // Drop HLC lock before logging to DB (log_to_db locks internally)
    drop(hlc_guard);

    logging::log_to_db(db, hlc, "info", LOG_SOURCE, &format!(
        "Invite processing complete for {invite_id} in space {space_id}"
    ));

    let _ = app_handle.emit("push-invite-received", ());

    Response::PushInviteAck { accepted: true }
}

/// Check invite policy against blocked DIDs and policy setting.
fn check_invite_policy(db: &DbConnection, inviter_did: &str) -> bool {
    // Check blocked DIDs (select_with_crdt adds WHERE haex_tombstone = 0)
    let blocked = core::select_with_crdt(
        "SELECT COUNT(*) FROM haex_blocked_dids WHERE did = ?1".to_string(),
        vec![serde_json::Value::String(inviter_did.to_string())],
        db,
    )
    .ok()
    .and_then(|rows| rows.first()?.first()?.as_i64())
    .unwrap_or(0);

    if blocked > 0 {
        return false;
    }

    // Check policy (select_with_crdt adds WHERE haex_tombstone = 0)
    let policy = core::select_with_crdt(
        "SELECT policy FROM haex_invite_policy WHERE id = 'default'".to_string(),
        vec![],
        db,
    )
    .ok()
    .and_then(|rows| rows.first()?.first()?.as_str().map(|s| s.to_string()))
    .unwrap_or_else(|| "all".to_string());

    match policy.as_str() {
        "nobody" => false,
        "contacts_only" => {
            // Check if inviter's DID matches a known contact (identity without private_key)
            let count = core::select_with_crdt(
                "SELECT COUNT(*) FROM haex_identities WHERE did = ?1 AND private_key IS NULL"
                    .to_string(),
                vec![serde_json::Value::String(inviter_did.to_string())],
                db,
            )
            .ok()
            .and_then(|rows| rows.first()?.first()?.as_i64())
            .unwrap_or(0);
            count > 0
        }
        _ => true,
    }
}
