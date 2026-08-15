use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct MlsIdentityInfo {
    pub signature_public_key: Vec<u8>,
    pub credential: Vec<u8>,
}

/// A freshly-generated KeyPackage paired with its proof-of-possession —
/// see `mls::pop` for what the signature attests.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct MlsKeyPackageWithPop {
    pub key_package: Vec<u8>,
    pub pop: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct MlsGroupInfo {
    pub group_id: String,
    pub epoch: u64,
    pub member_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct MlsCommitBundle {
    pub commit: Vec<u8>,
    pub welcome: Option<Vec<u8>>,
    pub group_info: Vec<u8>,
    /// Raw UCAN JWT token the committer holds for the space. A UCAN's own
    /// dot-separated segments are already base64url and safe as JSON text,
    /// so it is carried as a plain `String` — no outer base64 wrap on the
    /// wire, no lossy byte round-trip in storage.
    ///
    /// Only attached by `MlsManager::remove_member` today, and only when
    /// `authorize_local_removal` reports `proof_required = true` — i.e. the
    /// Remove targets an ACTIVE member and the committer holds
    /// `Invite`-or-higher. Absent for `add_member` (invite-token authority
    /// gates that upstream), self-leaves, and leader-rekey-after-self-leave
    /// where every removed leaf's DID is already gone from
    /// `haex_space_members` on this committer so the receiver's target-gone
    /// exemption applies. Travels from `remove_member` down to
    /// `local_delivery_broadcast_commit` and out onto the wire via
    /// `Request::MlsSendMessage::committer_ucan`. See
    /// `mls::authorization::authorize_committer_capability` (plan §5.8).
    #[serde(default)]
    pub committer_ucan: Option<String>,
    /// ed25519 signature over
    /// `sha256("haex-mls-commit-bind-v1" || sha256(commit))`, produced
    /// with the identity key resolvable from `committer_ucan`'s
    /// `audience_did`. Prevents UCAN replay against a different commit.
    /// Attached under the same `proof_required` condition as
    /// `committer_ucan`; present iff `committer_ucan` is present.
    #[serde(default)]
    pub committer_commit_bind_sig: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct MlsProcessedMessage {
    pub content_type: String,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct MlsEpochKey {
    pub epoch: u64,
    pub key: Vec<u8>,
}

/// Result of joining a group via External Commit.
/// Contains the commit (to be sent to server/leader) and the new epoch key.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct MlsExternalCommitResult {
    pub commit: Vec<u8>,
    pub epoch_key: MlsEpochKey,
}
