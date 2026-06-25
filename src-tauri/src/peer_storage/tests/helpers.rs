use std::collections::{HashMap, HashSet};

use base64::Engine as _;
use ed25519_dalek::{Signer, SigningKey};

use crate::peer_storage::endpoint::PeerEndpoint;

pub(super) const BASE64URL: base64::engine::GeneralPurpose = base64::engine::GeneralPurpose::new(
    &base64::alphabet::URL_SAFE,
    base64::engine::general_purpose::NO_PAD,
);
pub(super) const ED25519_MULTICODEC: [u8; 2] = [0xed, 0x01];

pub(super) fn did_from_signing_key(key: &SigningKey) -> String {
    let mut bytes = Vec::with_capacity(34);
    bytes.extend_from_slice(&ED25519_MULTICODEC);
    bytes.extend_from_slice(key.verifying_key().as_bytes());
    format!("did:key:z{}", bs58::encode(bytes).into_string())
}

/// Mint a UCAN for `space_id` with the given capability, signed by the
/// audience key. Mirrors the test helper used by
/// `ucan::verify::tests::make_test_token`, kept inline here so the
/// peer_storage tests have no cross-module test dependency.
pub(super) fn mint_ucan(
    signer: &SigningKey,
    space_id: &str,
    capability: &str,
    audience: &str,
) -> String {
    let issuer_did = did_from_signing_key(signer);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let header = serde_json::json!({"alg": "EdDSA", "typ": "JWT"});
    let payload = serde_json::json!({
        "ucv": "1.0",
        "iss": issuer_did,
        "aud": audience,
        "cap": { format!("space:{}", space_id): capability },
        "exp": now + 3600,
        "iat": now,
        "prf": [],
        "nnc": "test-nonce"
    });
    let header_b64 = BASE64URL.encode(serde_json::to_string(&header).unwrap().as_bytes());
    let payload_b64 = BASE64URL.encode(serde_json::to_string(&payload).unwrap().as_bytes());
    let signing_input = format!("{}.{}", header_b64, payload_b64);
    let signature = signer.sign(signing_input.as_bytes());
    format!(
        "{}.{}.{}",
        header_b64,
        payload_b64,
        BASE64URL.encode(signature.to_bytes())
    )
}

pub(super) fn read_ucan(signer: &SigningKey, space_id: &str, audience: &str) -> String {
    mint_ucan(signer, space_id, "space/read", audience)
}

pub(super) fn write_ucan(signer: &SigningKey, space_id: &str, audience: &str) -> String {
    mint_ucan(signer, space_id, "space/write", audience)
}

pub(super) struct Harness {
    // Kept alive so the bound iroh endpoint + accept loop keep running
    // for the duration of the test, even though we never call methods
    // on `server` directly after setup.
    pub(super) _server: PeerEndpoint,
    pub(super) client: PeerEndpoint,
    pub(super) server_remote_id: iroh::EndpointId,
    pub(super) share_name: String,
    pub(super) ucan: String,
    /// The client's verified DID — UCANs whose `aud` does not match this
    /// are rejected by the Layer 1.25 audience check in handle_stream.
    pub(super) client_did: String,
    pub(super) _tmp: tempfile::TempDir,
}

/// Spin up two local PeerEndpoints. Server hosts a 1 MiB ramp file under
/// share "media" / space "test-space". Client is registered as an allowed
/// peer for that space and has a fresh QUIC connection cached so
/// `open_stream` will reuse it without needing relay/address lookup.
pub(super) async fn setup_harness() -> Harness {
    let tmp = tempfile::tempdir().unwrap();
    let file_path = tmp.path().join("ramp.bin");
    let mut ramp = vec![0u8; 1024 * 1024];
    for (i, b) in ramp.iter_mut().enumerate() {
        *b = (i % 256) as u8;
    }
    tokio::fs::write(&file_path, &ramp).await.unwrap();

    let share_name = "media".to_string();
    let space_id = "test-space".to_string();

    // --- Server side ---
    let mut server = PeerEndpoint::new_ephemeral();
    server.set_random_test_identity();
    let server_id = server.start_for_test().await.expect("server bind");
    server
        .add_share(
            "share-1".to_string(),
            share_name.clone(),
            tmp.path().to_string_lossy().to_string(),
            space_id.clone(),
        )
        .await;

    // --- Client side ---
    let mut client = PeerEndpoint::new_ephemeral();
    let client_did = client.set_random_test_identity();
    client.start_for_test().await.expect("client bind");
    let client_id = client.endpoint_id();

    // Grant the client read access to the space on the server.
    let mut allowed = HashMap::new();
    let mut spaces = HashSet::new();
    spaces.insert(space_id.clone());
    allowed.insert(client_id.to_string(), spaces);
    server.set_allowed_peers(allowed).await;

    // Mirror the production load: handle_connection cross-checks the
    // crypto-verified DID against this map, so tests must populate it
    // with the client's expected owner DID.
    let mut owner_dids = HashMap::new();
    owner_dids.insert(client_id.to_string(), client_did.clone());
    server.set_peer_owner_dids(owner_dids).await;

    // Server endpoint addr (full, with direct addrs since RelayMode::Disabled).
    let server_addr = server.endpoint_ref().unwrap().addr();
    client
        .connect_for_test(server_addr)
        .await
        .expect("client → server connect");

    // Sign the UCAN with a fresh issuer key — the server's capability
    // check verifies the token signature but does not require iss ==
    // client EndpointId. The audience MUST equal the client's verified
    // DID so the Layer 1.25 audience check in handle_stream passes.
    let seed: [u8; 32] = rand::random();
    let ucan_signer = SigningKey::from_bytes(&seed);
    let ucan = read_ucan(&ucan_signer, &space_id, &client_did);

    Harness {
        _server: server,
        client,
        server_remote_id: server_id,
        share_name,
        ucan,
        client_did,
        _tmp: tmp,
    }
}

/// A harness variant where the client is wrapped in `Arc<RwLock<PeerEndpoint>>`
/// so it can be passed directly to `read_multipart_to_file`.
pub(super) struct MultipartHarness {
    pub(super) _server: PeerEndpoint,
    pub(super) client: std::sync::Arc<tokio::sync::RwLock<PeerEndpoint>>,
    pub(super) server_remote_id: iroh::EndpointId,
    pub(super) share_name: String,
    pub(super) ucan: String,
    pub(super) _tmp: tempfile::TempDir,
}

pub(super) async fn setup_multipart_harness() -> MultipartHarness {
    let tmp = tempfile::tempdir().unwrap();
    let file_path = tmp.path().join("ramp.bin");
    let mut ramp = vec![0u8; 1024 * 1024];
    for (i, b) in ramp.iter_mut().enumerate() {
        *b = (i % 256) as u8;
    }
    tokio::fs::write(&file_path, &ramp).await.unwrap();

    let share_name = "media".to_string();
    let space_id = "test-space".to_string();

    let mut server = PeerEndpoint::new_ephemeral();
    server.set_random_test_identity();
    let server_id = server.start_for_test().await.expect("server bind");
    server
        .add_share(
            "share-1".to_string(),
            share_name.clone(),
            tmp.path().to_string_lossy().to_string(),
            space_id.clone(),
        )
        .await;

    let mut client_inner = PeerEndpoint::new_ephemeral();
    let client_did = client_inner.set_random_test_identity();
    client_inner.start_for_test().await.expect("client bind");
    let client_id = client_inner.endpoint_id();

    let mut allowed = HashMap::new();
    let mut spaces = HashSet::new();
    spaces.insert(space_id.clone());
    allowed.insert(client_id.to_string(), spaces);
    server.set_allowed_peers(allowed).await;

    let mut owner_dids = HashMap::new();
    owner_dids.insert(client_id.to_string(), client_did.clone());
    server.set_peer_owner_dids(owner_dids).await;

    let server_addr = server.endpoint_ref().unwrap().addr();
    client_inner
        .connect_for_test(server_addr)
        .await
        .expect("client → server connect");

    let seed: [u8; 32] = rand::random();
    let ucan_signer = ed25519_dalek::SigningKey::from_bytes(&seed);
    let ucan = read_ucan(&ucan_signer, &space_id, &client_did);

    let client = std::sync::Arc::new(tokio::sync::RwLock::new(client_inner));

    MultipartHarness {
        _server: server,
        client,
        server_remote_id: server_id,
        share_name,
        ucan,
        _tmp: tmp,
    }
}
