//! Unit tests for the IAM adapter.
//!
//! Focus on endpoint routing, MinIO refusal, XML response parsing, and
//! error-code classification. Live IAM calls are deferred to the Phase L
//! e2e suite (against MinIO or a real AWS test account).

#![cfg_attr(test, allow(clippy::unwrap_used))]

use super::aws_compat::{classify_error, extract_xml_tag, AwsCompatIamAdapter, ProviderFlavor};
use super::{IamAdapter, IamAdapterError, ScopedCred};

fn rand_key() -> String {
    // Per test-keys-via-rand-random convention — no literal seeds/nonces.
    let bytes: [u8; 16] = rand::random();
    hex::encode(bytes)
}

#[test]
fn aws_provider_routes_to_iam_amazonaws_com() {
    let adapter = AwsCompatIamAdapter::new(&rand_key(), &rand_key(), ProviderFlavor::Aws).unwrap();
    assert_eq!(adapter.endpoint(), "https://iam.amazonaws.com");
    assert_eq!(adapter.region(), "us-east-1");
}

#[test]
fn wasabi_provider_routes_to_iam_wasabisys_com() {
    let adapter =
        AwsCompatIamAdapter::new(&rand_key(), &rand_key(), ProviderFlavor::Wasabi).unwrap();
    assert_eq!(adapter.endpoint(), "https://iam.wasabisys.com");
    // Wasabi shares the AWS SigV4 region convention for IAM.
    assert_eq!(adapter.region(), "us-east-1");
}

#[test]
fn minio_provider_flavor_is_rejected_with_clear_error() {
    let err = AwsCompatIamAdapter::new(
        &rand_key(),
        &rand_key(),
        ProviderFlavor::MinIO {
            admin_endpoint: "https://minio.example.com:9000".to_string(),
        },
    )
    .expect_err("invariant: MinIO must be rejected by AwsCompatIamAdapter::new");
    match err {
        IamAdapterError::Other(msg) => {
            assert!(
                msg.to_lowercase().contains("minio"),
                "error message should mention MinIO, got: {msg}"
            );
        }
        other => panic!("expected Other, got {other:?}"),
    }
}

#[test]
fn extract_xml_tag_pulls_access_key_from_create_access_key_response() {
    // Realistic-shaped CreateAccessKeyResponse (minimised).
    let xml = "<CreateAccessKeyResponse>\
                 <CreateAccessKeyResult>\
                   <AccessKey>\
                     <AccessKeyId>AKIAEXAMPLE</AccessKeyId>\
                     <SecretAccessKey>abc/def+123</SecretAccessKey>\
                     <UserName>haex-share-abcd</UserName>\
                     <Status>Active</Status>\
                   </AccessKey>\
                 </CreateAccessKeyResult>\
               </CreateAccessKeyResponse>";
    assert_eq!(extract_xml_tag(xml, "AccessKeyId"), Some("AKIAEXAMPLE"));
    assert_eq!(extract_xml_tag(xml, "SecretAccessKey"), Some("abc/def+123"));
    assert_eq!(extract_xml_tag(xml, "UserName"), Some("haex-share-abcd"));
    assert_eq!(extract_xml_tag(xml, "MissingTag"), None);
}

#[test]
fn classify_error_maps_no_such_entity_to_not_found() {
    let body = "<ErrorResponse>\
                  <Error>\
                    <Type>Sender</Type>\
                    <Code>NoSuchEntity</Code>\
                    <Message>The user with name x cannot be found.</Message>\
                  </Error>\
                </ErrorResponse>";
    let err = classify_error(body, reqwest::StatusCode::NOT_FOUND);
    match err {
        IamAdapterError::NotFound => {}
        other => panic!("expected NotFound, got {other:?}"),
    }
}

#[test]
fn classify_error_maps_access_denied_variants() {
    for code in [
        "AccessDenied",
        "AccessDeniedException",
        "UnauthorizedOperation",
        "InvalidClientTokenId",
        "SignatureDoesNotMatch",
    ] {
        let body = format!("<ErrorResponse><Error><Code>{code}</Code></Error></ErrorResponse>");
        let err = classify_error(&body, reqwest::StatusCode::FORBIDDEN);
        match err {
            IamAdapterError::AccessDenied(returned) => {
                assert_eq!(returned, code);
            }
            other => panic!("expected AccessDenied for {code}, got {other:?}"),
        }
    }
}

#[test]
fn classify_error_maps_unknown_code_to_other() {
    let body = "<ErrorResponse><Error><Code>SomeNovelIamError</Code></Error></ErrorResponse>";
    let err = classify_error(body, reqwest::StatusCode::BAD_REQUEST);
    match err {
        IamAdapterError::Other(msg) => {
            assert!(msg.contains("SomeNovelIamError"), "got: {msg}");
        }
        other => panic!("expected Other, got {other:?}"),
    }
}

#[test]
fn classify_error_without_code_element_falls_back_to_network() {
    let body = "not xml at all — probably a load-balancer 502 page";
    let err = classify_error(body, reqwest::StatusCode::BAD_GATEWAY);
    match err {
        IamAdapterError::Network(msg) => {
            assert!(msg.contains("502"), "got: {msg}");
        }
        other => panic!("expected Network, got {other:?}"),
    }
}

#[test]
fn scoped_cred_debug_impl_redacts_secret() {
    let cred = ScopedCred {
        access_key_id: "AKIAVISIBLE".to_string(),
        secret_access_key: "shhhh".to_string(),
        iam_user_name: "haex-share-1".to_string(),
    };
    let dbg = format!("{cred:?}");
    assert!(dbg.contains("<redacted>"), "expected redaction, got: {dbg}");
    assert!(!dbg.contains("shhhh"), "secret leaked in Debug: {dbg}");
    assert!(
        !dbg.contains("AKIAVISIBLE"),
        "key id leaked in Debug: {dbg}"
    );
    // iam_user_name is NOT a secret and remains visible.
    assert!(dbg.contains("haex-share-1"));
}

/// Trait-object usability check — Phase E will consume `dyn IamAdapter`.
/// This is a compile-time gate; the test doesn't actually make an IAM
/// call.
#[test]
fn adapter_is_dyn_compatible() {
    let adapter = AwsCompatIamAdapter::new(&rand_key(), &rand_key(), ProviderFlavor::Aws).unwrap();
    let _boxed: Box<dyn IamAdapter> = Box::new(adapter);
}
