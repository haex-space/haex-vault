//! Shared types for local space delivery.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Information about a connected peer (visible to admin)
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ConnectedPeer {
    pub endpoint_id: String,
    pub did: String,
    pub label: Option<String>,
    pub claims: Vec<PeerClaim>,
    pub connected_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PeerClaim {
    pub claim_type: String,
    pub value: String,
}

/// Status of the local delivery service
#[derive(Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DeliveryStatus {
    pub is_leader: bool,
    pub active_spaces: Vec<String>,
    pub connected_peers: Vec<ConnectedPeer>,
    pub buffered_messages: u32,
    pub buffered_welcomes: u32,
    pub buffered_key_packages: u32,
}

/// Information about the current leader for a space
#[derive(Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LeaderInfo {
    pub endpoint_id: String,
    pub priority: i32,
    pub space_id: String,
}

/// Result of a leader election, exposed to frontend.
#[derive(Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ElectionResultInfo {
    /// "leader", "peer", or "none"
    pub role: String,
    pub leader_endpoint_id: Option<String>,
    pub leader_priority: Option<i32>,
    pub leader_relay_url: Option<String>,
}

/// Info about a local invite token (exposed to frontend).
#[derive(Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LocalInviteInfo {
    pub id: String,
    pub target_did: Option<String>,
    pub capability: String,
    pub max_uses: u32,
    pub current_uses: u32,
    pub expires_at: String,
}

/// Result of claiming a local invite.
#[derive(Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ClaimInviteResult {
    pub space_id: String,
    pub capability: String,
}
