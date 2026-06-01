//! Invite token management: creation, validation, DB persistence.
//!
//! Tokens are CRDT-synced (`haex_invite_tokens` — no `_no_sync` suffix) so all devices
//! can validate ClaimInvite when elected leader. Uses `execute_with_crdt` / `select_with_crdt`.

use std::sync::Arc;
use std::sync::Mutex;

use time::OffsetDateTime;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::crdt::hlc::HlcService;
use crate::database::core;
use crate::database::DbConnection;

use super::error::DeliveryError;

/// A local invite token created by the admin.
#[derive(Debug, Clone)]
pub struct LocalInviteToken {
    pub id: String,
    pub space_id: String,
    /// If Some, only this DID can claim (contact invite). If None, anyone can (conference).
    pub target_did: Option<String>,
    /// Pre-created UCAN for contact invites (target_did is known).
    pub pre_created_ucan: Option<String>,
    pub capability: String,
    /// Whether to include space history when the invitee joins.
    pub include_history: bool,
    pub max_uses: u32,
    pub current_uses: u32,
    pub expires_at: OffsetDateTime,
    pub created_at: OffsetDateTime,
}

impl LocalInviteToken {
    pub fn is_valid(&self) -> bool {
        self.current_uses < self.max_uses && OffsetDateTime::now_utc() < self.expires_at
    }

    pub fn can_claim(&self, did: &str) -> bool {
        self.is_valid() && self.target_did.as_ref().map_or(true, |t| t == did)
    }
}

// ============================================================================
// Token creation
// ============================================================================

/// Create a contact invite token with a pre-created UCAN.
///
/// The target DID is known upfront, so the UCAN is created immediately.
pub fn create_contact_invite_token(
    db: &DbConnection,
    hlc: &Arc<Mutex<HlcService>>,
    invite_tokens: &Arc<RwLock<Vec<LocalInviteToken>>>,
    space_id: &str,
    target_did: &str,
    capability: &str,
    expires_in_seconds: u64,
    include_history: bool,
    pre_created_ucan: String,
) -> Result<String, DeliveryError> {
    let now = OffsetDateTime::now_utc();
    let expires_at = now + time::Duration::seconds(expires_in_seconds as i64);
    let token_id = Uuid::new_v4().to_string();

    let token = LocalInviteToken {
        id: token_id.clone(),
        space_id: space_id.to_string(),
        target_did: Some(target_did.to_string()),
        pre_created_ucan: Some(pre_created_ucan),
        capability: capability.to_string(),
        include_history,
        max_uses: 1,
        current_uses: 0,
        expires_at,
        created_at: now,
    };

    // Persist to CRDT-synced DB
    persist_invite_token(db, hlc, &token)?;

    // Also keep in memory for fast validation
    let tokens = invite_tokens.clone();
    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            tokens.write().await.push(token);
        })
    });

    Ok(token_id)
}

/// Create a conference invite token (no target DID, no pre-created UCAN).
///
/// The UCAN will be created at claim time when the claimer's DID is known.
pub async fn create_conference_invite_token(
    db: &DbConnection,
    hlc: &Arc<Mutex<HlcService>>,
    invite_tokens: &Arc<RwLock<Vec<LocalInviteToken>>>,
    space_id: &str,
    capability: &str,
    max_uses: u32,
    expires_in_seconds: u64,
    include_history: bool,
) -> Result<String, DeliveryError> {
    let now = OffsetDateTime::now_utc();
    let expires_at = now + time::Duration::seconds(expires_in_seconds as i64);
    let token_id = Uuid::new_v4().to_string();

    let token = LocalInviteToken {
        id: token_id.clone(),
        space_id: space_id.to_string(),
        target_did: None,
        pre_created_ucan: None,
        capability: capability.to_string(),
        include_history,
        max_uses,
        current_uses: 0,
        expires_at,
        created_at: now,
    };

    // Persist to CRDT-synced DB
    persist_invite_token(db, hlc, &token)?;

    // Also keep in memory for fast validation
    invite_tokens.write().await.push(token);
    Ok(token_id)
}

// ============================================================================
// Token validation
// ============================================================================

/// Read-only check that the token is claimable by this DID. Returns
/// (capability, Option<pre-created UCAN>) without mutating `current_uses`.
///
/// Caller must invoke [`consume_invite`] **after** the claim has fully
/// succeeded (MLS add_member, welcome buffered, response ready) so that a
/// mid-flight crash or dropped network response does not permanently burn
/// a `max_uses == 1` contact invite.
///
/// Checks in-memory tokens first, then falls back to DB lookup for tokens
/// created by other flows (e.g. queueQuicInviteAsync via Drizzle).
pub async fn validate_invite(
    db: &DbConnection,
    invite_tokens: &Arc<RwLock<Vec<LocalInviteToken>>>,
    token_id: &str,
    claimer_did: &str,
) -> Result<(String, Option<String>), DeliveryError> {
    let mut tokens = invite_tokens.write().await;

    // Try in-memory first; if not found, look up this specific token from DB
    if !tokens.iter().any(|t| t.id == token_id) {
        if let Some(db_token) = load_invite_token_by_id(db, token_id)? {
            tokens.push(db_token);
        }
    }

    let token = tokens
        .iter()
        .find(|t| t.id == token_id)
        .ok_or_else(|| DeliveryError::AccessDenied {
            reason: "Invalid invite token".to_string(),
        })?;

    if !token.can_claim(claimer_did) {
        return Err(DeliveryError::AccessDenied {
            reason: if !token.is_valid() {
                "Invite token expired or exhausted".to_string()
            } else {
                "This invite is not for your DID".to_string()
            },
        });
    }

    Ok((token.capability.clone(), token.pre_created_ucan.clone()))
}

/// Increment `current_uses` on the token and persist it. Call **only after**
/// the claim flow has fully succeeded.
pub async fn consume_invite(
    db: &DbConnection,
    hlc: &Arc<Mutex<HlcService>>,
    invite_tokens: &Arc<RwLock<Vec<LocalInviteToken>>>,
    token_id: &str,
) -> Result<(), DeliveryError> {
    let mut tokens = invite_tokens.write().await;

    let token = tokens
        .iter_mut()
        .find(|t| t.id == token_id)
        .ok_or_else(|| DeliveryError::AccessDenied {
            reason: "Invalid invite token".to_string(),
        })?;

    token.current_uses += 1;
    let current_uses = token.current_uses;

    // Persist updated usage count to DB (CRDT-synced)
    update_token_usage(db, hlc, token_id, current_uses)?;
    Ok(())
}

// ============================================================================
// DB persistence (CRDT-synced via execute_with_crdt / select_with_crdt)
// ============================================================================

/// Persist an invite token to the CRDT-synced haex_invite_tokens table.
fn persist_invite_token(
    db: &DbConnection,
    hlc: &Arc<Mutex<HlcService>>,
    token: &LocalInviteToken,
) -> Result<(), DeliveryError> {
    let hlc_guard = hlc.lock().map_err(|_| DeliveryError::Database {
        reason: "Failed to lock HLC service".to_string(),
    })?;
    let caps_json = format!("[\"{}\"]", token.capability);
    let expires_str = token
        .expires_at
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default();
    let created_str = token
        .created_at
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default();

    core::execute_with_crdt(
        "INSERT OR REPLACE INTO haex_invite_tokens \
         (id, space_id, target_did, capabilities, pre_created_ucan, include_history, max_uses, current_uses, expires_at, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)"
            .to_string(),
        vec![
            serde_json::Value::String(token.id.clone()),
            serde_json::Value::String(token.space_id.clone()),
            token.target_did.as_ref().map_or(serde_json::Value::Null, |d| {
                serde_json::Value::String(d.clone())
            }),
            serde_json::Value::String(caps_json),
            token.pre_created_ucan.as_ref().map_or(serde_json::Value::Null, |u| {
                serde_json::Value::String(u.clone())
            }),
            serde_json::Value::Number(serde_json::Number::from(token.include_history as i32)),
            serde_json::Value::Number(serde_json::Number::from(token.max_uses)),
            serde_json::Value::Number(serde_json::Number::from(token.current_uses)),
            serde_json::Value::String(expires_str),
            serde_json::Value::String(created_str),
        ],
        db,
        &hlc_guard,
    )
    .map_err(|e| DeliveryError::Database {
        reason: e.to_string(),
    })?;
    Ok(())
}

/// Load all invite tokens from DB for a space.
pub fn load_invite_tokens(
    db: &DbConnection,
    space_id: &str,
) -> Result<Vec<LocalInviteToken>, DeliveryError> {
    let rows = core::select_with_crdt(
        "SELECT id, space_id, target_did, capabilities, pre_created_ucan, include_history, \
         max_uses, current_uses, expires_at, created_at \
         FROM haex_invite_tokens WHERE space_id = ?1"
            .to_string(),
        vec![serde_json::Value::String(space_id.to_string())],
        db,
    )
    .map_err(|e| DeliveryError::Database {
        reason: e.to_string(),
    })?;

    let mut tokens = Vec::new();
    for row in rows {
        let id = row.get(0).and_then(|v| v.as_str()).unwrap_or_default().to_string();
        let space_id = row.get(1).and_then(|v| v.as_str()).unwrap_or_default().to_string();
        let target_did = row.get(2).and_then(|v| v.as_str()).map(|s| s.to_string());
        let caps_json = row.get(3).and_then(|v| v.as_str()).unwrap_or("[]");
        let capability = serde_json::from_str::<Vec<String>>(caps_json)
            .ok()
            .and_then(|v| v.into_iter().next())
            .unwrap_or_else(|| "space/read".to_string());
        let pre_created_ucan = row.get(4).and_then(|v| v.as_str()).map(|s| s.to_string());
        let include_history = row.get(5).and_then(|v| v.as_i64()).unwrap_or(0) != 0;
        let max_uses = row.get(6).and_then(|v| v.as_u64()).unwrap_or(1) as u32;
        let current_uses = row.get(7).and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        let expires_str = row.get(8).and_then(|v| v.as_str()).unwrap_or_default();
        let created_str = row.get(9).and_then(|v| v.as_str()).unwrap_or_default();

        let expires_at = time::OffsetDateTime::parse(
            expires_str,
            &time::format_description::well_known::Rfc3339,
        )
        .unwrap_or_else(|_| OffsetDateTime::now_utc());
        let created_at = time::OffsetDateTime::parse(
            created_str,
            &time::format_description::well_known::Rfc3339,
        )
        .unwrap_or_else(|_| OffsetDateTime::now_utc());

        tokens.push(LocalInviteToken {
            id,
            space_id,
            target_did,
            pre_created_ucan,
            capability,
            include_history,
            max_uses,
            current_uses,
            expires_at,
            created_at,
        });
    }
    Ok(tokens)
}

/// Load a single invite token by ID from the DB.
fn load_invite_token_by_id(
    db: &DbConnection,
    token_id: &str,
) -> Result<Option<LocalInviteToken>, DeliveryError> {
    let rows = core::select_with_crdt(
        "SELECT id, space_id, target_did, capabilities, pre_created_ucan, include_history, \
         max_uses, current_uses, expires_at, created_at \
         FROM haex_invite_tokens WHERE id = ?1"
            .to_string(),
        vec![serde_json::Value::String(token_id.to_string())],
        db,
    )
    .map_err(|e| DeliveryError::Database {
        reason: e.to_string(),
    })?;

    let row = match rows.first() {
        Some(r) => r,
        None => return Ok(None),
    };

    let id = row.get(0).and_then(|v| v.as_str()).unwrap_or_default().to_string();
    let space_id = row.get(1).and_then(|v| v.as_str()).unwrap_or_default().to_string();
    let target_did = row.get(2).and_then(|v| v.as_str()).map(|s| s.to_string());
    let caps_json = row.get(3).and_then(|v| v.as_str()).unwrap_or("[]");
    let capability = serde_json::from_str::<Vec<String>>(caps_json)
        .ok()
        .and_then(|v| v.into_iter().next())
        .unwrap_or_else(|| "space/read".to_string());
    let pre_created_ucan = row.get(4).and_then(|v| v.as_str()).map(|s| s.to_string());
    let include_history = row.get(5).and_then(|v| v.as_i64()).unwrap_or(0) != 0;
    let max_uses = row.get(6).and_then(|v| v.as_u64()).unwrap_or(1) as u32;
    let current_uses = row.get(7).and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    let expires_str = row.get(8).and_then(|v| v.as_str()).unwrap_or_default();
    let created_str = row.get(9).and_then(|v| v.as_str()).unwrap_or_default();

    let expires_at = time::OffsetDateTime::parse(
        expires_str,
        &time::format_description::well_known::Rfc3339,
    )
    .unwrap_or_else(|_| OffsetDateTime::now_utc());
    let created_at = time::OffsetDateTime::parse(
        created_str,
        &time::format_description::well_known::Rfc3339,
    )
    .unwrap_or_else(|_| OffsetDateTime::now_utc());

    Ok(Some(LocalInviteToken {
        id,
        space_id,
        target_did,
        pre_created_ucan,
        capability,
        include_history,
        max_uses,
        current_uses,
        expires_at,
        created_at,
    }))
}

/// Update token usage count in DB after claim.
fn update_token_usage(
    db: &DbConnection,
    hlc: &Arc<Mutex<HlcService>>,
    token_id: &str,
    current_uses: u32,
) -> Result<(), DeliveryError> {
    let hlc_guard = hlc.lock().map_err(|_| DeliveryError::Database {
        reason: "Failed to lock HLC service".to_string(),
    })?;
    core::execute_with_crdt(
        "UPDATE haex_invite_tokens SET current_uses = ?1 WHERE id = ?2".to_string(),
        vec![
            serde_json::Value::Number(serde_json::Number::from(current_uses)),
            serde_json::Value::String(token_id.to_string()),
        ],
        db,
        &hlc_guard,
    )
    .map_err(|e| DeliveryError::Database {
        reason: e.to_string(),
    })?;
    Ok(())
}

#[cfg(test)]
mod target_did_anti_manipulation_tests {
    //! Freeze the Phase-2 anti-manipulation invariant for `target_did`
    //! (plan §4.3, test T10).
    //!
    //! ## The invariant
    //!
    //! The `target_did` value that gates a ClaimInvite is read **only** from
    //! the in-memory `invite_tokens` cache or from `haex_invite_tokens` (the
    //! CRDT-synced DB row created by the inviter). It is **never** taken
    //! from a request payload, never from a link-payload hint, and never
    //! from any CRDT push that has not been authored under the leader's
    //! UCAN.
    //!
    //! A future refactor that accidentally accepts a `target_did` parameter
    //! at the validation surface — or that derives `target_did` from a
    //! `LocalInviteToken` instance produced by the caller — would silently
    //! re-open the entire ClaimInvite spoofing surface that C5 closed. The
    //! source-text guards below lock the calling convention in place.
    //!
    //! ## Why source-text, not behavioural
    //!
    //! `validate_invite` requires an `Arc<RwLock<Vec<LocalInviteToken>>>`
    //! and a `DbConnection`; a behavioural test of "this function never
    //! consults a parameter target_did" is moot when the function does not
    //! take such a parameter to begin with — the invariant is structural.
    //! End-to-end T10 (Targeted-Invite where a link-hint disagrees with the
    //! DB token) lives in the haex-e2e-tests companion as
    //! `invitations/targeted-invite-did-mismatch`.

    /// `validate_invite` must read its `target_did` from the token row, not
    /// from a caller-supplied parameter. The function takes `claimer_did`
    /// (the DID that wants to claim, gated upstream by the Phase-2
    /// quic_did_auth handshake) — the `target_did` comparison must happen
    /// against the loaded `LocalInviteToken::target_did` via `can_claim`.
    /// Any signature change that introduces a `target_did` parameter on the
    /// validation surface trips this guard.
    #[test]
    fn validate_invite_does_not_accept_target_did_parameter() {
        let source = include_str!("invite_tokens.rs");
        let production = source
            .split_once("#[cfg(test)]")
            .map(|(p, _)| p)
            .unwrap_or(source);

        // Isolate the validate_invite parameter list (between the opening
        // paren of `pub async fn validate_invite(` and the closing paren).
        // The check is intentionally scoped to this function: token-creation
        // helpers like `create_contact_invite_token` do legitimately take a
        // `target_did: &str` parameter — that is the inviter choosing the
        // claimant — and must not collide with the validation surface guard.
        let after_sig = production
            .split_once("pub async fn validate_invite(")
            .expect("validate_invite signature missing")
            .1;
        let params = after_sig
            .split_once(") -> Result")
            .expect("validate_invite return-type missing")
            .0;

        assert!(
            params.contains("claimer_did: &str,"),
            "validate_invite must accept the claimer DID (gated by Phase-2 \
             handshake) as a parameter"
        );
        assert!(
            !params.contains("target_did"),
            "validate_invite must NOT take a target_did parameter in any \
             shape — target DID is authoritative from haex_invite_tokens, \
             never from a caller-supplied value (plan §4.3). Params seen: \
             {params}"
        );
    }

    /// The claim-side comparison must run against the persisted token row,
    /// not against caller-supplied data. `can_claim` is the only place that
    /// dereferences `self.target_did`; removing the `can_claim` invocation
    /// from `validate_invite` would silently accept every claim regardless
    /// of `target_did`.
    #[test]
    fn validate_invite_gates_target_did_via_loaded_token_can_claim() {
        let source = include_str!("invite_tokens.rs");
        let production = source
            .split_once("#[cfg(test)]")
            .map(|(p, _)| p)
            .unwrap_or(source);

        assert!(
            production.contains("token.can_claim(claimer_did)"),
            "validate_invite must gate the claim through \
             `token.can_claim(claimer_did)` so target_did comes from the \
             loaded haex_invite_tokens row, not from any caller-supplied \
             value"
        );
        assert!(
            production
                .contains("self.target_did.as_ref().map_or(true, |t| t == did)"),
            "LocalInviteToken::can_claim must compare claimer DID against \
             the row's own target_did field — the entire authoritative \
             chain hinges on this exact comparison"
        );
    }
}
