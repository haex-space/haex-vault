use serde::Deserialize;
use serde_json::Value as JsonValue;

/// Column-signature record accompanying a `RemoteColumnChange` on the wire.
///
/// Runde 5 lays down the plumbing: the field is `Option<ColumnSig>` with a
/// `#[serde(default)]` on the parent so old pushes without a sig still
/// deserialise cleanly. Full enforcement lands in Runde 7 (Task H3) when the
/// TS push path starts populating the field for every change. Until then
/// the field is dormant in production and only exercised by the tests.
///
/// Wire encoding matches `SigRecord`'s JSON shape in `column_sig::storage`:
///   - `authorDid` — the `did:key:…` string of the signing member.
///   - `sig`       — base64-STANDARD-encoded 64-byte Ed25519 signature.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnSig {
    pub author_did: String,
    /// Base64-STANDARD-encoded 64-byte Ed25519 signature bytes.
    pub sig: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteColumnChange {
    pub table_name: String,
    pub row_pks: String, // JSON string
    pub column_name: String,
    pub hlc_timestamp: String,
    pub decrypted_value: JsonValue, // Already decrypted in frontend
    /// Runde-5 plumbing: per-column column signature, `None` until the TS
    /// push path is upgraded to populate it (Task H3). `#[serde(default)]`
    /// keeps old wire payloads deserialisable.
    #[serde(default)]
    pub sig: Option<ColumnSig>,
}
