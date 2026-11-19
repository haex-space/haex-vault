use serde_json::json;

use super::*;

#[test]
fn codes_are_those_the_vault_sdk_knows_plus_haexs_own() {
    let codes = [
        (ExtensionErrorCode::SecurityViolation, 1000),
        (ExtensionErrorCode::NotFound, 1001),
        (ExtensionErrorCode::PermissionDenied, 1002),
        (ExtensionErrorCode::PermissionPromptRequired, 1004),
        (ExtensionErrorCode::Database, 2000),
        (ExtensionErrorCode::Filesystem, 2001),
        (ExtensionErrorCode::Http, 2002),
        (ExtensionErrorCode::Shell, 2003),
        (ExtensionErrorCode::Web, 2005),
        (ExtensionErrorCode::Manifest, 3000),
        (ExtensionErrorCode::Validation, 3001),
        (ExtensionErrorCode::LimitExceeded, 7000),
        (ExtensionErrorCode::NotSupported, 8000),
        (ExtensionErrorCode::NotAvailable, 8001),
        (ExtensionErrorCode::Disabled, 8002),
    ];
    for (code, number) in codes {
        assert_eq!(code.as_u16(), number, "{code:?}");
    }
}

#[test]
fn a_bridge_error_serialises_as_the_sdk_reads_it() {
    assert_eq!(
        serde_json::to_value(BridgeError::not_supported()).expect("serialise"),
        json!({ "code": 8000, "message": "not supported" })
    );
    assert_eq!(
        serde_json::to_value(
            BridgeError::new(
                ExtensionErrorCode::PermissionPromptRequired,
                "prompt required"
            )
            .with_details(json!({ "resourceType": "web", "action": "GET", "target": "*" }))
        )
        .expect("serialise"),
        json!({
            "code": 1004,
            "message": "prompt required",
            "details": { "resourceType": "web", "action": "GET", "target": "*" },
        })
    );
}

#[test]
fn the_fixed_answers_have_their_codes() {
    assert_eq!(
        BridgeError::not_available().code,
        ExtensionErrorCode::NotAvailable
    );
    assert_eq!(BridgeError::disabled().code, ExtensionErrorCode::Disabled);
}

#[test]
fn a_missing_function_of_the_device_answers_not_available() {
    assert!(BridgeError::unless(true).is_ok());
    let error = BridgeError::unless(false).unwrap_err();
    assert_eq!(error.code, ExtensionErrorCode::NotAvailable);
}
