//! PushInvite handler: receives push invites on the invitee side.
//!
//! Creates a pending invite entry with embedded space metadata.
//! Uses `execute_with_crdt` / `select_with_crdt` since the table is CRDT-synced.

use std::sync::Arc;
use std::sync::Mutex;

use sha2::{Digest, Sha256};
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
    inviter_avatar: Option<&str>,
    inviter_avatar_options: Option<&str>,
    space_endpoints: &[String],
    origin_url: Option<&str>,
) -> Response {
    let token_fp = token_fingerprint(token_id);
    logging::log_to_db(db, hlc, "info", LOG_SOURCE, &format!(
        "Received invite for space {space_id} ({space_name}) from {inviter_did}, token={token_fp}"
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

    // 2b. Idempotency: ack without re-inserting / re-emitting if we already
    //     have a row for this token_id. Sender-side QUIC retry on transient
    //     response-read errors (see space_delivery/local/quic_retry.rs)
    //     re-delivers the same request after the receiver already processed
    //     it once — without this guard the user sees a toast per retry.
    let existing_for_token = core::select_with_crdt(
        "SELECT COUNT(*) FROM haex_pending_invites WHERE token_id = ?1".to_string(),
        vec![serde_json::Value::String(token_id.to_string())],
        db,
    )
    .ok()
    .and_then(|rows| rows.first()?.first()?.as_i64())
    .unwrap_or(0);

    if existing_for_token > 0 {
        logging::log_to_db(db, hlc, "info", LOG_SOURCE, &format!(
            "SKIPPED (duplicate token): space={space_id} token={token_fp} — already received"
        ));
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
            // Can't use log_to_db (it would also try to lock HLC). Stderr only.
            eprintln!("[{LOG_SOURCE}] [error] ABORT: HLC lock poisoned while processing invite for space {space_id} from {inviter_did}");
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

    // eprintln only — can't log_to_db while holding hlc_guard (would deadlock).
    // The DB log for this step happens post-drop with the outcome.
    eprintln!("[{LOG_SOURCE}] [info] Inserting pending invite {invite_id} for space {space_id}");

    let insert_result = core::execute_with_crdt(
        "INSERT OR IGNORE INTO haex_pending_invites \
         (id, space_id, space_name, space_type, origin_url, inviter_did, inviter_label, inviter_avatar, inviter_avatar_options, \
          capabilities, include_history, token_id, space_endpoints, status, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, 'pending', ?14) \
         RETURNING id"
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
            inviter_avatar.map_or(serde_json::Value::Null, |a| {
                serde_json::Value::String(a.to_string())
            }),
            inviter_avatar_options.map_or(serde_json::Value::Null, |o| {
                serde_json::Value::String(o.to_string())
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

    // Drop HLC lock before logging to DB (log_to_db locks internally)
    drop(hlc_guard);

    // Persist the INSERT outcome to haex_logs so production (where stderr is /dev/null)
    // can tell whether a given invite reached the row-creation stage or not.
    // With `RETURNING id`, `execute_with_crdt` yields one row on a real insert
    // and an empty Vec when `INSERT OR IGNORE` skipped a duplicate id — so
    // we can distinguish "wrote the row" from "ignored a UUID collision"
    // and avoid ACKing success when no row exists.
    let inserted_rows = match &insert_result {
        Ok(rows) => rows,
        Err(e) => {
            logging::log_to_db(db, hlc, "error", LOG_SOURCE, &format!(
                "INSERT FAILED: pending invite {invite_id} (space={space_id}, token={token_fp}): {e}"
            ));
            // Fail fast: don't emit push-invite-received or ACK success when
            // the row isn't persisted, or the UI fires a toast for an invite
            // that doesn't exist in the DB.
            return Response::PushInviteAck { accepted: false };
        }
    };

    if inserted_rows.is_empty() {
        // INSERT OR IGNORE skipped the row (duplicate id). UUID collisions
        // are astronomically unlikely, but reporting accepted=true when no
        // row was actually written would surface a phantom invite to the
        // user — so bail out explicitly.
        logging::log_to_db(db, hlc, "warn", LOG_SOURCE, &format!(
            "INSERT IGNORED (duplicate id): pending invite {invite_id} (space={space_id}, token={token_fp})"
        ));
        return Response::PushInviteAck { accepted: false };
    }

    logging::log_to_db(db, hlc, "info", LOG_SOURCE, &format!(
        "INSERT OK: pending invite {invite_id} (space={space_id}, token={token_fp})"
    ));

    logging::log_to_db(db, hlc, "info", LOG_SOURCE, &format!(
        "Invite processing complete for {invite_id} in space {space_id}"
    ));

    // Emitting is what triggers the frontend toast + invite list reload.
    // If this fails (AppHandle dead / event channel closed) the invite is
    // persisted but the user sees no notification — we MUST log the outcome
    // so this regression is traceable without shell access.
    match app_handle.emit("push-invite-received", ()) {
        Ok(()) => logging::log_to_db(db, hlc, "info", LOG_SOURCE, &format!(
            "Emitted push-invite-received for invite {invite_id} (space={space_id})"
        )),
        Err(e) => logging::log_to_db(db, hlc, "error", LOG_SOURCE, &format!(
            "FAILED to emit push-invite-received for invite {invite_id}: {e}"
        )),
    }

    Response::PushInviteAck { accepted: true }
}

/// Short, non-reversible fingerprint of a token for log diagnostics.
/// Full `token_id` is a bearer credential; persisting it in `haex_logs`
/// would make those rows as sensitive as the invite itself.
fn token_fingerprint(token_id: &str) -> String {
    let digest = Sha256::digest(token_id.as_bytes());
    format!("sha256:{}", hex::encode(&digest[..6]))
}

#[cfg(test)]
mod tests {
    use super::token_fingerprint;

    const SECRET_TOKEN: &str = "bearer-secret-abc123-xyz789-do-not-log";

    #[test]
    fn fingerprint_is_deterministic() {
        // Same token must always produce the same fingerprint so logs from
        // different hosts can be correlated without storing the raw token.
        assert_eq!(
            token_fingerprint(SECRET_TOKEN),
            token_fingerprint(SECRET_TOKEN),
        );
    }

    #[test]
    fn fingerprint_has_fixed_shape() {
        // "sha256:" prefix (7 chars) + 6 bytes hex-encoded (12 chars) = 19.
        let fp = token_fingerprint(SECRET_TOKEN);
        assert!(fp.starts_with("sha256:"), "got {fp:?}");
        assert_eq!(fp.len(), 19, "unexpected length: {fp:?}");
        // Hex-only payload after the prefix.
        assert!(
            fp["sha256:".len()..].chars().all(|c| c.is_ascii_hexdigit()),
            "payload not hex: {fp:?}",
        );
    }

    #[test]
    fn fingerprint_differs_per_input() {
        // Collision resistance for distinct inputs — ensures per-invite
        // fingerprints are actually distinguishing, not always the same.
        assert_ne!(
            token_fingerprint("token-a"),
            token_fingerprint("token-b"),
        );
    }

    #[test]
    fn fingerprint_does_not_leak_token_plaintext() {
        // Regression guard for the review finding: raw token_id must not
        // be recoverable from persisted diagnostics. A plain SHA-256 prefix
        // is non-reversible — this test asserts no substring of the token
        // leaks into the output.
        let fp = token_fingerprint(SECRET_TOKEN);
        assert!(!fp.contains(SECRET_TOKEN));
        for fragment in ["bearer", "secret", "abc123", "xyz789"] {
            assert!(
                !fp.contains(fragment),
                "fingerprint {fp:?} leaked token fragment {fragment:?}",
            );
        }
    }
}

/// Check invite policy against blocked DIDs and policy setting.
fn check_invite_policy(db: &DbConnection, inviter_did: &str) -> bool {
    // Check blocked DIDs
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

    // Check policy
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
