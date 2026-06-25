//! Identity-access enforcement primitives for principals (extensions /
//! external clients) under [`ResourceType::Identities`].
//!
//! Identities are NOT yet exposed to principals through any command — no
//! `extension_identity_*` Tauri command and no `__core__` bridge handler exists
//! today. These primitives implement the security rules from the design
//! (`docs/plans/2026-06-19-unified-principal-permissions-design.md` §6) so that
//! whenever an identity read/write path *is* added it MUST go through them:
//!
//! - **Read** ([`IdentityAction::Read`]): returns DID + name + avatar +
//!   avatar_options + notes — **never** `private_key` — for BOTH own identities
//!   (`private_key` set) and contacts (`private_key NULL`). The
//!   [`IdentityReadView`] DTO has no `private_key` field at all, so the secret
//!   cannot leak even by accident; [`project_identity_read`] is the pure mapper.
//! - **Write** ([`IdentityAction::Write`]): permits **only** inserting a NEW
//!   contact (`private_key NULL`, `source='contact'`). It must NOT create/edit
//!   owned identities, edit/delete existing rows, or ever accept a
//!   caller-supplied `private_key`. [`validate_contact_insert`] is the pure
//!   validator.
//!
//! The action-level permission check lives in
//! [`crate::extension::permissions::manager::PermissionManager::check_identities_permission`].
//!
//! These primitives are intentionally ahead of their wiring: no production
//! caller exists yet (identities aren't exposed to principals), so the items
//! are `#![allow(dead_code)]` — mirroring the `#[allow(dead_code)]` on the other
//! not-yet-called `check_*` methods. They are exercised by the unit tests and
//! are the mandatory entrypoint for any future identity read/write path.
#![allow(dead_code)]

use crate::extension::error::ExtensionError;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// The columns of a `haex_identities` row that are relevant to projection /
/// validation. A plain input struct (not the DTO returned to callers) — it
/// still carries `private_key` so the projection step is the thing that drops
/// it. `private_key` is `None` for contacts and `Some` for own identities.
#[derive(Debug, Clone, PartialEq)]
pub struct IdentityRow {
    pub id: String,
    pub did: String,
    pub name: String,
    pub source: String,
    /// `Some` for own identities, `None` for contacts. NEVER forwarded to a
    /// principal — see [`project_identity_read`].
    pub private_key: Option<String>,
    pub avatar: Option<String>,
    pub avatar_options: Option<String>,
    pub notes: Option<String>,
}

/// The `private_key`-free view of an identity returned to a principal on read.
///
/// SECURITY: this struct intentionally has **no `private_key` field**. The
/// secret cannot be serialized to a principal because there is nowhere to put
/// it — masking is enforced by the type, not by a runtime check. Covers both
/// own identities and contacts (the shape is identical; only `source` differs).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct IdentityReadView {
    pub id: String,
    pub did: String,
    pub name: String,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_options: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

/// Pure read-projection: maps a `haex_identities` row to the
/// `private_key`-free [`IdentityReadView`] a principal is allowed to see.
///
/// The `private_key` field of the input is simply never read, so the output
/// can never carry it — for own identities AND contacts alike.
pub fn project_identity_read(row: &IdentityRow) -> IdentityReadView {
    IdentityReadView {
        id: row.id.clone(),
        did: row.did.clone(),
        name: row.name.clone(),
        source: row.source.clone(),
        avatar: row.avatar.clone(),
        avatar_options: row.avatar_options.clone(),
        notes: row.notes.clone(),
    }
}

/// The fields a principal may supply when inserting a NEW contact.
///
/// There is deliberately **no `private_key` field** and **no `id` of an
/// existing row** — a principal can neither set a private key nor target an
/// existing identity. `source` is validated to be `'contact'`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ContactInsert {
    pub did: String,
    pub name: String,
    /// Must be exactly `"contact"`. Present so a caller can't smuggle
    /// `source='own'`; validated by [`validate_contact_insert`].
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_options: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

/// The expected `source` value for a contact row.
pub const CONTACT_SOURCE: &str = "contact";

/// Pure contact-only-write validator.
///
/// Returns `Ok(())` only when the insert is a valid NEW contact:
/// `source == "contact"`, a non-empty DID and name. Any deviation —
/// `source != "contact"` (e.g. an attempt to create an owned identity), an
/// empty DID/name — is a [`ExtensionError::SecurityViolation`].
///
/// Note on `private_key`: [`ContactInsert`] has no `private_key` field, so a
/// caller has nowhere to put one. Rejecting a supplied key, updates, and
/// deletes is enforced structurally — the write entrypoint only ever accepts a
/// [`ContactInsert`] (never an id-bearing update/delete payload, never a
/// private key). This validator is the second line of defense on the value
/// itself.
pub fn validate_contact_insert(insert: &ContactInsert) -> Result<(), ExtensionError> {
    if insert.source != CONTACT_SOURCE {
        return Err(ExtensionError::SecurityViolation {
            reason: format!(
                "Identity write may only insert a contact (source='{CONTACT_SOURCE}'); \
                 got source='{}'. Creating an owned identity is not permitted.",
                insert.source
            ),
        });
    }
    if insert.did.trim().is_empty() {
        return Err(ExtensionError::SecurityViolation {
            reason: "Identity contact insert requires a non-empty DID.".to_string(),
        });
    }
    if insert.name.trim().is_empty() {
        return Err(ExtensionError::SecurityViolation {
            reason: "Identity contact insert requires a non-empty name.".to_string(),
        });
    }
    Ok(())
}
