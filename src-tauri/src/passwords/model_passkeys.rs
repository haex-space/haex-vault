//! Passkey types of the password manager (spec 034 FR-004, spec 036 US5, data-model.md,
//! `contracts/passkey-service.md`): what the window and the passkey service tell about a passkey.
//! None of them carries a key.

use std::fmt;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// The entry a linked passkey belongs to (spec 036, research R6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct LinkedFrom {
    pub item_id: String,
    pub title: Option<String>,
}

/// A passkey shown at an entry, without its keys: one of its own, or with `linked_from` one of
/// another entry shown by a link (then the window offers only "Verweis lösen").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct PasskeyView {
    pub id: String,
    pub relying_party_id: String,
    pub relying_party_name: Option<String>,
    pub user_name: Option<String>,
    pub nickname: Option<String>,
    #[ts(type = "number")]
    pub algorithm: i64,
    pub created_at: Option<String>,
    pub last_used_at: Option<String>,
    /// The entry the passkey belongs to (`None` only for old data without an entry).
    pub item_id: Option<String>,
    pub is_discoverable: bool,
    /// The imported counter; holzi's own confirmations leave it as it is (research R5).
    #[ts(type = "number")]
    pub sign_count: i64,
    pub linked_from: Option<LinkedFrom>,
}

/// A passkey as the passkey service lists it (`passkey_list`, `ChoiceRequired`): the header data,
/// the credential id as Base64URL, never a key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct PasskeyHeader {
    pub id: String,
    pub credential_id: String,
    pub rp_id: String,
    pub rp_name: Option<String>,
    pub user_name: Option<String>,
    pub user_display_name: Option<String>,
    pub nickname: Option<String>,
    #[ts(type = "number")]
    pub algorithm: i64,
    pub is_discoverable: bool,
    pub created_at: Option<String>,
    pub last_used_at: Option<String>,
    /// The entry through which the caller reads the passkey: its own when readable, else a target.
    pub item_id: String,
    /// Set when the caller reads the passkey through a link: the entry it belongs to.
    pub linked_from_item_id: Option<String>,
}

/// Whether the user took part in one ceremony (spec 049, ADR-0015): `Present` after a confirmation
/// bound to exactly this request, `Verified` when that confirmation also took the vault password.
/// Deliberately not `Deserialize`: no input from outside can claim it; only the caller that showed
/// the confirmation sets it. `None` keeps the flags of spec 036 (no UP, no UV).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum UserPresence {
    #[default]
    None,
    Present,
    Verified,
}

/// Where a new passkey goes: an entry the caller may write, or a new entry created with the passkey
/// in the same transaction (spec 049, research R11).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PasskeyTarget {
    Existing(String),
    New(NewPasskeyEntry),
}

/// The fields of the entry a new passkey creates.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NewPasskeyEntry {
    pub title: String,
    pub url: Option<String>,
    pub username: Option<String>,
    pub tags: Vec<String>,
}

/// `passkey_create` (all binary fields Base64URL without padding).
#[derive(Debug, Clone)]
pub struct PasskeyCreateRequest {
    pub target: PasskeyTarget,
    pub rp_id: String,
    pub rp_name: String,
    pub origin: String,
    pub user_handle: String,
    pub user_name: String,
    pub user_display_name: Option<String>,
    pub challenge: String,
    pub exclude_credentials: Vec<String>,
    /// COSE algorithms in the relying party's order of preference.
    pub pub_key_cred_params: Vec<i64>,
    pub discoverable: bool,
    pub presence: UserPresence,
}

/// The answer of `passkey_create`; holds no private key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PasskeyCreated {
    pub credential_id: String,
    pub attestation_object: String,
    pub client_data_json: String,
    pub public_key_spki: String,
    pub algorithm: i64,
    pub item_id: String,
    /// The row id of the new passkey.
    pub passkey_id: String,
}

/// `passkey_confirm`.
#[derive(Debug, Clone, Default)]
pub struct PasskeyConfirmRequest {
    pub rp_id: String,
    pub origin: String,
    pub challenge: String,
    pub allow_credentials: Vec<String>,
    pub item_id: Option<String>,
    pub presence: UserPresence,
}

/// The answer of `passkey_confirm`; the signature is public, the key never leaves.
#[derive(Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PasskeyAssertion {
    pub credential_id: String,
    pub authenticator_data: String,
    pub client_data_json: String,
    pub signature: String,
    pub user_handle: Option<String>,
    pub item_id: String,
}

impl fmt::Debug for PasskeyAssertion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PasskeyAssertion")
            .field("credential_id", &self.credential_id)
            .field("item_id", &self.item_id)
            .finish_non_exhaustive()
    }
}

/// `passkey_list`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PasskeyListRequest {
    pub rp_id: Option<String>,
    pub item_id: Option<String>,
    #[serde(default)]
    pub discoverable_only: bool,
}
