//! PushInvite handler: receives push invites on the invitee side.
//!
//! Creates a dummy space (status='pending') and a pending invite entry.
//! Uses `execute_with_crdt` / `select_with_crdt` since both tables are CRDT-synced.

use std::sync::Arc;
use std::sync::Mutex;

use tauri::{AppHandle, Emitter};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::crdt::hlc::HlcService;
use crate::database::core;
use crate::database::DbConnection;

use super::protocol::Response;

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
    // 1. Validate capabilities — reject if empty or containing unknown values
    if capabilities.is_empty() {
        return Response::Error {
            message: "Invite has no capabilities".to_string(),
        };
    }
    for cap in capabilities {
        if !VALID_CAPABILITIES.contains(&cap.as_str()) {
            return Response::Error {
                message: format!("Unknown capability: {cap}"),
            };
        }
    }

    // 2. Check invite policy
    if !check_invite_policy(db, inviter_did) {
        return Response::PushInviteAck { accepted: false };
    }

    // 3. Lock HLC for CRDT-synced writes
    let hlc_guard = match hlc.lock() {
        Ok(guard) => guard,
        Err(_) => return Response::PushInviteAck { accepted: false },
    };

    // 4. Create dummy space with status 'pending'
    let _ = core::execute_with_crdt(
        "INSERT OR IGNORE INTO haex_spaces (id, type, status, name, origin_url, role) \
         VALUES (?1, ?2, 'pending', ?3, ?4, ?5)"
            .to_string(),
        vec![
            serde_json::Value::String(space_id.to_string()),
            serde_json::Value::String(space_type.to_string()),
            serde_json::Value::String(space_name.to_string()),
            origin_url.map_or(serde_json::Value::Null, |u| {
                serde_json::Value::String(u.to_string())
            }),
            serde_json::Value::String(capabilities.join(",")),
        ],
        db,
        &hlc_guard,
    );

    // 5. Create pending invite entry
    let invite_id = Uuid::new_v4().to_string();
    let now = OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default();
    let capabilities_json =
        serde_json::to_string(capabilities).unwrap_or_else(|_| "[]".to_string());
    let endpoints_json =
        serde_json::to_string(space_endpoints).unwrap_or_else(|_| "[]".to_string());

    let _ = core::execute_with_crdt(
        "INSERT OR IGNORE INTO haex_pending_invites \
         (id, space_id, inviter_did, inviter_label, capabilities, include_history, token_id, space_endpoints, status, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'pending', ?9)"
            .to_string(),
        vec![
            serde_json::Value::String(invite_id),
            serde_json::Value::String(space_id.to_string()),
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
    );

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
        // TODO: "contacts_only" needs DID→publicKey resolution to check against haex_contacts
        _ => true,
    }
}
