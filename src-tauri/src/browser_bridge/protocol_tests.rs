//! Tests for the wire messages (spec 049, T009): the literal frames of `contracts/wire-protocol.md`
//! parse, unknown frames are a parse error, and the answers have the template's shape.

use serde_json::{json, Value};

use super::protocol::{
    parse, render, ChallengeFrame, ErrorCode, ErrorFrame, HandshakeResponse, Incoming, Outgoing,
    Reply, ResponseEnvelope, PROTOCOL_VERSION,
};

fn as_json(frame: &Outgoing) -> Value {
    serde_json::from_str(&render(frame)).expect("json")
}

#[test]
fn the_handshake_of_the_extension_parses() {
    let text = json!({
        "type": "handshake",
        "version": 2,
        "client": {
            "clientId": "3f1c0000000000000000000000000000",
            "clientName": "haex-pass Browser Extension",
            "publicKey": "AAAA",
            "requestedExtensions": [{ "name": "core", "extensionPublicKey": "__core__" }],
            "permissions": { "core": { "passwords": [{ "target": "*", "operation": "readWrite" }] } }
        }
    })
    .to_string();
    let Ok(Incoming::Handshake(handshake)) = parse(&text) else {
        panic!("handshake expected");
    };
    assert_eq!(handshake.version, 2);
    assert_eq!(
        handshake.client.client_id,
        "3f1c0000000000000000000000000000"
    );
    assert_eq!(
        handshake.client.client_name.as_deref(),
        Some("haex-pass Browser Extension")
    );
    assert!(handshake.client.permissions.is_some());
}

#[test]
fn a_handshake_without_optional_parts_parses() {
    let text = r#"{"type":"handshake","version":1,"client":{"clientId":"x","publicKey":"AAAA"}}"#;
    let Ok(Incoming::Handshake(handshake)) = parse(text) else {
        panic!("handshake expected");
    };
    assert_eq!(handshake.version, 1);
    assert!(handshake.client.permissions.is_none());
    assert!(handshake.client.client_name.is_none());
}

#[test]
fn a_request_parses_and_knows_its_target() {
    let text = json!({
        "type": "request", "action": "get-items", "message": "m", "iv": "i",
        "clientId": "ignored", "publicKey": "p",
        "extensionPublicKey": "__core__", "extensionName": "core"
    })
    .to_string();
    let Ok(Incoming::Request(request)) = parse(&text) else {
        panic!("request expected");
    };
    assert_eq!(request.action, "get-items");
    assert!(request.targets_core());

    let other = json!({
        "type": "request", "action": "get-items", "message": "m", "iv": "i", "publicKey": "p",
        "extensionPublicKey": "abc", "extensionName": "core"
    })
    .to_string();
    let Ok(Incoming::Request(request)) = parse(&other) else {
        panic!("request expected");
    };
    assert!(!request.targets_core());
}

#[test]
fn ping_parses_and_unknown_frames_are_a_parse_error() {
    assert_eq!(parse(r#"{"type":"ping"}"#), Ok(Incoming::Ping));
    assert_eq!(parse(r#"{"type":"hello"}"#), Err(ErrorCode::ParseError));
    assert_eq!(parse("not json"), Err(ErrorCode::ParseError));
    assert_eq!(parse(r#"{"version":2}"#), Err(ErrorCode::ParseError));
}

#[test]
fn the_handshake_response_has_the_template_shape() {
    let frame = Outgoing::HandshakeResponse(HandshakeResponse {
        version: 2,
        server_public_key: "S".into(),
        authorized: false,
        pending_approval: true,
    });
    assert_eq!(
        as_json(&frame),
        json!({
            "type": "handshakeResponse", "version": 2, "serverPublicKey": "S",
            "authorized": false, "pendingApproval": true
        })
    );
}

#[test]
fn the_response_keeps_null_extension_fields_and_an_empty_client_id() {
    let frame = Outgoing::Response(ResponseEnvelope {
        action: "get-items".into(),
        message: "m".into(),
        iv: "i".into(),
        client_id: String::new(),
        public_key: "p".into(),
        extension_public_key: None,
        extension_name: None,
    });
    assert_eq!(
        as_json(&frame),
        json!({
            "type": "response", "action": "get-items", "message": "m", "iv": "i",
            "clientId": "", "publicKey": "p", "extensionPublicKey": null, "extensionName": null
        })
    );
}

#[test]
fn update_pong_and_error_frames_have_the_template_shape() {
    assert_eq!(
        as_json(&Outgoing::AuthorizationUpdate { authorized: true }),
        json!({ "type": "authorizationUpdate", "authorized": true })
    );
    assert_eq!(as_json(&Outgoing::Pong), json!({ "type": "pong" }));
    let error = as_json(&Outgoing::Error(ErrorFrame::new(
        ErrorCode::PermissionsDeclarationRequired,
    )));
    assert_eq!(error["type"], "error");
    assert_eq!(error["code"], "PERMISSIONS_DECLARATION_REQUIRED");
    assert!(error["message"].is_string());
}

#[test]
fn replies_carry_data_or_a_fixed_error() {
    assert_eq!(
        serde_json::to_value(Reply::ok("r1".into(), json!({ "entries": [] }))).expect("json"),
        json!({ "requestId": "r1", "success": true, "data": { "entries": [] } })
    );
    assert_eq!(
        serde_json::to_value(Reply::fail("r2".into(), ErrorCode::NotSupported)).expect("json"),
        json!({ "requestId": "r2", "success": false, "error": "Not supported", "errorCode": "NOT_SUPPORTED" })
    );
    assert_eq!(ErrorCode::UserCancelled.message(), "USER_CANCELLED");
}

#[test]
fn the_proof_of_protocol_3_parses_and_the_challenge_has_the_contract_shape() {
    assert_eq!(PROTOCOL_VERSION, 3);
    let text = json!({ "type": "proof", "publicKey": "AAAA", "iv": "BBBB", "message": "CCCC" })
        .to_string();
    let Ok(Incoming::Proof(proof)) = parse(&text) else {
        panic!("proof expected");
    };
    assert_eq!(
        (
            proof.public_key.as_str(),
            proof.iv.as_str(),
            proof.message.as_str()
        ),
        ("AAAA", "BBBB", "CCCC")
    );
    let challenge = Outgoing::Challenge(ChallengeFrame {
        server_public_key: "S".into(),
        public_key: "P".into(),
        iv: "I".into(),
        message: "M".into(),
    });
    assert_eq!(
        as_json(&challenge),
        json!({ "type": "challenge", "serverPublicKey": "S", "publicKey": "P", "iv": "I", "message": "M" })
    );
}

#[test]
fn an_outdated_client_and_a_failed_proof_get_their_own_codes() {
    for (code, wire) in [
        (
            ErrorCode::ProtocolUpdateRequired,
            "PROTOCOL_UPDATE_REQUIRED",
        ),
        (ErrorCode::ProofFailed, "PROOF_FAILED"),
    ] {
        let frame = as_json(&Outgoing::Error(ErrorFrame::new(code)));
        assert_eq!(frame["type"], "error");
        assert_eq!(frame["code"], wire);
        assert!(!frame["message"].as_str().expect("text").is_empty());
    }
    assert!(ErrorCode::ProtocolUpdateRequired
        .message()
        .contains("update haex-pass"));
}
