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

/// Sign one UCAN payload with `signer` and the shared UCAN JWT wrapping.
fn sign_ucan_payload(signer: &SigningKey, payload: &serde_json::Value) -> String {
    let header = serde_json::json!({"alg": "EdDSA", "typ": "JWT"});
    let header_b64 = BASE64URL.encode(serde_json::to_string(&header).unwrap().as_bytes());
    let payload_b64 = BASE64URL.encode(serde_json::to_string(payload).unwrap().as_bytes());
    let signing_input = format!("{}.{}", header_b64, payload_b64);
    let signature = signer.sign(signing_input.as_bytes());
    format!(
        "{}.{}.{}",
        header_b64,
        payload_b64,
        BASE64URL.encode(signature.to_bytes())
    )
}

/// Mint a self-certifying two-hop UCAN chain:
///
/// - Root: `iss = aud = root_did`, `cap = space/admin`, `prf = []` — the
///   self-signed Space-Root that the Phase-2 walker terminates on.
/// - Leaf: `iss = root_did`, `aud = audience`, `cap = <capability>`,
///   `prf = [root_token]` — the delegated grant presented on the wire.
///
/// `space_id` MUST be derived from `root_did` via `derive_space_id` so the
/// self-certifying binding check inside `validate_token` succeeds.
pub(super) fn mint_delegated_ucan(
    root_signer: &SigningKey,
    space_id: &str,
    capability: &str,
    audience: &str,
) -> String {
    let root_did = did_from_signing_key(root_signer);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let root_payload = serde_json::json!({
        "ucv": "1.0",
        "iss": root_did,
        "aud": root_did,
        "cap": { format!("space:{}", space_id): "space/admin" },
        "exp": now + 3600,
        "iat": now,
        "prf": [],
        "nnc": "test-root-nonce"
    });
    let root_token = sign_ucan_payload(root_signer, &root_payload);
    let leaf_payload = serde_json::json!({
        "ucv": "1.0",
        "iss": root_did,
        "aud": audience,
        "cap": { format!("space:{}", space_id): capability },
        "exp": now + 3600,
        "iat": now,
        "prf": [root_token],
        "nnc": "test-leaf-nonce"
    });
    sign_ucan_payload(root_signer, &leaf_payload)
}

pub(super) fn read_ucan(root_signer: &SigningKey, space_id: &str, audience: &str) -> String {
    mint_delegated_ucan(root_signer, space_id, "space/read", audience)
}

pub(super) fn write_ucan(root_signer: &SigningKey, space_id: &str, audience: &str) -> String {
    mint_delegated_ucan(root_signer, space_id, "space/write", audience)
}

/// Derive a self-certifying `space_id` for `root_did` with a random 16-byte
/// nonce. Matches the algorithm implemented in `crate::ucan::space_id` and
/// used by the Phase-2 chain walker's root-binding check.
///
/// The nonce is materialised directly from the RNG (never a zero-init
/// buffer that's later overwritten) so CodeQL doesn't flag the local as
/// a hardcoded cryptographic value.
pub(super) fn derive_test_space_id(root_did: &str) -> String {
    use crate::ucan::space_id::{derive_space_id, NONCE_LEN};
    let nonce: [u8; NONCE_LEN] = rand::random();
    derive_space_id(root_did, &nonce)
}

/// Mint a fresh Space-Root key + self-certifying `space_id` bound to its
/// DID. Every peer_storage test now runs its own space so `space_id` must
/// match the root key that signs its UCAN chain — this is the single-call
/// factory that keeps the two in sync.
pub(super) fn mint_test_root_and_space() -> (SigningKey, String) {
    let seed: [u8; 32] = rand::random();
    let signer = SigningKey::from_bytes(&seed);
    let root_did = did_from_signing_key(&signer);
    let space_id = derive_test_space_id(&root_did);
    (signer, space_id)
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
    /// Self-certifying `space_id` derived from `ucan_root_signer.did`.
    /// Tests that mint their own UCANs (e.g. write tokens) must reuse this
    /// so the Phase-2 chain walker's root-binding check accepts the
    /// resolved root.
    pub(super) space_id: String,
    /// Ed25519 signing key of the Space-Root that `space_id` binds to.
    /// Reused by tests that need to mint additional UCANs alongside the
    /// default `ucan` field.
    pub(super) ucan_root_signer: SigningKey,
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
    // UCAN root key + self-certifying space_id must be minted together so the
    // Phase-2 verifier's `verify_space_id_binding` accepts the resolved root.
    let (ucan_signer, space_id) = mint_test_root_and_space();

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

    // The UCAN is a two-hop chain: self-signed admin root (issued to itself)
    // → read delegate to `client_did`. `space_id` was derived from the root
    // key above so `verify_space_id_binding` accepts the resolved root.
    let ucan = read_ucan(&ucan_signer, &space_id, &client_did);

    Harness {
        _server: server,
        client,
        server_remote_id: server_id,
        share_name,
        ucan,
        client_did,
        space_id,
        ucan_root_signer: ucan_signer,
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
    // UCAN root key + self-certifying space_id must be minted together so the
    // Phase-2 verifier's `verify_space_id_binding` accepts the resolved root.
    let (ucan_signer, space_id) = mint_test_root_and_space();

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

    // Two-hop chain rooted at `ucan_signer` (see setup comment above).
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
