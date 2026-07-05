//! Unit tests for the IAM adapter.
//!
//! Focus on endpoint routing, MinIO refusal, XML response parsing, and
//! error-code classification. Live IAM calls are deferred to the Phase L
//! e2e suite (against MinIO or a real AWS test account).

#![cfg_attr(test, allow(clippy::unwrap_used))]

use super::aws_compat::{
    body_contains_error_envelope, classify_error, cleanup_user_steps, extract_all_xml_tags,
    extract_xml_tag, AwsCompatIamAdapter, ProviderFlavor, HAEX_SHARE_POLICY_NAME,
};
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
fn extract_all_xml_tags_collects_every_access_key_from_list_response() {
    // ListAccessKeysResponse with two key entries — the rollback path uses
    // this to discover keys whose CreateAccessKey response was unparseable.
    let xml = "<ListAccessKeysResponse>\
                 <ListAccessKeysResult>\
                   <AccessKeyMetadata>\
                     <member>\
                       <AccessKeyId>AKIAFIRST</AccessKeyId>\
                       <Status>Active</Status>\
                     </member>\
                     <member>\
                       <AccessKeyId>AKIASECOND</AccessKeyId>\
                       <Status>Inactive</Status>\
                     </member>\
                   </AccessKeyMetadata>\
                 </ListAccessKeysResult>\
               </ListAccessKeysResponse>";
    assert_eq!(
        extract_all_xml_tags(xml, "AccessKeyId"),
        vec!["AKIAFIRST", "AKIASECOND"]
    );
    assert!(extract_all_xml_tags(xml, "MissingTag").is_empty());
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

// -- Fix 1: HAEX_SHARE_POLICY_NAME is a stable, non-caller-configurable
// constant. Pinning the value is a regression fence — if create/delete ever
// desynchronise on this literal we leak orphan policies that block
// DeleteUser.

#[test]
fn haex_share_policy_name_is_stable() {
    assert_eq!(
        HAEX_SHARE_POLICY_NAME, "haex-share-policy",
        "the inline policy name must stay stable — create and delete both \
         hardcode this constant and any change desyncs the two paths"
    );
}

// -- Fix 2: body_contains_error_envelope discriminates success from a
// 2xx-wrapped error payload (Wasabi + reverse-proxy pathology).

#[test]
fn body_contains_error_envelope_detects_wrapped_error() {
    let payload = "<ErrorResponse><Error><Code>AccessDenied</Code>\
                   <Message>nope</Message></Error></ErrorResponse>";
    assert!(body_contains_error_envelope(payload));
}

#[test]
fn body_contains_error_envelope_ignores_success_responses() {
    // A minimal but realistic IAM success envelope.
    let create_user_ok = "<CreateUserResponse>\
                            <CreateUserResult>\
                              <User>\
                                <UserName>haex-share-xyz</UserName>\
                              </User>\
                            </CreateUserResult>\
                          </CreateUserResponse>";
    assert!(!body_contains_error_envelope(create_user_ok));

    // Also empty body and put-user-policy-ok shape.
    assert!(!body_contains_error_envelope(""));
    assert!(!body_contains_error_envelope(
        "<PutUserPolicyResponse><ResponseMetadata><RequestId>abc</RequestId>\
         </ResponseMetadata></PutUserPolicyResponse>"
    ));
}

#[test]
fn two_hundred_with_embedded_error_maps_via_classify_error() {
    // End-to-end verification of the Fix-3 path: a 200 status carrying an
    // <ErrorResponse> body must classify as the corresponding error rather
    // than being handed back to the caller as Ok(body).
    let body = "<ErrorResponse><Error><Code>AccessDenied</Code>\
                <Message>reverse proxy stripped the status code</Message>\
                </Error></ErrorResponse>";
    assert!(body_contains_error_envelope(body));
    match classify_error(body, reqwest::StatusCode::OK) {
        IamAdapterError::AccessDenied(code) => assert_eq!(code, "AccessDenied"),
        other => panic!("expected AccessDenied on 200-with-error, got {other:?}"),
    }
}

// -- Fix 2 (rollback): IamAdapterError must be Clone so the rollback path
// can log the cleanup failure and still surface the primary error. (The
// production impl does not clone; this pins the derive so we don't remove
// it in a future refactor.)

// -- Fix 2 (rollback step count): cleanup_user_steps omits DeleteAccessKey
// when the caller has no access-key id (the rollback path when
// CreateAccessKey never succeeded). Also verifies delete order stays
// DeleteAccessKey → DeleteUserPolicy → DeleteUser, which is IAM's required
// dependency order.

#[test]
fn cleanup_user_steps_full_path_has_three_steps() {
    let steps = cleanup_user_steps("haex-share-abc", Some("AKIA123"));
    assert_eq!(steps.len(), 3, "full delete must run 3 IAM actions");
    let actions: Vec<&str> = steps
        .iter()
        .filter_map(|s| s.iter().find(|(k, _)| *k == "Action").map(|(_, v)| *v))
        .collect();
    assert_eq!(
        actions,
        vec!["DeleteAccessKey", "DeleteUserPolicy", "DeleteUser"],
        "IAM requires this dependency order"
    );
}

#[test]
fn cleanup_user_steps_rollback_path_skips_delete_access_key() {
    let steps = cleanup_user_steps("haex-share-abc", None);
    assert_eq!(
        steps.len(),
        2,
        "rollback before CreateAccessKey succeeded must not attempt \
         DeleteAccessKey — there is no key id yet"
    );
    let actions: Vec<&str> = steps
        .iter()
        .filter_map(|s| s.iter().find(|(k, _)| *k == "Action").map(|(_, v)| *v))
        .collect();
    assert_eq!(actions, vec!["DeleteUserPolicy", "DeleteUser"]);
}

#[test]
fn cleanup_user_steps_uses_hardcoded_policy_name() {
    // Guards create/delete symmetry: the delete step must reference the
    // same policy name the adapter attached during create.
    let steps = cleanup_user_steps("haex-share-abc", Some("AKIA123"));
    let policy_step = steps
        .iter()
        .find(|s| s.iter().any(|(_, v)| *v == "DeleteUserPolicy"))
        .expect("invariant: DeleteUserPolicy step must exist");
    let policy_name = policy_step
        .iter()
        .find(|(k, _)| *k == "PolicyName")
        .map(|(_, v)| *v)
        .expect("invariant: DeleteUserPolicy must carry PolicyName");
    assert_eq!(policy_name, HAEX_SHARE_POLICY_NAME);
}

#[test]
fn iam_adapter_error_is_clone() {
    let err = IamAdapterError::AccessDenied("test".to_string());
    let cloned = err.clone();
    match (err, cloned) {
        (IamAdapterError::AccessDenied(a), IamAdapterError::AccessDenied(b)) => {
            assert_eq!(a, b);
        }
        _ => panic!("clone changed variant"),
    }
}
