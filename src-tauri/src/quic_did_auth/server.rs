//! Server-side of the QUIC DID-auth handshake.
//!
//! `challenge_and_verify` sends a fresh nonce + server endpoint id to the
//! client and verifies the returned signature against the public key encoded
//! in the client's DID. On success it returns the cryptographically verified
//! DID — from that point on, that DID is bound to the current QUIC connection
//! and can be trusted as the peer identity for UCAN audience checks.

use std::time::Duration;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use ed25519_dalek::Signature;
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::time::timeout;

use crate::ucan::public_key_from_did;

use super::wire::{build_sig_input, Challenge, Response, MAX_MESSAGE_SIZE, NONCE_LEN, PROTOCOL_VERSION};

/// Read/write timeout for the handshake. Matches `quic_retry::READ_TIMEOUT_SECS`
/// (the existing slow-peer budget for sync requests).
pub const CHALLENGE_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Error)]
pub enum ChallengeError {
    #[error("wire protocol error: {0}")]
    WireProtocol(String),
    #[error("unsupported protocol version: {0}")]
    UnsupportedVersion(u32),
    #[error("client endpoint id mismatch: announced {announced}, actual {actual}")]
    EndpointIdMismatch { announced: String, actual: String },
    #[error("malformed DID: {0}")]
    MalformedDid(String),
    #[error("malformed base64 in nonce or signature")]
    MalformedBase64,
    #[error("nonce length must be {expected} bytes, got {got}")]
    NonceLength { expected: usize, got: usize },
    #[error("signature verification failed")]
    SignatureInvalid,
    #[error("timeout waiting for client response")]
    Timeout,
}

/// Run the server side of the handshake. Generates a fresh nonce, writes the
/// Challenge, awaits the Response, and returns the verified DID.
///
/// Caller passes both endpoint ids as strings — `own_endpoint_id` is what the
/// server reports inside the Challenge, `remote_endpoint_id` is what iroh
/// reports for the connected peer (`connection.remote_id().to_string()`) and
/// must equal what the client claims in the Response.
pub async fn challenge_and_verify<R, W>(
    send: &mut W,
    recv: &mut R,
    own_endpoint_id: &str,
    remote_endpoint_id: &str,
) -> Result<String, ChallengeError>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut nonce = [0u8; NONCE_LEN];
    rand::fill(&mut nonce);

    let challenge = Challenge {
        v: PROTOCOL_VERSION,
        nonce: BASE64.encode(nonce),
        server_endpoint_id: own_endpoint_id.to_string(),
    };

    write_message(send, &challenge).await?;

    let response: Response = timeout(CHALLENGE_TIMEOUT, read_message(recv))
        .await
        .map_err(|_| ChallengeError::Timeout)??;

    verify_response(&response, &nonce, remote_endpoint_id, own_endpoint_id)
}

/// Pure verification step, split out so it is unit-testable without an I/O
/// pair. Caller must supply the nonce that was sent in the Challenge.
pub(crate) fn verify_response(
    response: &Response,
    expected_nonce: &[u8],
    remote_endpoint_id: &str,
    server_endpoint_id: &str,
) -> Result<String, ChallengeError> {
    if response.v != PROTOCOL_VERSION {
        return Err(ChallengeError::UnsupportedVersion(response.v));
    }

    if response.client_endpoint_id != remote_endpoint_id {
        return Err(ChallengeError::EndpointIdMismatch {
            announced: response.client_endpoint_id.clone(),
            actual: remote_endpoint_id.to_string(),
        });
    }

    let verifying_key = public_key_from_did(&response.did)
        .map_err(|e| ChallengeError::MalformedDid(e.to_string()))?;

    let sig_bytes = BASE64
        .decode(&response.signature)
        .map_err(|_| ChallengeError::MalformedBase64)?;

    let signature = Signature::from_slice(&sig_bytes).map_err(|_| ChallengeError::SignatureInvalid)?;

    let sig_input = build_sig_input(expected_nonce, &response.client_endpoint_id, server_endpoint_id);

    verifying_key
        .verify_strict(&sig_input, &signature)
        .map_err(|_| ChallengeError::SignatureInvalid)?;

    Ok(response.did.clone())
}

async fn write_message<T, W>(send: &mut W, msg: &T) -> Result<(), ChallengeError>
where
    T: serde::Serialize,
    W: AsyncWrite + Unpin,
{
    let json = serde_json::to_vec(msg).map_err(|e| ChallengeError::WireProtocol(e.to_string()))?;
    if json.len() > MAX_MESSAGE_SIZE {
        return Err(ChallengeError::WireProtocol(format!(
            "outgoing message too large: {} bytes (max {})",
            json.len(),
            MAX_MESSAGE_SIZE
        )));
    }
    let len_be = (json.len() as u32).to_be_bytes();
    send.write_all(&len_be)
        .await
        .map_err(|e| ChallengeError::WireProtocol(e.to_string()))?;
    send.write_all(&json)
        .await
        .map_err(|e| ChallengeError::WireProtocol(e.to_string()))?;
    Ok(())
}

async fn read_message<T, R>(recv: &mut R) -> Result<T, ChallengeError>
where
    T: serde::de::DeserializeOwned,
    R: AsyncRead + Unpin,
{
    let mut len_buf = [0u8; 4];
    recv.read_exact(&mut len_buf)
        .await
        .map_err(|e| ChallengeError::WireProtocol(e.to_string()))?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > MAX_MESSAGE_SIZE {
        return Err(ChallengeError::WireProtocol(format!(
            "incoming message too large: {len} bytes (max {MAX_MESSAGE_SIZE})"
        )));
    }
    let mut buf = vec![0u8; len];
    recv.read_exact(&mut buf)
        .await
        .map_err(|e| ChallengeError::WireProtocol(e.to_string()))?;
    serde_json::from_slice(&buf).map_err(|e| ChallengeError::WireProtocol(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    const ED25519_MULTICODEC: [u8; 2] = [0xed, 0x01];

    fn did_from_signing_key(sk: &SigningKey) -> String {
        let mut bytes = Vec::with_capacity(34);
        bytes.extend_from_slice(&ED25519_MULTICODEC);
        bytes.extend_from_slice(sk.verifying_key().as_bytes());
        format!("did:key:z{}", bs58::encode(bytes).into_string())
    }

    /// Helper: build a well-formed Response for the given nonce + endpoints.
    fn build_response(
        sk: &SigningKey,
        nonce: &[u8],
        client_endpoint_id: &str,
        server_endpoint_id: &str,
    ) -> Response {
        let did = did_from_signing_key(sk);
        let sig_input = build_sig_input(nonce, client_endpoint_id, server_endpoint_id);
        let sig = sk.sign(&sig_input);
        Response {
            v: PROTOCOL_VERSION,
            did,
            client_endpoint_id: client_endpoint_id.into(),
            signature: BASE64.encode(sig.to_bytes()),
        }
    }

    #[test]
    fn verify_response_accepts_valid_signature() {
        let sk = SigningKey::from_bytes(&[7u8; 32]);
        let nonce = [42u8; NONCE_LEN];
        let resp = build_response(&sk, &nonce, "client-ep", "server-ep");

        let did = verify_response(&resp, &nonce, "client-ep", "server-ep").unwrap();
        assert_eq!(did, did_from_signing_key(&sk));
    }

    #[test]
    fn verify_response_rejects_unsupported_version() {
        let sk = SigningKey::from_bytes(&[1u8; 32]);
        let nonce = [0u8; NONCE_LEN];
        let mut resp = build_response(&sk, &nonce, "c", "s");
        resp.v = 99;

        let err = verify_response(&resp, &nonce, "c", "s").unwrap_err();
        assert!(matches!(err, ChallengeError::UnsupportedVersion(99)));
    }

    #[test]
    fn verify_response_rejects_endpoint_id_mismatch() {
        let sk = SigningKey::from_bytes(&[2u8; 32]);
        let nonce = [0u8; NONCE_LEN];
        // Client signed claiming endpoint "client-A", server connected from "client-B"
        let resp = build_response(&sk, &nonce, "client-A", "s");

        let err = verify_response(&resp, &nonce, "client-B", "s").unwrap_err();
        assert!(matches!(err, ChallengeError::EndpointIdMismatch { .. }));
    }

    #[test]
    fn verify_response_rejects_malformed_did() {
        let sk = SigningKey::from_bytes(&[3u8; 32]);
        let nonce = [0u8; NONCE_LEN];
        let mut resp = build_response(&sk, &nonce, "c", "s");
        resp.did = "not-a-did-key".into();

        let err = verify_response(&resp, &nonce, "c", "s").unwrap_err();
        assert!(matches!(err, ChallengeError::MalformedDid(_)));
    }

    #[test]
    fn verify_response_rejects_tampered_signature() {
        let sk = SigningKey::from_bytes(&[4u8; 32]);
        let nonce = [0u8; NONCE_LEN];
        let mut resp = build_response(&sk, &nonce, "c", "s");
        // Decode → flip a byte → re-encode
        let mut sig = BASE64.decode(&resp.signature).unwrap();
        sig[0] ^= 0xFF;
        resp.signature = BASE64.encode(&sig);

        let err = verify_response(&resp, &nonce, "c", "s").unwrap_err();
        assert!(matches!(err, ChallengeError::SignatureInvalid));
    }

    #[test]
    fn verify_response_rejects_nonce_substitution() {
        // Attacker captures a valid Response for nonce N, replays under nonce N'.
        let sk = SigningKey::from_bytes(&[5u8; 32]);
        let nonce_original = [0u8; NONCE_LEN];
        let nonce_replay = [1u8; NONCE_LEN];
        let resp = build_response(&sk, &nonce_original, "c", "s");

        let err = verify_response(&resp, &nonce_replay, "c", "s").unwrap_err();
        assert!(matches!(err, ChallengeError::SignatureInvalid));
    }

    #[test]
    fn verify_response_rejects_server_substitution() {
        // Same client, same nonce, signed for server S1 but verified by S2.
        let sk = SigningKey::from_bytes(&[6u8; 32]);
        let nonce = [0u8; NONCE_LEN];
        let resp = build_response(&sk, &nonce, "c", "server-1");

        let err = verify_response(&resp, &nonce, "c", "server-2").unwrap_err();
        assert!(matches!(err, ChallengeError::SignatureInvalid));
    }

    #[test]
    fn verify_response_rejects_malformed_base64_signature() {
        let sk = SigningKey::from_bytes(&[8u8; 32]);
        let nonce = [0u8; NONCE_LEN];
        let mut resp = build_response(&sk, &nonce, "c", "s");
        resp.signature = "!!! not base64 !!!".into();

        let err = verify_response(&resp, &nonce, "c", "s").unwrap_err();
        assert!(matches!(err, ChallengeError::MalformedBase64));
    }

    #[test]
    fn verify_response_rejects_wrong_length_signature() {
        let sk = SigningKey::from_bytes(&[9u8; 32]);
        let nonce = [0u8; NONCE_LEN];
        let mut resp = build_response(&sk, &nonce, "c", "s");
        // Truncate to 16 bytes (valid base64, invalid length)
        resp.signature = BASE64.encode([0u8; 16]);

        let err = verify_response(&resp, &nonce, "c", "s").unwrap_err();
        assert!(matches!(err, ChallengeError::SignatureInvalid));
    }

    // Timeout behaviour is delegated to tokio::time::timeout — no dedicated
    // test here. Roundtrip coverage for `read_message` happens via the
    // client-side roundtrip tests in client.rs.
}
