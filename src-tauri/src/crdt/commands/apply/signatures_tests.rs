use super::*;
use crate::crdt::column_sig::limits::MAX_VALUE_BYTES_LEN;
use crate::crdt::column_sig::value_bytes::StorageClass;
use serde_json::json;

fn blob_change(value: serde_json::Value) -> RemoteColumnChange {
    RemoteColumnChange {
        table_name: "items".to_string(),
        row_pks: r#"{"id":"row-1"}"#.to_string(),
        column_name: "payload".to_string(),
        hlc_timestamp: "1/node".to_string(),
        decrypted_value: value,
        sig: None,
    }
}

fn blob_sig() -> ColumnSig {
    ColumnSig {
        author_did: "did:key:z6MkwQpY6JvXxJmY3XcW3VYgVY8J8WjP2aYkQbQ5VqYQv".to_string(),
        sig: String::new(),
        storage_class: StorageClass::Blob,
    }
}

#[test]
fn verify_change_sig_rejects_oversized_blob_arrays_before_restore() {
    let oversized = json!(vec![0u8; MAX_VALUE_BYTES_LEN]);
    let err = verify_change_sig(
        &blob_change(oversized),
        &blob_sig(),
        Some("space-1"),
        "items",
        r#"{"id":"row-1"}"#,
    )
    .unwrap_err();

    assert_eq!(err, "BLOB value exceeds the column-signature size limit");
}

#[test]
fn verify_change_sig_allows_small_blob_arrays_to_reach_signature_validation() {
    let small_body = json!([0, 1, 2]);
    let err = verify_change_sig(
        &blob_change(small_body),
        &blob_sig(),
        Some("space-1"),
        "items",
        r#"{"id":"row-1"}"#,
    )
    .unwrap_err();

    assert_ne!(err, "BLOB value exceeds the column-signature size limit");
}
