use crate::crdt::column_sig::value_bytes::StorageClass;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

/// Column-signature record accompanying a `RemoteColumnChange` on the wire.
///
/// Wire encoding matches `SigRecord`'s JSON shape in `column_sig::storage`:
///   - `authorDid` — the `did:key:…` string of the signing member.
///   - `sig`       — base64-STANDARD-encoded 64-byte Ed25519 signature.
///   - `storageClass` — the original SQLite storage class.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnSig {
    pub author_did: String,
    /// Base64-STANDARD-encoded 64-byte Ed25519 signature bytes.
    pub sig: String,
    /// Original SQLite storage class. JSON/IPC alone cannot distinguish an
    /// integer-valued REAL from INTEGER or a base64 TEXT from BLOB.
    pub storage_class: StorageClass,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteColumnChange {
    pub table_name: String,
    pub row_pks: String, // JSON string
    pub column_name: String,
    pub hlc_timestamp: String,
    pub decrypted_value: JsonValue, // Already decrypted in frontend
    /// Per-column author signature. Personal-vault sync remains unsigned;
    /// shared-space apply paths reject missing signatures.
    #[serde(default)]
    pub sig: Option<ColumnSig>,
}
