use serde::Deserialize;
use serde_json::Value as JsonValue;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteColumnChange {
    pub table_name: String,
    pub row_pks: String, // JSON string
    pub column_name: String,
    pub hlc_timestamp: String,
    pub decrypted_value: JsonValue, // Already decrypted in frontend
}
