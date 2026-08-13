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
    /// UCAN token bytes the committer holds for the space. Present iff the
    /// commit is membership-changing AND the committing device holds
    /// `Invite`-or-higher. Absent for application messages, key rotations,
    /// self-leaves, and leader-rekey-after-self-leave (where every removed
    /// leaf's DID is already gone from `haex_space_members`, so the
    /// receiver's target-gone exemption applies and no proof is needed).
    /// Travels alongside `commit` from `mls_remove_member` down to
    /// `local_delivery_broadcast_commit` and out onto the wire via
    /// `Request::MlsSendMessage::committer_ucan`. See
    /// `mls::authorization::authorize_committer_capability` (plan §5.8).
    #[serde(default)]
    pub committer_ucan: Option<Vec<u8>>,
    /// ed25519 signature over
    /// `sha256("haex-mls-commit-bind-v1" || sha256(commit))`, produced
    /// with the identity key resolvable from `committer_ucan`'s
    /// `audience_did`. Prevents UCAN replay against a different commit.
    /// Present iff `committer_ucan` is present.
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
