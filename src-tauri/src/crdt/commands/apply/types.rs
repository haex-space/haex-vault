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

/// Convert vault's wire DTO to `haex_crdt`'s own change type, for the
/// `apply_remote_changes` call.
///
/// `sig` is carried through as opaque JSON in the crate's generic `sig`
/// field — the crate never interprets it (D-3: it does not know what a
/// column signature means), but [`super::policy::VaultApplyPolicy`] gets it
/// back verbatim via [`from_crate_change`] to run its own per-space
/// signature verification in `prepare_row`. `device_id` has no wire
/// equivalent on this DTO and is not treated as an authorization signal
/// anywhere in the apply path — left empty.
///
/// A `ColumnSig` (two `String`s and a plain enum) cannot realistically fail
/// to serialize, but a signed column falling back to `None` on a
/// hypothetical failure — rather than panicking — is the fail-closed
/// direction anyway: `enforce_sigs` drops an unsigned column, it never lets
/// one through unverified.
pub(super) fn to_crate_change(change: &RemoteColumnChange) -> haex_crdt::ColumnChange {
    haex_crdt::ColumnChange {
        table_name: change.table_name.clone(),
        row_pks: change.row_pks.clone(),
        column_name: change.column_name.clone(),
        hlc_timestamp: change.hlc_timestamp.clone(),
        value: change.decrypted_value.clone(),
        device_id: String::new(),
        sig: change
            .sig
            .as_ref()
            .and_then(|s| serde_json::to_value(s).ok()),
    }
}

/// Reverse of [`to_crate_change`] — reconstructs vault's wire DTO from the
/// crate's [`haex_crdt::ColumnChange`], so the existing, unchanged
/// `resolve_row_space_id_for_sig` / `verify_change_sig` /
/// `build_incoming_registry_change` helpers (which all take
/// `&RemoteColumnChange`, unmodified from before this cutover) keep working
/// verbatim from inside [`super::policy::VaultApplyPolicy::prepare_row`].
///
/// A `sig` that fails to deserialize back into [`ColumnSig`] becomes `None`
/// rather than an error: the column is then treated as unsigned, which
/// `enforce_sigs` already drops (except the `authored_by_did` exemption) —
/// fail-closed, not fail-open, and observably identical to today's
/// malformed-signature handling (`verify_change_sig` also drops on any
/// malformed-sig error).
pub(super) fn from_crate_change(change: &haex_crdt::ColumnChange) -> RemoteColumnChange {
    RemoteColumnChange {
        table_name: change.table_name.clone(),
        row_pks: change.row_pks.clone(),
        column_name: change.column_name.clone(),
        hlc_timestamp: change.hlc_timestamp.clone(),
        decrypted_value: change.value.clone(),
        sig: change
            .sig
            .as_ref()
            .and_then(|v| serde_json::from_value(v.clone()).ok()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crdt::column_sig::value_bytes::StorageClass;

    fn sample_sig() -> ColumnSig {
        ColumnSig {
            author_did: "did:key:zTest".to_string(),
            sig: "c2lnbmF0dXJl".to_string(),
            storage_class: StorageClass::Text,
        }
    }

    #[test]
    fn to_crate_change_preserves_wire_fields_and_serializes_sig() {
        let change = RemoteColumnChange {
            table_name: "items".to_string(),
            row_pks: r#"{"id":"r1"}"#.to_string(),
            column_name: "name".to_string(),
            hlc_timestamp: "1/aaa".to_string(),
            decrypted_value: JsonValue::String("hello".to_string()),
            sig: Some(sample_sig()),
        };
        let crate_change = to_crate_change(&change);
        assert_eq!(crate_change.table_name, "items");
        assert_eq!(crate_change.row_pks, r#"{"id":"r1"}"#);
        assert_eq!(crate_change.column_name, "name");
        assert_eq!(crate_change.hlc_timestamp, "1/aaa");
        assert_eq!(crate_change.value, JsonValue::String("hello".to_string()));
        assert_eq!(crate_change.device_id, "");
        assert!(crate_change.sig.is_some(), "signed change must carry sig");
    }

    #[test]
    fn to_crate_change_carries_none_sig_through_as_none() {
        let change = RemoteColumnChange {
            table_name: "items".to_string(),
            row_pks: r#"{"id":"r1"}"#.to_string(),
            column_name: "name".to_string(),
            hlc_timestamp: "1/aaa".to_string(),
            decrypted_value: JsonValue::Null,
            sig: None,
        };
        assert!(to_crate_change(&change).sig.is_none());
    }

    #[test]
    fn round_trip_through_crate_change_preserves_sig() {
        let original = RemoteColumnChange {
            table_name: "items".to_string(),
            row_pks: r#"{"id":"r1"}"#.to_string(),
            column_name: "name".to_string(),
            hlc_timestamp: "1/aaa".to_string(),
            decrypted_value: JsonValue::String("hello".to_string()),
            sig: Some(sample_sig()),
        };
        let restored = from_crate_change(&to_crate_change(&original));
        assert_eq!(restored.table_name, original.table_name);
        assert_eq!(restored.row_pks, original.row_pks);
        assert_eq!(restored.column_name, original.column_name);
        assert_eq!(restored.hlc_timestamp, original.hlc_timestamp);
        assert_eq!(restored.decrypted_value, original.decrypted_value);
        let restored_sig = restored.sig.expect("sig must round-trip");
        let original_sig = original.sig.unwrap();
        assert_eq!(restored_sig.author_did, original_sig.author_did);
        assert_eq!(restored_sig.sig, original_sig.sig);
        assert_eq!(restored_sig.storage_class, original_sig.storage_class);
    }

    #[test]
    fn round_trip_preserves_none_sig() {
        let original = RemoteColumnChange {
            table_name: "items".to_string(),
            row_pks: r#"{"id":"r1"}"#.to_string(),
            column_name: "name".to_string(),
            hlc_timestamp: "1/aaa".to_string(),
            decrypted_value: JsonValue::Null,
            sig: None,
        };
        assert!(from_crate_change(&to_crate_change(&original)).sig.is_none());
    }
}
