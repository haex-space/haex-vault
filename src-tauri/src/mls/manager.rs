use std::sync::{Arc, Mutex};

use openmls::prelude::tls_codec::Serialize as TlsSerializeTrait;
use openmls::prelude::*;
use openmls_basic_credential::SignatureKeyPair;
use openmls_traits::OpenMlsProvider;
use rusqlite::Connection;

use crate::mls::authorization::PresentedCapability;
use crate::mls::provider::HaexMlsProvider;
use crate::mls::storage::SqlCipherMlsStorage;
use crate::mls::types::{MlsCommitBundle, MlsEpochKey, MlsGroupInfo, MlsIdentityInfo};

const CIPHERSUITE: Ciphersuite = Ciphersuite::MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519;

pub struct MlsManager {
    provider: HaexMlsProvider,
    /// Kept for read-only application-policy queries (membership, capability)
    /// that run alongside MLS operations. The provider's storage adapter
    /// clones from the same Arc.
    conn: Arc<Mutex<Option<Connection>>>,
}

impl MlsManager {
    pub fn new(conn: Arc<Mutex<Option<Connection>>>) -> Self {
        let storage = SqlCipherMlsStorage { conn: conn.clone() };
        Self {
            provider: HaexMlsProvider::new(storage),
            conn,
        }
    }

    pub fn init_tables(&self) -> Result<(), String> {
        self.provider
            .storage()
            .init_tables()
            .map_err(|e| format!("Failed to init MLS tables: {e}"))
    }

    pub fn init_identity(&self, did: &str) -> Result<MlsIdentityInfo, String> {
        // Return existing identity if one exists (idempotent)
        if let Ok(Some(pub_key)) = self.provider.storage().load_own_identity_key() {
            // Update stored DID (may have changed)
            self.provider
                .storage()
                .store_own_did(did)
                .map_err(|e| format!("Failed to store DID: {e}"))?;

            let credential = BasicCredential::new(did.as_bytes().to_vec());
            let credential_with_key = CredentialWithKey {
                credential: credential.into(),
                signature_key: pub_key.clone().into(),
            };
            return Ok(MlsIdentityInfo {
                signature_public_key: pub_key,
                credential: credential_with_key.credential.serialized_content().to_vec(),
            });
        }

        // Create new identity
        let credential = BasicCredential::new(did.as_bytes().to_vec());
        let signer = SignatureKeyPair::new(CIPHERSUITE.signature_algorithm())
            .map_err(|e| format!("Failed to generate signature key pair: {e}"))?;
        signer
            .store(self.provider.storage())
            .map_err(|e| format!("Failed to store signature key pair: {e}"))?;

        self.provider
            .storage()
            .store_own_identity_key(&signer.to_public_vec())
            .map_err(|e| format!("Failed to store identity key: {e}"))?;

        self.provider
            .storage()
            .store_own_did(did)
            .map_err(|e| format!("Failed to store DID: {e}"))?;

        let credential_with_key = CredentialWithKey {
            credential: credential.into(),
            signature_key: signer.to_public_vec().into(),
        };

        Ok(MlsIdentityInfo {
            signature_public_key: signer.to_public_vec(),
            credential: credential_with_key.credential.serialized_content().to_vec(),
        })
    }

    pub fn create_group(&self, space_id: &str) -> Result<MlsGroupInfo, String> {
        let signer = self.get_signer()?;
        let credential_with_key = self.get_credential_with_key(&signer);

        let group_id = GroupId::from_slice(space_id.as_bytes());
        let group_config = MlsGroupCreateConfig::builder()
            .ciphersuite(CIPHERSUITE)
            .use_ratchet_tree_extension(true)
            .build();

        let group = MlsGroup::new_with_group_id(
            &self.provider,
            &signer,
            &group_config,
            group_id,
            credential_with_key,
        )
        .map_err(|e| format!("Failed to create MLS group: {e}"))?;

        Ok(MlsGroupInfo {
            group_id: space_id.to_string(),
            epoch: group.epoch().as_u64(),
            member_count: group.members().count() as u32,
        })
    }

    pub fn add_member(
        &self,
        space_id: &str,
        key_package_bytes: &[u8],
        expected_did: &str,
        pop: &[u8],
    ) -> Result<MlsCommitBundle, String> {
        let signer = self.get_signer()?;
        let group_id = GroupId::from_slice(space_id.as_bytes());
        let mut group = MlsGroup::load(self.provider.storage(), &group_id)
            .map_err(|e| format!("Failed to load group: {e}"))?
            .ok_or_else(|| format!("Group not found for space: {space_id}"))?;

        let key_package_in = KeyPackageIn::tls_deserialize_exact_bytes(key_package_bytes)
            .map_err(|e| format!("Failed to deserialize key package: {e}"))?;

        let key_package = key_package_in
            .validate(self.provider.crypto(), ProtocolVersion::Mls10)
            .map_err(|e| format!("Invalid key package: {e}"))?;

        // The KeyPackage carries a self-asserted DID string as its BasicCredential.
        // Reject any package whose credential does not match the invitee we intended
        // to add — otherwise an attacker who possesses ANY valid KeyPackage could be
        // added under someone else's name. (Proof-of-possession of the identity key
        // is enforced separately right below.)
        let cred_bytes = key_package.leaf_node().credential().serialized_content();
        if cred_bytes != expected_did.as_bytes() {
            return Err(format!(
                "credential DID mismatch: expected {expected_did}, got {}",
                String::from_utf8_lossy(cred_bytes)
            ));
        }

        // Proof-of-possession. The authoritative source is now the PoP
        // carried as a leaf-node extension inside the KeyPackage itself
        // (`mls::pop::HAEX_POP_EXTENSION_TYPE`) — that is what remote
        // receivers verify in `mls::authorization`. Extract + verify it
        // here on the local Add path too, so a malformed or stripped KP
        // fails fast rather than round-tripping through delivery.
        //
        // The explicit `pop` argument is retained for the transitional
        // leader-side plumbing (`buffer::store_key_package` /
        // `haex_local_delivery_key_packages_no_sync.pop_blob`); we
        // cross-check it against the leaf-embedded PoP so a bug in that
        // plumbing (mismatched pair, wrong KP for a PoP) cannot slip past
        // this layer. A follow-up will drop the parallel plumbing once
        // the extension is the single source of truth end-to-end.
        let identity_pub = crate::ucan::public_key_from_did(expected_did)
            .map_err(|e| format!("Cannot resolve identity key from DID {expected_did}: {e}"))?;
        let mls_sig_pub: &[u8; 32] = key_package
            .leaf_node()
            .signature_key()
            .as_slice()
            .try_into()
            .map_err(|_| "MLS signature key is not 32 bytes".to_string())?;
        let embedded_pop = crate::mls::pop::extract_pop_from_leaf(key_package.leaf_node())
            .ok_or_else(|| {
                "KeyPackage is missing the required proof-of-possession leaf extension".to_string()
            })?;
        crate::mls::pop::verify_pop(&identity_pub, mls_sig_pub, expected_did, &embedded_pop)
            .map_err(|e| format!("Invalid proof-of-possession in KeyPackage extension: {e}"))?;
        if pop != embedded_pop.to_bytes().as_slice() {
            return Err(
                "Passed proof-of-possession does not match the KeyPackage-embedded PoP \
                 (leader-plumbing mismatch)"
                    .to_string(),
            );
        }

        // Check for duplicate signature key in existing group members.
        // This can happen on re-invite after partial success or retry scenarios.
        //
        // Not gated by `authorization::authorize_local_removal`: the leaf
        // being cleaned up here belongs to the SAME `expected_did` already
        // authorized (via PoP + credential checks above) to be (re-)added —
        // this is a self-cleanup of the invitee's own stale prior leaf, not
        // a removal of a different member. See that function's docs for why
        // `add_member` as a whole is not gated by the local committer's own
        // capability (invite-token authority model, not device authority).
        let new_sig_key = key_package.leaf_node().signature_key().as_slice().to_vec();
        let own_leaf = group.own_leaf_index();
        let conflicting_index = group
            .members()
            .find(|m| m.index != own_leaf && m.signature_key == new_sig_key.as_slice())
            .map(|m| m.index);

        if let Some(leaf_index) = conflicting_index {
            eprintln!(
                "[MLS] Duplicate signature key at leaf {leaf_index:?} in group {space_id} — removing before re-add"
            );
            group
                .remove_members(&self.provider, &signer, &[leaf_index])
                .map_err(|e| format!("Failed to remove conflicting member: {e}"))?;
            group
                .merge_pending_commit(&self.provider)
                .map_err(|e| format!("Failed to merge remove commit: {e}"))?;
        }

        let (commit, welcome, _group_info) = group
            .add_members(&self.provider, &signer, &[key_package])
            .map_err(|e| {
                let member_keys: Vec<String> = group.members()
                    .map(|m| hex::encode(&m.signature_key[..8.min(m.signature_key.len())]))
                    .collect();
                format!(
                    "Failed to add member: {e} (group has {} members, sig_keys: [{:?}], new_key: {})",
                    member_keys.len(),
                    member_keys.join(", "),
                    hex::encode(&new_sig_key[..8.min(new_sig_key.len())]),
                )
            })?;

        group
            .merge_pending_commit(&self.provider)
            .map_err(|e| format!("Failed to merge commit: {e}"))?;

        let commit_bytes = commit
            .tls_serialize_detached()
            .map_err(|e| format!("Failed to serialize commit: {e}"))?;

        let welcome_bytes = welcome
            .tls_serialize_detached()
            .map_err(|e| format!("Failed to serialize welcome: {e}"))?;

        let group_info_bytes = group
            .export_group_info(self.provider.crypto(), &signer, true)
            .map_err(|e| format!("Failed to export group info: {e}"))?
            .tls_serialize_detached()
            .map_err(|e| format!("Failed to serialize group info: {e}"))?;

        // Adds do not carry a receive-side committer-capability proof
        // (plan §5.0 — leader-relay-Add case). The KeyPackage's own PoP
        // and Phase-1 addee-membership check bound the Add; nothing on
        // the wire encodes a "the leader relaying this ClaimInvite held
        // Invite-or-higher itself" claim.
        Ok(MlsCommitBundle {
            commit: commit_bytes,
            welcome: Some(welcome_bytes),
            group_info: group_info_bytes,
            committer_ucan: None,
            committer_commit_bind_sig: None,
        })
    }

    /// Resolve `(committer_did, target_did)` for a Remove of `leaf_index`
    /// in `space_id`. Shared by [`Self::remove_member`] and its e2e-hooks
    /// sibling [`Self::remove_member_unchecked`] so the two cannot drift on
    /// which principal signs a commit.
    ///
    /// Both DIDs are lifted from the group leaves themselves (via
    /// `authorization::did_from_credential`), which returns `None` for
    /// anything that is not a UTF-8 BasicCredential. A lossy conversion
    /// would fail OPEN: the mangled string could never match a
    /// `haex_identities.did`, so `is_space_member` would answer
    /// "already left" and skip the local gate entirely.
    ///
    /// The committer DID comes from our own leaf in this group, NOT from
    /// `get_own_did()` — the latter is a single device-global value
    /// (`storage::store_own_did`) that a later `init_identity` with a
    /// different default identity would overwrite while the group leaf
    /// keeps the DID it was created/joined with. Authorizing anything
    /// other than the signing DID would gate the wrong principal.
    fn resolve_removal_dids(
        &self,
        group: &MlsGroup,
        space_id: &str,
        leaf_index: LeafNodeIndex,
        member_index: u32,
    ) -> Result<(String, String), String> {
        let resolve_did = |m: &Member, what: &str| -> Result<String, String> {
            crate::mls::authorization::did_from_credential(&m.credential).ok_or_else(|| {
                format!(
                    "Cannot resolve a DID from the {what} credential at leaf {} in space \
                     {space_id} (not a UTF-8 BasicCredential)",
                    m.index.u32()
                )
            })
        };
        let target_did = group
            .members()
            .find(|m| m.index == leaf_index)
            .ok_or_else(|| format!("No member at leaf index {member_index} in space {space_id}"))
            .and_then(|m| resolve_did(&m, "removal target's"))?;
        let own_leaf = group.own_leaf_index();
        let committer_did = group
            .members()
            .find(|m| m.index == own_leaf)
            .ok_or_else(|| format!("Own leaf {own_leaf:?} not present in space {space_id}"))
            .and_then(|m| resolve_did(&m, "own"))?;
        Ok((committer_did, target_did))
    }

    pub fn remove_member(
        &self,
        space_id: &str,
        member_index: u32,
    ) -> Result<MlsCommitBundle, String> {
        let signer = self.get_signer()?;
        let group_id = GroupId::from_slice(space_id.as_bytes());
        let mut group = MlsGroup::load(self.provider.storage(), &group_id)
            .map_err(|e| format!("Failed to load group: {e}"))?
            .ok_or_else(|| format!("Group not found for space: {space_id}"))?;

        let leaf_index = LeafNodeIndex::new(member_index);

        // This commit never passes through `decrypt`/`authorization::inspect`
        // (only INCOMING commits do), so nothing else gates it. Require the
        // local committer to hold Invite-or-higher unless the target has
        // already left — see `authorization::authorize_local_removal` for
        // the full rationale (leader-side rekey-after-self-leave exemption).
        //
        // See `resolve_removal_dids` for why we lift both DIDs from the
        // group leaves themselves (and reject lossy UTF-8 conversions).
        let (committer_did, target_did) =
            self.resolve_removal_dids(&group, space_id, leaf_index, member_index)?;
        // `proof_required` mirrors the receiver's own target-gone exemption
        // in `authorize_committer_capability`: `true` iff the target is
        // still an active member (so the receive-side gate will demand a
        // proof too), `false` iff the target already left (leader-rekey
        // case — no proof needed, and this device may not even hold one).
        let proof_required = crate::mls::authorization::authorize_local_removal(
            &self.conn,
            space_id,
            &committer_did,
            &target_did,
        )?;

        let (commit, _welcome, _group_info) = group
            .remove_members(&self.provider, &signer, &[leaf_index])
            .map_err(|e| format!("Failed to remove member: {e}"))?;

        group
            .merge_pending_commit(&self.provider)
            .map_err(|e| format!("Failed to merge commit: {e}"))?;

        let commit_bytes = commit
            .tls_serialize_detached()
            .map_err(|e| format!("Failed to serialize commit: {e}"))?;

        let group_info_bytes = group
            .export_group_info(self.provider.crypto(), &signer, true)
            .map_err(|e| format!("Failed to export group info: {e}"))?
            .tls_serialize_detached()
            .map_err(|e| format!("Failed to serialize group info: {e}"))?;

        // Plan §6: attach a committer-capability proof only when the
        // receive-side gate will actually require one. Loading the UCAN by
        // `committer_did` (not `get_own_did()`) is deliberate — see the
        // resolution above the DID's own comment; the same reasoning that
        // picked the signing DID applies to the capability lookup.
        let (committer_ucan, committer_commit_bind_sig) = if proof_required {
            let db = crate::database::DbConnection(self.conn.clone());
            let ucan_token = crate::space_delivery::local::ucan::load_active_ucan_for_audience(
                &db,
                space_id,
                &committer_did,
                &[crate::ucan::Cap::Invite, crate::ucan::Cap::Admin],
            )
            .map_err(|e| format!("Failed to load committer UCAN for space {space_id}: {e}"))?
            .ok_or_else(|| {
                format!(
                    "Committer {committer_did} holds no UCAN for space {space_id} despite \
                     passing the local capability gate (data inconsistency)"
                )
            })?;

            let identity = crate::space_delivery::local::quic_retry::load_signing_identity_for_did(
                &db,
                &committer_did,
            )
            .map_err(|e| format!("Failed to load identity signing key for {committer_did}: {e}"))?;
            let sig =
                crate::mls::commit_bind::sign_commit_bind(&identity.signing_key, &commit_bytes);
            (Some(ucan_token), Some(sig.to_bytes().to_vec()))
        } else {
            (None, None)
        };

        Ok(MlsCommitBundle {
            commit: commit_bytes,
            welcome: None,
            group_info: group_info_bytes,
            committer_ucan,
            committer_commit_bind_sig,
        })
    }

    /// Current MLS epoch of `space_id`'s group.
    pub fn current_epoch(&self, space_id: &str) -> Result<u64, String> {
        let group_id = GroupId::from_slice(space_id.as_bytes());
        MlsGroup::load(self.provider.storage(), &group_id)
            .map_err(|e| format!("Failed to load group: {e}"))?
            .ok_or_else(|| format!("Group not found for space: {space_id}"))
            .map(|g| g.epoch().as_u64())
    }

    /// Gate-free sibling of [`MlsManager::remove_member`] for e2e attack
    /// specs: produces a cryptographically valid Remove commit WITHOUT
    /// `authorization::authorize_local_removal` and without attaching a
    /// committer UCAN. Always returns a commit-bind signature over the
    /// produced bytes, signed with this device's real identity key — so a
    /// spec can pair it with any (forged, expired, replayed, absent) UCAN.
    ///
    /// Merges the pending commit locally, exactly like the production path:
    /// this vault's group advances an epoch and diverges from every honest
    /// peer that rejects the commit. Each attack spec must therefore use its
    /// own space.
    #[cfg(feature = "e2e-hooks")]
    pub fn remove_member_unchecked(
        &self,
        space_id: &str,
        member_index: u32,
    ) -> Result<(Vec<u8>, Vec<u8>, String, String), String> {
        let signer = self.get_signer()?;
        let group_id = GroupId::from_slice(space_id.as_bytes());
        let mut group = MlsGroup::load(self.provider.storage(), &group_id)
            .map_err(|e| format!("Failed to load group: {e}"))?
            .ok_or_else(|| format!("Group not found for space: {space_id}"))?;

        let leaf_index = LeafNodeIndex::new(member_index);

        // Delegate to the shared resolver so this attack-shape commit is
        // signed by the same principal `remove_member` (production) would
        // sign with — otherwise a later drift in the resolution rule would
        // silently exercise a stale identity through these specs.
        let (committer_did, target_did) =
            self.resolve_removal_dids(&group, space_id, leaf_index, member_index)?;

        let (commit, _welcome, _group_info) = group
            .remove_members(&self.provider, &signer, &[leaf_index])
            .map_err(|e| format!("Failed to remove member: {e}"))?;

        group
            .merge_pending_commit(&self.provider)
            .map_err(|e| format!("Failed to merge commit: {e}"))?;

        let commit_bytes = commit
            .tls_serialize_detached()
            .map_err(|e| format!("Failed to serialize commit: {e}"))?;

        let db = crate::database::DbConnection(self.conn.clone());
        let identity = crate::space_delivery::local::quic_retry::load_signing_identity_for_did(
            &db,
            &committer_did,
        )
        .map_err(|e| format!("Failed to load identity signing key for {committer_did}: {e}"))?;
        let sig = crate::mls::commit_bind::sign_commit_bind(&identity.signing_key, &commit_bytes);

        Ok((
            commit_bytes,
            sig.to_bytes().to_vec(),
            committer_did,
            target_did,
        ))
    }

    pub fn encrypt(&self, space_id: &str, plaintext: &[u8]) -> Result<Vec<u8>, String> {
        let signer = self.get_signer()?;
        let group_id = GroupId::from_slice(space_id.as_bytes());
        let mut group = MlsGroup::load(self.provider.storage(), &group_id)
            .map_err(|e| format!("Failed to load group: {e}"))?
            .ok_or_else(|| format!("Group not found for space: {space_id}"))?;

        let msg = group
            .create_message(&self.provider, &signer, plaintext)
            .map_err(|e| format!("Failed to encrypt: {e}"))?;

        msg.tls_serialize_detached()
            .map_err(|e| format!("Failed to serialize message: {e}"))
    }

    /// `presented_capability` and `presented_commit_bind_sig` carry the
    /// plan-§5.8 receive-side committer-capability proof: an already
    /// UCAN-chain-verified capability (see
    /// `space_delivery::local::ucan::resolve_presented_committer_capability`,
    /// which owns the chain-walk) plus the raw ed25519 signature bytes
    /// binding that capability to this exact commit. `None` for anything
    /// that doesn't carry the proof (application messages, Adds, key
    /// rotations, self-leaves, callers outside local/P2P delivery). See
    /// `mls::authorization::authorize_committer_capability`.
    pub fn decrypt(
        &self,
        space_id: &str,
        ciphertext: &[u8],
        presented_capability: Option<PresentedCapability>,
        presented_commit_bind_sig: Option<&[u8]>,
    ) -> Result<Vec<u8>, String> {
        let group_id = GroupId::from_slice(space_id.as_bytes());
        let mut group = MlsGroup::load(self.provider.storage(), &group_id)
            .map_err(|e| format!("Failed to load group: {e}"))?
            .ok_or_else(|| format!("Group not found for space: {space_id}"))?;

        let mls_message_in = MlsMessageIn::tls_deserialize_exact_bytes(ciphertext)
            .map_err(|e| format!("Failed to deserialize message: {e}"))?;

        let protocol_message = mls_message_in
            .try_into_protocol_message()
            .map_err(|e| format!("Not a protocol message: {e}"))?;

        let processed = group
            .process_message(&self.provider, protocol_message)
            .map_err(|e| format!("Failed to process message: {e}"))?;

        // Snapshot sender + credential BEFORE `into_content` consumes `processed`.
        // Both feed the authorization inspector for the StagedCommit arm.
        let sender = processed.sender().clone();
        let committer_credential = processed.credential().clone();

        match processed.into_content() {
            ProcessedMessageContent::ApplicationMessage(app_msg) => Ok(app_msg.into_bytes()),
            ProcessedMessageContent::StagedCommitMessage(staged_commit) => {
                // Phase-1 + Phase-2 + Phase-3 membership-change authorization.
                // Runs BEFORE `merge_staged_commit` so a rejected commit does
                // not advance the local epoch. See `mls::authorization`.
                let facts = crate::mls::authorization::inspect(
                    &sender,
                    &committer_credential,
                    &staged_commit,
                    &group,
                );
                crate::mls::authorization::authorize(&self.conn, space_id, &facts)?;
                crate::mls::authorization::verify_pops(space_id, &facts)?;

                // A presented capability must verify against THIS commit's
                // exact bytes before it is trusted at all — independent of
                // whether the commit even needs one (fail-closed on a
                // malformed/mismatched proof rather than silently ignoring
                // it). `ciphertext` here is the same commit bytes the
                // sender hashed when producing the bind signature
                // (`MlsManager::remove_member`).
                if let Some(cap) = presented_capability.as_ref() {
                    let sig_bytes = presented_commit_bind_sig.ok_or_else(|| {
                        format!(
                            "Rejecting MLS commit for space {space_id}: committer capability \
                             presented without a commit-bind signature"
                        )
                    })?;
                    crate::mls::commit_bind::verify_commit_bind_bytes(
                        &cap.audience_did,
                        ciphertext,
                        sig_bytes,
                    )
                    .map_err(|e| {
                        format!(
                            "Rejecting MLS commit for space {space_id}: commit-bind signature \
                             invalid for committer {}: {e}",
                            cap.audience_did
                        )
                    })?;
                }
                crate::mls::authorization::authorize_committer_capability(
                    &self.conn,
                    space_id,
                    &facts,
                    presented_capability.as_ref(),
                )?;

                group
                    .merge_staged_commit(&self.provider, *staged_commit)
                    .map_err(|e| format!("Failed to merge staged commit: {e}"))?;
                Ok(Vec::new())
            }
            ProcessedMessageContent::ProposalMessage(_) => {
                eprintln!(
                    "[MLS] Unexpected ProposalMessage received for space {space_id}, ignoring"
                );
                Ok(Vec::new())
            }
            _ => Err("Unknown message type".to_string()),
        }
    }

    pub fn process_message(
        &self,
        space_id: &str,
        message: &[u8],
        presented_capability: Option<PresentedCapability>,
        presented_commit_bind_sig: Option<&[u8]>,
    ) -> Result<Vec<u8>, String> {
        self.decrypt(
            space_id,
            message,
            presented_capability,
            presented_commit_bind_sig,
        )
    }

    /// Process an MLS Welcome message to join an existing group.
    /// Creates the local group state from the Welcome (the group does NOT need to exist yet).
    pub fn process_welcome(
        &self,
        space_id: &str,
        welcome_bytes: &[u8],
    ) -> Result<MlsGroupInfo, String> {
        let mls_message_in = MlsMessageIn::tls_deserialize_exact_bytes(welcome_bytes)
            .map_err(|e| format!("Failed to deserialize welcome message: {e}"))?;
        let welcome = match mls_message_in.extract() {
            MlsMessageBodyIn::Welcome(w) => w,
            _ => {
                return Err(
                    "Expected Welcome message but got a different MLS message type".to_string(),
                )
            }
        };

        // If a stale MLS group exists for this space (e.g. from a prior membership
        // that was removed/declined), delete it before joining with the new Welcome.
        // Surface both load and delete failures: a storage error here is not
        // benign — silently treating "load failed" as "no stale group" would
        // proceed to join, and a partial delete followed by a new join can
        // leave keying material from two epochs interleaved in storage,
        // permanently locking this device out of the new group.
        let expected_group_id = GroupId::from_slice(space_id.as_bytes());
        match MlsGroup::load(self.provider.storage(), &expected_group_id) {
            Ok(Some(mut old_group)) => {
                eprintln!("[MLS] Deleting stale group for space {space_id} before re-joining");
                old_group
                    .delete(self.provider.storage())
                    .map_err(|e| format!("Failed to delete stale group before re-join: {e}"))?;
            }
            Ok(None) => {}
            Err(e) => {
                return Err(format!(
                    "Failed to load existing MLS group for space {space_id}: {e}"
                ));
            }
        }

        let group_config = MlsGroupJoinConfig::builder()
            .use_ratchet_tree_extension(true)
            .build();

        let group = StagedWelcome::new_from_welcome(&self.provider, &group_config, welcome, None)
            .map_err(|e| format!("Failed to stage welcome: {e}"))?
            .into_group(&self.provider)
            .map_err(|e| format!("Failed to join group from welcome: {e}"))?;

        // Verify the group ID matches the expected space
        if group.group_id() != &expected_group_id {
            return Err(format!(
                "Group ID mismatch: expected {} but welcome contains {}",
                space_id,
                String::from_utf8_lossy(group.group_id().as_slice()),
            ));
        }

        Ok(MlsGroupInfo {
            group_id: space_id.to_string(),
            epoch: group.epoch().as_u64(),
            member_count: group.members().count() as u32,
        })
    }

    /// Generate `count` fresh KeyPackages, each paired with a proof-of-possession
    /// binding its MLS signature key to `identity_signing_key`. The identity key
    /// lives in `haex_identities`, outside the MLS provider's own storage, so the
    /// caller resolves and passes it in rather than this module reaching across
    /// into an unrelated table.
    ///
    /// Every KeyPackage additionally carries the PoP as a leaf-node extension
    /// under [`crate::mls::pop::HAEX_POP_EXTENSION_TYPE`], so a receiver can
    /// verify the DID↔MLS-key binding on incoming Add proposals without an
    /// out-of-band channel. External-commit joiners are not covered by this
    /// extension (openmls 0.8.1's external commit builder does not expose
    /// leaf-node extensions); those still rely on the Phase-1 addee-DID
    /// membership check in [`crate::mls::authorization`]. The tuple's second
    /// element (the raw PoP bytes) is retained for the transitional
    /// leader-side plumbing (`haex_local_delivery_key_packages_no_sync.pop_blob`)
    /// — receivers authoritatively use the embedded extension.
    pub fn generate_key_packages(
        &self,
        count: u32,
        identity_signing_key: &ed25519_dalek::SigningKey,
    ) -> Result<Vec<(Vec<u8>, Vec<u8>)>, String> {
        let signer = self.get_signer()?;
        let credential_with_key = self.get_credential_with_key(&signer);
        let own_did = String::from_utf8_lossy(credential_with_key.credential.serialized_content())
            .into_owned();

        // The vault uses a single MLS signature key per identity (see
        // `init_identity`), so every KeyPackage this call mints shares the
        // same `mls_sig_pub` — one PoP signature covers them all. Compute it
        // once and clone the resulting extension into each KP's leaf.
        let mls_sig_pub_vec = signer.to_public_vec();
        let mls_sig_pub: &[u8; 32] = mls_sig_pub_vec
            .as_slice()
            .try_into()
            .map_err(|_| "MLS signature key is not 32 bytes".to_string())?;
        let pop = crate::mls::pop::sign_pop(identity_signing_key, mls_sig_pub, &own_did);
        let pop_bytes = pop.to_bytes().to_vec();
        let leaf_extensions = Extensions::single(crate::mls::pop::pop_leaf_extension(&pop))
            .map_err(|e| format!("Failed to build PoP leaf extension list: {e:?}"))?;
        let leaf_capabilities = Capabilities::builder()
            .extensions(vec![ExtensionType::Unknown(
                crate::mls::pop::HAEX_POP_EXTENSION_TYPE,
            )])
            .build();

        let mut packages = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let bundle = KeyPackage::builder()
                .leaf_node_capabilities(leaf_capabilities.clone())
                .leaf_node_extensions(leaf_extensions.clone())
                .build(
                    CIPHERSUITE,
                    &self.provider,
                    &signer,
                    credential_with_key.clone(),
                )
                .map_err(|e| format!("Failed to build key package: {e}"))?;

            let bytes = bundle
                .key_package()
                .tls_serialize_detached()
                .map_err(|e| format!("Failed to serialize key package: {e}"))?;
            packages.push((bytes, pop_bytes.clone()));
        }
        Ok(packages)
    }

    /// The DID this manager's MLS identity is bound to (set by `init_identity`).
    pub fn get_own_did(&self) -> Result<String, String> {
        self.provider
            .storage()
            .load_own_did()
            .map_err(|e| format!("Failed to read own DID: {e}"))?
            .ok_or_else(|| "No identity found. Call mls_init_identity first.".to_string())
    }

    /// Check if this device has an MLS group for the given space.
    pub fn has_group(&self, space_id: &str) -> bool {
        let group_id = GroupId::from_slice(space_id.as_bytes());
        matches!(
            MlsGroup::load(self.provider.storage(), &group_id),
            Ok(Some(_))
        )
    }

    /// Derive the current epoch's sync encryption key from MLS group state.
    /// Uses MLS export_secret (RFC 9420 §8.5) to derive a 32-byte symmetric key.
    /// Caller is responsible for persisting the key via CRDT.
    pub fn derive_epoch_key(&self, space_id: &str) -> Result<MlsEpochKey, String> {
        let group_id = GroupId::from_slice(space_id.as_bytes());
        let group = MlsGroup::load(self.provider.storage(), &group_id)
            .map_err(|e| format!("Failed to load group: {e}"))?
            .ok_or_else(|| format!("Group not found for space: {space_id}"))?;

        let epoch = group.epoch().as_u64();
        let key = group
            .export_secret(self.provider.crypto(), "haex-vault-sync", &[], 32)
            .map_err(|e| format!("Failed to export secret: {e}"))?;

        Ok(MlsEpochKey { epoch, key })
    }

    /// Export the current GroupInfo for a space, including ratchet tree.
    /// Used by the leader to serve External Commit rejoin requests.
    pub fn get_group_info(&self, space_id: &str) -> Result<Vec<u8>, String> {
        let signer = self.get_signer()?;
        let group_id = GroupId::from_slice(space_id.as_bytes());
        let group = MlsGroup::load(self.provider.storage(), &group_id)
            .map_err(|e| format!("Failed to load group: {e}"))?
            .ok_or_else(|| format!("Group not found for space: {space_id}"))?;

        group
            .export_group_info(self.provider.crypto(), &signer, true)
            .map_err(|e| format!("Failed to export group info: {e}"))?
            .tls_serialize_detached()
            .map_err(|e| format!("Failed to serialize group info: {e}"))
    }

    /// Join a group via External Commit using a GroupInfo blob.
    /// The blob is a TLS-serialized MlsMessageOut (from export_group_info).
    /// Returns the commit bytes (to be sent to the leader/server) and the new epoch key.
    pub fn join_by_external_commit(
        &self,
        space_id: &str,
        group_info_bytes: &[u8],
    ) -> Result<(Vec<u8>, MlsEpochKey), String> {
        let signer = self.get_signer()?;
        let credential_with_key = self.get_credential_with_key(&signer);

        // The GroupInfo is wrapped in an MlsMessage — extract it
        let mls_msg = MlsMessageIn::tls_deserialize_exact_bytes(group_info_bytes)
            .map_err(|e| format!("Failed to deserialize MLS message: {e}"))?;
        let verifiable_group_info = match mls_msg.extract() {
            MlsMessageBodyIn::GroupInfo(gi) => gi,
            other => {
                return Err(format!(
                    "Expected GroupInfo but got {:?}",
                    std::mem::discriminant(&other)
                ))
            }
        };

        let (mut group, commit_bundle) = MlsGroup::external_commit_builder()
            .with_config(
                MlsGroupJoinConfig::builder()
                    .use_ratchet_tree_extension(true)
                    .build(),
            )
            .build_group(&self.provider, verifiable_group_info, credential_with_key)
            .map_err(|e| format!("Failed to build external commit group: {e}"))?
            .load_psks(self.provider.storage())
            .map_err(|e| format!("Failed to load PSKs: {e}"))?
            .build(
                self.provider.rand(),
                self.provider.crypto(),
                &signer,
                |_| true,
            )
            .map_err(|e| format!("Failed to build external commit: {e}"))?
            .finalize(&self.provider)
            .map_err(|e| format!("Failed to finalize external commit: {e}"))?;
        let commit = commit_bundle.into_commit();

        // Verify group ID matches expected space
        let expected_group_id = GroupId::from_slice(space_id.as_bytes());
        if group.group_id() != &expected_group_id {
            return Err(format!(
                "Group ID mismatch: expected {space_id} but GroupInfo contains {}",
                String::from_utf8_lossy(group.group_id().as_slice()),
            ));
        }

        group
            .merge_pending_commit(&self.provider)
            .map_err(|e| format!("Failed to merge external commit: {e}"))?;

        let epoch = group.epoch().as_u64();
        let key = group
            .export_secret(self.provider.crypto(), "haex-vault-sync", &[], 32)
            .map_err(|e| format!("Failed to export secret: {e}"))?;

        let commit_bytes = commit
            .tls_serialize_detached()
            .map_err(|e| format!("Failed to serialize commit: {e}"))?;

        Ok((commit_bytes, MlsEpochKey { epoch, key }))
    }

    fn get_signer(&self) -> Result<SignatureKeyPair, String> {
        let pub_key_bytes = self
            .provider
            .storage()
            .load_own_identity_key()
            .map_err(|e| format!("Failed to read identity: {e}"))?
            .ok_or_else(|| "No identity found. Call mls_init_identity first.".to_string())?;

        SignatureKeyPair::read(
            self.provider.storage(),
            &pub_key_bytes,
            CIPHERSUITE.signature_algorithm(),
        )
        .ok_or_else(|| "Signature key pair not found in storage".to_string())
    }

    pub fn find_member_index_by_did(
        &self,
        space_id: &str,
        target_did: &str,
    ) -> Result<Option<u32>, String> {
        let group_id = GroupId::from_slice(space_id.as_bytes());
        let group = MlsGroup::load(self.provider.storage(), &group_id)
            .map_err(|e| format!("Failed to load group: {e}"))?
            .ok_or_else(|| format!("Group not found for space: {space_id}"))?;

        let target_bytes = target_did.as_bytes();
        for member in group.members() {
            if member.credential.serialized_content() == target_bytes {
                return Ok(Some(member.index.u32()));
            }
        }
        Ok(None)
    }

    fn get_credential_with_key(&self, signer: &SignatureKeyPair) -> CredentialWithKey {
        let did_bytes = self
            .provider
            .storage()
            .load_own_did()
            .ok()
            .flatten()
            .map(|d| d.into_bytes())
            .unwrap_or_default();
        let credential = BasicCredential::new(did_bytes);
        CredentialWithKey {
            credential: credential.into(),
            signature_key: signer.to_public_vec().into(),
        }
    }
}

#[cfg(test)]
#[path = "manager_tests.rs"]
mod tests;
