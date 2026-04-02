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

/// Validate and consume an invite token. Returns (capability, Option<pre-created UCAN>).
pub async fn validate_and_consume_invite(
    db: &DbConnection,
    hlc: &Arc<Mutex<HlcService>>,
    invite_tokens: &Arc<RwLock<Vec<LocalInviteToken>>>,
    token_id: &str,
    claimer_did: &str,
) -> Result<(String, Option<String>), DeliveryError> {
    let mut tokens = invite_tokens.write().await;
    let token = tokens
        .iter_mut()
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

    token.current_uses += 1;
    let current_uses = token.current_uses;
    let capability = token.capability.clone();
    let pre_ucan = token.pre_created_ucan.clone();

    // Persist updated usage count to DB
    let _ = update_token_usage(db, hlc, token_id, current_uses);

    Ok((capability, pre_ucan))
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
