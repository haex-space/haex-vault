//! The rows of the phone's address book as the Kotlin side hands them over (spec 051,
//! contracts/android-plugin.md). Kotlin only maps rows of the Contacts Provider to these shapes;
//! every rule of the sync lies in the core (`contacts/phone/`).

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// An account of the phone's address book. The local account has an empty type and name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhoneAccount {
    #[serde(rename = "type")]
    pub account_type: String,
    pub name: String,
    /// A sync adapter that uploads changes, or the local account.
    pub writable: bool,
    /// The account the phone itself saves new contacts in.
    pub is_default: bool,
}

/// One raw contact as the index lists it: enough to see what changed since the last run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawIndex {
    pub id: i64,
    /// `RawContacts.VERSION`, raised on every change of the raw contact or its data.
    pub version: i64,
    pub deleted: bool,
    pub account_type: String,
    pub account_name: String,
    pub source_id: Option<String>,
    /// The aggregated contact the raw contact belongs to.
    pub contact_id: i64,
    pub lookup_key: String,
    /// `Contacts.CONTACT_LAST_UPDATED_TIMESTAMP` of the aggregated contact, in milliseconds.
    pub last_updated: Option<i64>,
    pub read_only: bool,
}

/// The data rows of one raw contact; only the mimetypes the contract names.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawData {
    pub id: i64,
    pub data: Vec<DataRow>,
    /// The raw contact has a photo.
    #[serde(default)]
    pub has_photo: bool,
}

/// One data row: `mimetype` is the short name (`Phone`, `StructuredName`, …).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataRow {
    pub data_id: i64,
    pub mimetype: String,
    pub is_read_only: bool,
    pub fields: Map<String, Value>,
}

/// Where an op finds the raw contact or group it works on: an existing id, or the `ref` of an
/// insert earlier in the same call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Target {
    Id(i64),
    Ref(u32),
}

/// One change of the phone's address book; a call applies its ops as one batch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum Op {
    #[serde(rename_all = "camelCase")]
    InsertRaw {
        account_type: String,
        account_name: String,
        #[serde(rename = "ref")]
        reference: u32,
    },
    #[serde(rename_all = "camelCase")]
    InsertData {
        raw: Target,
        mimetype: String,
        fields: Map<String, Value>,
    },
    #[serde(rename_all = "camelCase")]
    UpdateData {
        data_id: i64,
        mimetype: String,
        fields: Map<String, Value>,
    },
    #[serde(rename_all = "camelCase")]
    DeleteData { data_id: i64 },
    #[serde(rename_all = "camelCase")]
    DeleteRaw { raw_contact_id: i64 },
}

/// What a batch did: per op the raw contact it touched and that raw contact's version after the
/// whole batch, or the error that stopped it (without personal data).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyResult {
    pub results: Vec<OpResult>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpResult {
    pub raw_contact_id: Option<i64>,
    pub version: Option<i64>,
}

/// What every call of the plugin's contact commands fails with off Android.
pub const UNAVAILABLE: &str = "the phone's address book exists only on Android";
