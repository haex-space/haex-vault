use std::sync::{Arc, Mutex};

use openmls::prelude::tls_codec::Serialize as TlsSerializeTrait;
use openmls::prelude::*;
use openmls_basic_credential::SignatureKeyPair;
use openmls_traits::OpenMlsProvider;
use rusqlite::Connection;

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

        Ok(MlsCommitBundle {
            commit: commit_bytes,
            welcome: Some(welcome_bytes),
            group_info: group_info_bytes,
        })
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

        Ok(MlsCommitBundle {
            commit: commit_bytes,
            welcome: None,
            group_info: group_info_bytes,
        })
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

    pub fn decrypt(&self, space_id: &str, ciphertext: &[u8]) -> Result<Vec<u8>, String> {
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
                crate::mls::authorization::authorize_committer_capability(
                    &self.conn, space_id, &facts,
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

    pub fn process_message(&self, space_id: &str, message: &[u8]) -> Result<Vec<u8>, String> {
        self.decrypt(space_id, message)
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
