//! Space delivery protocol types over QUIC streams.
//!
//! Request/response protocol for MLS delivery and CRDT sync in local spaces.

use serde::{Deserialize, Serialize};

/// ALPN protocol identifier for space delivery
pub const ALPN: &[u8] = b"haex-delivery/1";

/// Maximum request size (10 MB — CRDT changes can be large)
const MAX_REQUEST_SIZE: usize = 10 * 1024 * 1024;

/// Maximum response size (10 MB)
const MAX_RESPONSE_SIZE: usize = 10 * 1024 * 1024;

// ============================================================================
// Request types
// ============================================================================

/// All request types for the space delivery protocol.
/// Tagged by `op` field for JSON serialization.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Request {
    // -- MLS Delivery --
    /// Upload key packages for a DID in a space
    MlsUploadKeyPackages {
        space_id: String,
        /// Base64-encoded key packages
        packages: Vec<String>,
    },
    /// Fetch a key package for a target DID
    MlsFetchKeyPackage {
        space_id: String,
        target_did: String,
    },
    /// Send an MLS message (commit, proposal, application)
    MlsSendMessage {
        space_id: String,
        /// Base64-encoded MLS message
        message: String,
        message_type: String,
    },
    /// Fetch MLS messages after a given ID
    MlsFetchMessages {
        space_id: String,
        after_id: Option<i64>,
    },
    /// Send a welcome message to a specific recipient
    MlsSendWelcome {
        space_id: String,
        recipient_did: String,
        /// Base64-encoded welcome message
        welcome: String,
    },
    /// Fetch welcome messages for the caller
    MlsFetchWelcomes {
        space_id: String,
    },

    // -- CRDT Sync --
    /// Push CRDT changes to the leader
    SyncPush {
        space_id: String,
        /// JSON-serialized CRDT changes (same format as server push)
        changes: serde_json::Value,
    },
    /// Pull CRDT changes from the leader
    SyncPull {
        space_id: String,
        after_timestamp: Option<String>,
    },

    // -- Identity --
    /// Announce identity to the leader (sent on connect)
    Announce {
        did: String,
        endpoint_id: String,
        /// Optional claims the peer chooses to share
        label: Option<String>,
        claims: Option<Vec<IdentityClaim>>,
    },

    // -- Invites --
    /// Claim an invite token. Invitee sends token + KeyPackages.
    /// Leader validates, creates UCAN, adds to MLS group, returns Welcome.
    ClaimInvite {
        space_id: String,
        /// The invite token ID
        token: String,
        /// Invitee's DID
        did: String,
        /// Invitee's endpoint ID
        endpoint_id: String,
        /// Base64-encoded MLS KeyPackages
        key_packages: Vec<String>,
        /// Optional label to share with the leader
        label: Option<String>,
        /// SPKI Base64 public key for haex_space_members (derived from DID on sender side)
        public_key: Option<String>,
    },

    // -- Push Invites (peer-to-peer, inviter → invitee) --
    /// Push an invite directly to a peer's device.
    /// The invitee creates a dummy space + pending invite locally.
    PushInvite {
        space_id: String,
        space_name: String,
        space_type: String,
        token_id: String,
        capabilities: Vec<String>,
        include_history: bool,
        inviter_did: String,
        inviter_label: Option<String>,
        /// All known space device EndpointIds — invitee tries each until one answers ClaimInvite
        space_endpoints: Vec<String>,
        origin_url: Option<String>,
        /// RFC3339 deadline
        expires_at: String,
    },
}

// ============================================================================
// Response types
// ============================================================================

/// All response types for the space delivery protocol.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    /// Success with no data
    Ok,
    /// MLS message stored, returns ID
    MessageStored { message_id: i64 },
    /// Single key package
    KeyPackage {
        /// Base64-encoded
        package: String,
    },
    /// List of MLS messages
    Messages { messages: Vec<MlsMessageEntry> },
    /// List of welcome messages
    Welcomes {
        /// Base64-encoded welcomes
        welcomes: Vec<String>,
    },
    /// CRDT sync changes
    SyncChanges { changes: serde_json::Value },
    /// Invite claimed successfully — includes MLS welcome and delegated UCAN
    InviteClaimed {
        /// Base64-encoded MLS welcome message
        welcome: String,
        /// The delegated UCAN token for this member
        ucan: String,
        /// The capability granted (e.g. "space/write")
        capability: String,
    },
    /// Acknowledgment for a push invite
    PushInviteAck {
        accepted: bool,
    },
    /// Error response
    Error { message: String },
}

// ============================================================================
// Notification types (pushed from leader to peers over long-lived stream)
// ============================================================================

/// Notifications pushed from leader to connected peers.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Notification {
    /// New sync data available
    Sync { space_id: String, tables: Vec<String> },
    /// New MLS message available
    Mls { space_id: String, message_type: String },
    /// New invite available
    Invite { space_id: String, invite_id: String },
    /// Leader is shutting down (handoff or stop)
    LeaderStopping,
}

// ============================================================================
// Supporting types
// ============================================================================

/// An MLS message stored in the leader's buffer
#[derive(Debug, Serialize, Deserialize)]
pub struct MlsMessageEntry {
    pub id: i64,
    pub sender_did: String,
    pub message_type: String,
    /// Base64-encoded
    pub message: String,
    pub created_at: String,
}

/// An identity claim shared by a peer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityClaim {
    pub claim_type: String,
    pub value: String,
}

// ============================================================================
// Wire format helpers (reuse pattern from peer_storage)
// ============================================================================

use crate::peer_storage::protocol::PeerProtocolError;

/// Encode a message to bytes (length-prefixed JSON)
pub fn encode<T: Serialize>(msg: &T) -> Result<Vec<u8>, serde_json::Error> {
    let json = serde_json::to_vec(msg)?;
    let len = (json.len() as u32).to_be_bytes();
    let mut buf = Vec::with_capacity(4 + json.len());
    buf.extend_from_slice(&len);
    buf.extend_from_slice(&json);
    Ok(buf)
}

/// Read a request from a QUIC receive stream
pub async fn read_request(
    recv: &mut iroh::endpoint::RecvStream,
) -> Result<Request, PeerProtocolError> {
    crate::peer_storage::protocol::read_message(recv, MAX_REQUEST_SIZE).await
}

/// Read a response from a QUIC receive stream
pub async fn read_response(
    recv: &mut iroh::endpoint::RecvStream,
) -> Result<Response, PeerProtocolError> {
    crate::peer_storage::protocol::read_message(recv, MAX_RESPONSE_SIZE).await
}

