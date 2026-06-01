//! Client-side of the QUIC DID-auth handshake.
//!
//! `respond_to_challenge` reads the server's Challenge, signs the canonical
//! payload with the caller's identity signing key, and writes the Response.
//! The caller is responsible for supplying the DID and signing key that match
//! the role the client wants to claim for this connection — there is no
//! implicit identity selection (see `space-join-did-selection` memory).

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use ed25519_dalek::{Signer, SigningKey};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::time::timeout;

use super::server::{ChallengeError, CHALLENGE_TIMEOUT};
use super::wire::{build_sig_input, Challenge, Response, MAX_MESSAGE_SIZE, NONCE_LEN, PROTOCOL_VERSION};

/// Run the client side of the handshake: read the server's Challenge, sign
/// the canonical payload, write the Response.
pub async fn respond_to_challenge<R, W>(
    send: &mut W,
    recv: &mut R,
    my_did: &str,
    my_signing_key: &SigningKey,
    own_endpoint_id: &str,
) -> Result<(), ChallengeError>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let challenge: Challenge = timeout(CHALLENGE_TIMEOUT, read_message(recv))
        .await
        .map_err(|_| ChallengeError::Timeout)??;

    if challenge.v != PROTOCOL_VERSION {
        return Err(ChallengeError::UnsupportedVersion(challenge.v));
    }

    let nonce_bytes = BASE64
        .decode(&challenge.nonce)
        .map_err(|_| ChallengeError::MalformedBase64)?;

    if nonce_bytes.len() != NONCE_LEN {
        return Err(ChallengeError::NonceLength {
            expected: NONCE_LEN,
            got: nonce_bytes.len(),
        });
    }

    let sig_input = build_sig_input(&nonce_bytes, own_endpoint_id, &challenge.server_endpoint_id);
    let signature = my_signing_key.sign(&sig_input);

    let response = Response {
        v: PROTOCOL_VERSION,
        did: my_did.to_string(),
        client_endpoint_id: own_endpoint_id.to_string(),
        signature: BASE64.encode(signature.to_bytes()),
    };

    write_message(send, &response).await
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
    use crate::quic_did_auth::server::challenge_and_verify;
    use tokio::io::duplex;

    const ED25519_MULTICODEC: [u8; 2] = [0xed, 0x01];

    fn did_from_signing_key(sk: &SigningKey) -> String {
        let mut bytes = Vec::with_capacity(34);
        bytes.extend_from_slice(&ED25519_MULTICODEC);
        bytes.extend_from_slice(sk.verifying_key().as_bytes());
        format!("did:key:z{}", bs58::encode(bytes).into_string())
    }

    /// Spawn the server task and the client task on opposite ends of two
    /// in-memory duplex pipes, run them to completion, return the server's
    /// verification result.
    async fn run_handshake(
        client_did: &str,
        client_sk: &SigningKey,
        client_endpoint: &str,
        server_endpoint: &str,
        // What the server *thinks* the remote endpoint id is — usually
        // identical to client_endpoint, but tests can decouple them.
        remote_endpoint_as_seen_by_server: &str,
    ) -> Result<String, ChallengeError> {
        let (s2c_writer, s2c_reader) = duplex(8 * 1024);
        let (c2s_writer, c2s_reader) = duplex(8 * 1024);

        let server_ep = server_endpoint.to_string();
        let remote_ep = remote_endpoint_as_seen_by_server.to_string();
        let server_task = tokio::spawn(async move {
            let mut send = s2c_writer;
            let mut recv = c2s_reader;
            challenge_and_verify(&mut send, &mut recv, &server_ep, &remote_ep).await
        });

        let did = client_did.to_string();
        let sk = client_sk.clone();
        let client_ep = client_endpoint.to_string();
        let client_task = tokio::spawn(async move {
            let mut send = c2s_writer;
            let mut recv = s2c_reader;
            respond_to_challenge(&mut send, &mut recv, &did, &sk, &client_ep).await
        });

        let server_result = server_task.await.unwrap();
        let _ = client_task.await.unwrap();
        server_result
    }

    #[tokio::test]
    async fn roundtrip_happy_path_returns_client_did() {
        let sk = SigningKey::from_bytes(&[11u8; 32]);
        let did = did_from_signing_key(&sk);

        let verified = run_handshake(&did, &sk, "client-ep", "server-ep", "client-ep")
            .await
            .unwrap();
        assert_eq!(verified, did);
    }

    #[tokio::test]
    async fn roundtrip_rejects_when_client_endpoint_lies() {
        // Client signs claiming endpoint "lies-A", server-side iroh reports "real-B".
        let sk = SigningKey::from_bytes(&[12u8; 32]);
        let did = did_from_signing_key(&sk);

        let err = run_handshake(&did, &sk, "lies-A", "server-ep", "real-B")
            .await
            .unwrap_err();
        assert!(matches!(err, ChallengeError::EndpointIdMismatch { .. }));
    }

    #[tokio::test]
    async fn roundtrip_two_distinct_clients_get_distinct_dids() {
        let sk_a = SigningKey::from_bytes(&[1u8; 32]);
        let sk_b = SigningKey::from_bytes(&[2u8; 32]);
        let did_a = did_from_signing_key(&sk_a);
        let did_b = did_from_signing_key(&sk_b);

        let got_a = run_handshake(&did_a, &sk_a, "ep-a", "server", "ep-a").await.unwrap();
        let got_b = run_handshake(&did_b, &sk_b, "ep-b", "server", "ep-b").await.unwrap();
        assert_eq!(got_a, did_a);
        assert_eq!(got_b, did_b);
        assert_ne!(got_a, got_b);
    }
}
