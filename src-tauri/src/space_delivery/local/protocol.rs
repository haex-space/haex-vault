//! Space delivery protocol types over QUIC streams.
//!
//! Request/response protocol for MLS delivery and CRDT sync in local spaces.

use serde::{Deserialize, Serialize};

use crate::ucan::CapabilityLevel;

/// ALPN protocol identifier for space delivery.
///
/// Version bumped from `haex-delivery/1` to `haex-delivery/2` when Phase 2 of
/// the quic_did_auth refactor introduced the server-initiated handshake on
/// the first bidirectional stream and removed the payload `did` field from
/// Announce + ClaimInvite. A `haex-delivery/1` peer trying to connect to a
/// `haex-delivery/2` server (or vice versa) fails the QUIC TLS ALPN
/// negotiation immediately, rather than handshaking and then dropping at the
/// application layer — which keeps the wire break diagnosable and isolates
/// the binary-compat boundary in bisect to this single commit.
pub const ALPN: &[u8] = b"haex-delivery/2";

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
    /// Acknowledge successful processing of MLS messages (commits).
    /// Sent by peer after processing commits from MlsFetchMessages.
    MlsAckCommit {
        space_id: String,
        /// IDs of the messages that were successfully processed
        message_ids: Vec<i64>,
    },
    /// Query how many key packages the leader has stored for the calling peer.
    MlsKeyPackageCount {
        space_id: String,
    },
    /// Request rejoin via External Commit. Peer is stuck on old epoch.
    /// Leader responds with current GroupInfo so peer can create External Commit.
    RequestRejoin {
        space_id: String,
        /// UCAN token — deprecated wire field. The AuthGate consumes the
        /// connection-cached UCAN populated at Announce time, never this
        /// payload field. Kept on the wire as `Option<String>` for forward
        /// compatibility: future senders can omit it; older receivers that
        /// still expect it parse `Some(...)` from current senders.
        #[serde(default)]
        ucan_token: Option<String>,
    },
    /// Submit an External Commit to rejoin a group.
    /// Leader validates UCAN for the DID in the commit, then distributes it.
    SubmitExternalCommit {
        space_id: String,
        /// Base64-encoded MLS commit message
        commit: String,
        /// UCAN token — deprecated wire field (see `RequestRejoin::ucan_token`).
        #[serde(default)]
        ucan_token: Option<String>,
    },

    // -- CRDT Sync --
    /// Push CRDT changes to the leader.
    /// Requires UCAN with `space/write` capability for the target space.
    SyncPush {
        space_id: String,
        /// JSON-serialized CRDT changes (same format as server push)
        changes: serde_json::Value,
        /// UCAN token — deprecated wire field (see `RequestRejoin::ucan_token`).
        #[serde(default)]
        ucan_token: Option<String>,
    },
    /// Pull CRDT changes from the leader.
    /// Requires UCAN with `space/read` capability (or higher) for the target space.
    SyncPull {
        space_id: String,
        after_timestamp: Option<String>,
        /// UCAN token — deprecated wire field (see `RequestRejoin::ucan_token`).
        #[serde(default)]
        ucan_token: Option<String>,
    },

    // -- Identity --
    /// Announce identity to the leader (sent on connect).
    /// Requires UCAN with `space/read` capability (or higher) for the target space —
    /// the announce populates `haex_space_devices` which is space-scoped sync state.
    ///
    /// The peer's DID is no longer carried on the wire — it is established
    /// cryptographically by the quic_did_auth handshake at connection-accept
    /// time and bound to the connection. Carrying it in the payload was a
    /// trust hazard (plan §1.3 / §4.2).
    Announce {
        endpoint_id: String,
        space_id: String,
        label: Option<String>,
        claims: Option<Vec<IdentityClaim>>,
        /// UCAN token — required for Announce since this call bootstraps the
        /// AuthGate's cached `ValidatedUcan` for the rest of the connection.
        /// Optional on the wire (forward-compat shape across all request
        /// variants), but receivers reject `None` here — the cache cannot
        /// be populated without a token. Enforcement lives in
        /// `leader.rs::handle_delivery_request` (Announce arm) before
        /// `require_valid_ucan` runs. Other request variants treat the
        /// field as truly optional.
        #[serde(default)]
        ucan_token: Option<String>,
    },

    // -- Invites --
    /// Claim an invite token. Invitee sends token + KeyPackages.
    /// Leader validates, creates UCAN, adds to MLS group, returns Welcome.
    ///
    /// The invitee's DID is no longer carried on the wire — it is bound by
    /// the quic_did_auth handshake and read from the connection state. A
    /// payload-supplied DID would let any peer with knowledge of the token
    /// claim it under an arbitrary identity (plan §4.2 scenarios 1 + 2).
    ClaimInvite {
        space_id: String,
        /// The invite token ID
        token: String,
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
        inviter_avatar: Option<String>,
        inviter_avatar_options: Option<String>,
        /// All known space device EndpointIds — invitee tries each until one answers ClaimInvite
        space_endpoints: Vec<String>,
        origin_url: Option<String>,
        /// RFC3339 deadline
        expires_at: String,
        /// Inviter's configured relay URL. Stored on the invitee side so the
        /// inviter's haex_space_devices stub (seeded from this invite during
        /// accept) carries a working relay before the real CRDT-replicated
        /// row arrives. Without this, the first sync round after accept may
        /// have to rely on mDNS / hole-punching alone. Optional for backward
        /// compatibility with older senders.
        #[serde(default)]
        inviter_relay_url: Option<String>,
    },
}

impl Request {
    /// Returns the `space_id` this request targets.
    ///
    /// Every `Request` variant carries a `space_id` because every request is
    /// space-scoped — the unified AuthGate uses this to route the membership
    /// + capability lookup before dispatching to the variant-specific handler.
    pub fn space_id_of(&self) -> &str {
        match self {
            Request::Announce { space_id, .. }
            | Request::MlsUploadKeyPackages { space_id, .. }
            | Request::MlsFetchKeyPackage { space_id, .. }
            | Request::MlsSendMessage { space_id, .. }
            | Request::MlsFetchMessages { space_id, .. }
            | Request::MlsSendWelcome { space_id, .. }
            | Request::MlsFetchWelcomes { space_id, .. }
            | Request::MlsAckCommit { space_id, .. }
            | Request::MlsKeyPackageCount { space_id, .. }
            | Request::RequestRejoin { space_id, .. }
            | Request::SubmitExternalCommit { space_id, .. }
            | Request::SyncPush { space_id, .. }
            | Request::SyncPull { space_id, .. }
            | Request::ClaimInvite { space_id, .. }
            | Request::PushInvite { space_id, .. } => space_id,
        }
    }

    /// Returns the minimum `CapabilityLevel` required to dispatch this
    /// request, or `None` if it bypasses the AuthGate.
    ///
    /// For `Some(level)`, the level is a **minimum floor**: the gate (see
    /// `auth_gate::authorize_request`, arriving in Phase 3 of the
    /// unified-authgate refactor) permits any capability `>= level` via
    /// `require_capability`. So a `Write` member always satisfies a `Read`
    /// floor.
    ///
    /// - `Announce` bypasses because it bootstraps the membership cache the
    ///   gate would query — gating it against itself is circular.
    /// - `ClaimInvite` bypasses because authentication is by invite token,
    ///   not by capability — the claimer is not yet a member.
    /// - `PushInvite` bypasses because it is leader-internal delivery to the
    ///   invitee's device, not a membership-scoped operation.
    ///
    /// `RequestRejoin` and `SubmitExternalCommit` are deliberately classified
    /// as `Read` to mirror the existing inline UCAN checks in
    /// `leader.rs::dispatch_request` (search for `CapabilityLevel::Read` in
    /// the `RequestRejoin` / `SubmitExternalCommit` arms). This refactor must
    /// not change behaviour — a read-only member that has fallen out of MLS
    /// epoch can rejoin today, and must keep being able to rejoin after the
    /// inline checks are deleted in Phase 5.
    ///
    /// `SyncPush` is intentionally `Read` here. Per-batch refinement happens
    /// in `inbound_sync::authorize_inbound_sync_push` — pushes touching only
    /// membership-system tables (MEMBERSHIP_SYSTEM_TABLES) are allowed for
    /// any member, other tables require `Write`. The gate enforces only
    /// "must be a member to push at all"; the inbound-sync validator
    /// enforces the per-table refinement. Tightening this to `Write` would
    /// silently break read-only members trying to push their own
    /// membership / device / KeyPackage rows.
    ///
    /// **All MLS-protocol operations are `Read` at this gate.** UCAN
    /// capability is the *sole* mechanism for authorising space-content
    /// writes (`haex_peer_shares` and the file bytes those shares point at);
    /// MLS itself has no concept of "may write resource X" — it only knows
    /// "is in the group" / "is not in the group" — so MLS-message
    /// classification is the wrong tool for that question.
    ///
    /// MLS-group membership is a separate domain: every active space member
    /// is also an MLS-group member regardless of read/write capability, and
    /// the MLS state machine itself enforces what each member may do inside
    /// the group (signatures, epoch ordering, sender membership). Gating
    /// MLS-protocol traffic by `Write` conflates the two layers —
    /// concretely:
    ///
    /// - `MlsUploadKeyPackages`: the peer uploads its **own** KeyPackages
    ///   (the leader tags each row with `verified_did` and stores them in
    ///   the `_no_sync` buffer). A read-only member cannot inject packages
    ///   for any other DID and the rows never leave the leader. Without
    ///   this, read-only members exhaust the initial `ClaimInvite` batch
    ///   and can never refill — every later Welcome that needs their
    ///   KeyPackage silently fails and they fall out of the encrypted
    ///   group while still listed as an active member.
    /// - `MlsAckCommit`: pure bookkeeping per-DID — marks the caller's own
    ///   pending-commit entries as acked so the leader can clean up. Every
    ///   member must ack; gating it on `Write` strands read-only members'
    ///   acks forever and stops cleanup for the whole group.
    /// - `MlsSendMessage` / `MlsSendWelcome`: the leader is a relay. MLS
    ///   message validity (signatures, epoch, sender membership) is
    ///   enforced at the recipient by the MLS state machine; application
    ///   policy ("only admins may invite") is enforced at *invite-token
    ///   creation*, not at Welcome forwarding. An extra `Write` check
    ///   here is layered defense, but it is the wrong layer — it makes
    ///   "read-only space member" mean "second-class MLS member", which
    ///   does not exist in the protocol.
    pub fn required_capability(&self) -> Option<CapabilityLevel> {
        match self {
            Request::MlsFetchKeyPackage { .. }
            | Request::MlsFetchMessages { .. }
            | Request::MlsFetchWelcomes { .. }
            | Request::MlsKeyPackageCount { .. }
            | Request::MlsUploadKeyPackages { .. }
            | Request::MlsSendMessage { .. }
            | Request::MlsSendWelcome { .. }
            | Request::MlsAckCommit { .. }
            | Request::SyncPull { .. }
            | Request::SyncPush { .. }
            | Request::RequestRejoin { .. }
            | Request::SubmitExternalCommit { .. } => Some(CapabilityLevel::Read),

            Request::Announce { .. }
            | Request::ClaimInvite { .. }
            | Request::PushInvite { .. } => None,
        }
    }

    /// Returns a stable, PascalCase name for this request variant.
    ///
    /// Used as the `source` field of `haex_logs` rows written from the
    /// AuthGate's reject branches, so an operator triaging sync failures
    /// in-app can filter by op without parsing free-text messages.
    ///
    /// Note the deliberate case split: the on-the-wire JSON `op` tag is
    /// SCREAMING_SNAKE_CASE (`"SYNC_PUSH"`, set by the `serde(tag = "op",
    /// rename_all = "SCREAMING_SNAKE_CASE")` attribute on this enum), but
    /// `op_name()` returns *PascalCase* (`"SyncPush"`) to match the existing
    /// `log_to_db` `source` convention established by `leader.rs` calls
    /// like `log_to_db(..., "Announce", ...)` and
    /// `log_to_db(..., "ClaimInvite", ...)`. Keeping these tied to a
    /// single match-arm avoids the renaming-skew failure mode where the
    /// wire tag and the log source diverge silently.
    pub fn op_name(&self) -> &'static str {
        match self {
            Request::MlsUploadKeyPackages { .. } => "MlsUploadKeyPackages",
            Request::MlsFetchKeyPackage { .. } => "MlsFetchKeyPackage",
            Request::MlsSendMessage { .. } => "MlsSendMessage",
            Request::MlsFetchMessages { .. } => "MlsFetchMessages",
            Request::MlsSendWelcome { .. } => "MlsSendWelcome",
            Request::MlsFetchWelcomes { .. } => "MlsFetchWelcomes",
            Request::MlsAckCommit { .. } => "MlsAckCommit",
            Request::MlsKeyPackageCount { .. } => "MlsKeyPackageCount",
            Request::RequestRejoin { .. } => "RequestRejoin",
            Request::SubmitExternalCommit { .. } => "SubmitExternalCommit",
            Request::SyncPush { .. } => "SyncPush",
            Request::SyncPull { .. } => "SyncPull",
            Request::Announce { .. } => "Announce",
            Request::ClaimInvite { .. } => "ClaimInvite",
            Request::PushInvite { .. } => "PushInvite",
        }
    }
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
    /// GroupInfo for External Commit rejoin
    GroupInfo {
        /// Base64-encoded MLS GroupInfo (with ratchet tree)
        group_info: String,
    },
    /// Key package status: current count and how many the leader still needs
    KeyPackageCount {
        /// How many key packages the leader currently holds for this peer
        available: u32,
        /// How many more the leader wants the peer to upload (0 = sufficient)
        needed: u32,
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

#[cfg(test)]
#[path = "protocol_tests.rs"]
mod tests;
