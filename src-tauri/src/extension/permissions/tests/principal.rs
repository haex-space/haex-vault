// src-tauri/src/extension/permissions/tests/principal.rs

use crate::extension::permissions::types::Principal;

#[test]
fn extension_principal_exposes_id_and_kind() {
    let principal = Principal::Extension("ext-123".to_string());

    assert_eq!(principal.id(), "ext-123");
    assert_eq!(principal.kind_str(), "extension");
    assert!(principal.is_extension());
}

#[test]
fn external_client_principal_exposes_id_and_kind() {
    let principal = Principal::ExternalClient("client-abc".to_string());

    assert_eq!(principal.id(), "client-abc");
    assert_eq!(principal.kind_str(), "external_client");
    assert!(!principal.is_extension());
}
