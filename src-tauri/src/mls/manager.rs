use std::sync::{Arc, Mutex};

use openmls::prelude::*;
use openmls::prelude::tls_codec::Serialize as TlsSerializeTrait;
use openmls_basic_credential::SignatureKeyPair;
use openmls_traits::OpenMlsProvider;
use rusqlite::Connection;

use crate::mls::provider::HaexMlsProvider;
use crate::mls::storage::SqlCipherMlsStorage;
use crate::mls::types::{MlsCommitBundle, MlsEpochKey, MlsGroupInfo, MlsIdentityInfo};

const CIPHERSUITE: Ciphersuite = Ciphersuite::MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519;

pub struct MlsManager {
    provider: HaexMlsProvider,
}

impl MlsManager {
    pub fn new(conn: Arc<Mutex<Option<Connection>>>) -> Self {
        let storage = SqlCipherMlsStorage { conn };
        Self {
            provider: HaexMlsProvider::new(storage),
        }
    }

    pub fn init_tables(&self) -> Result<(), String> {
        self.provider.storage().init_tables()
            .map_err(|e| format!("Failed to init MLS tables: {e}"))
    }

    pub fn init_identity(&self, did: &str) -> Result<MlsIdentityInfo, String> {
        // Return existing identity if one exists (idempotent)
        if let Ok(Some(pub_key)) = self.provider.storage().load_own_identity_key() {
            // Update stored DID (may have changed)
            self.provider.storage().store_own_did(did)
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
        signer.store(self.provider.storage())
            .map_err(|e| format!("Failed to store signature key pair: {e}"))?;

        self.provider.storage().store_own_identity_key(&signer.to_public_vec())
            .map_err(|e| format!("Failed to store identity key: {e}"))?;

        self.provider.storage().store_own_did(did)
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
        ).map_err(|e| format!("Failed to create MLS group: {e}"))?;

        Ok(MlsGroupInfo {
            group_id: space_id.to_string(),
            epoch: group.epoch().as_u64(),
            member_count: group.members().count() as u32,
        })
    }

    pub fn add_member(&self, space_id: &str, key_package_bytes: &[u8]) -> Result<MlsCommitBundle, String> {
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

        // Check for duplicate signature key in existing group members.
        // This can happen on re-invite after partial success or retry scenarios.
        let new_sig_key = key_package.leaf_node().signature_key().as_slice().to_vec();
        let own_leaf = group.own_leaf_index();
        let conflicting_index = group.members()
            .find(|m| m.index != own_leaf && m.signature_key == new_sig_key.as_slice())
            .map(|m| m.index);

        if let Some(leaf_index) = conflicting_index {
            eprintln!(
                "[MLS] Duplicate signature key at leaf {leaf_index:?} in group {space_id} — removing before re-add"
            );
            group.remove_members(&self.provider, &signer, &[leaf_index])
                .map_err(|e| format!("Failed to remove conflicting member: {e}"))?;
            group.merge_pending_commit(&self.provider)
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

        group.merge_pending_commit(&self.provider)
            .map_err(|e| format!("Failed to merge commit: {e}"))?;

        let commit_bytes = commit.tls_serialize_detached()
            .map_err(|e| format!("Failed to serialize commit: {e}"))?;

        let welcome_bytes = welcome.tls_serialize_detached()
            .map_err(|e| format!("Failed to serialize welcome: {e}"))?;

        let group_info_bytes = group.export_group_info(self.provider.crypto(), &signer, true)
            .map_err(|e| format!("Failed to export group info: {e}"))?
            .tls_serialize_detached()
            .map_err(|e| format!("Failed to serialize group info: {e}"))?;

        Ok(MlsCommitBundle {
            commit: commit_bytes,
            welcome: Some(welcome_bytes),
            group_info: group_info_bytes,
        })
    }

    pub fn remove_member(&self, space_id: &str, member_index: u32) -> Result<MlsCommitBundle, String> {
        let signer = self.get_signer()?;
        let group_id = GroupId::from_slice(space_id.as_bytes());
        let mut group = MlsGroup::load(self.provider.storage(), &group_id)
            .map_err(|e| format!("Failed to load group: {e}"))?
            .ok_or_else(|| format!("Group not found for space: {space_id}"))?;

        let leaf_index = LeafNodeIndex::new(member_index);
        let (commit, _welcome, _group_info) = group
            .remove_members(&self.provider, &signer, &[leaf_index])
            .map_err(|e| format!("Failed to remove member: {e}"))?;

        group.merge_pending_commit(&self.provider)
            .map_err(|e| format!("Failed to merge commit: {e}"))?;

        let commit_bytes = commit.tls_serialize_detached()
            .map_err(|e| format!("Failed to serialize commit: {e}"))?;

        let group_info_bytes = group.export_group_info(self.provider.crypto(), &signer, true)
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

        let msg = group.create_message(&self.provider, &signer, plaintext)
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

        let protocol_message = mls_message_in.try_into_protocol_message()
            .map_err(|e| format!("Not a protocol message: {e}"))?;

        let processed = group.process_message(&self.provider, protocol_message)
            .map_err(|e| format!("Failed to process message: {e}"))?;

        match processed.into_content() {
            ProcessedMessageContent::ApplicationMessage(app_msg) => {
                Ok(app_msg.into_bytes())
            }
            ProcessedMessageContent::StagedCommitMessage(staged_commit) => {
                group.merge_staged_commit(&self.provider, *staged_commit)
                    .map_err(|e| format!("Failed to merge staged commit: {e}"))?;
                Ok(Vec::new())
            }
            ProcessedMessageContent::ProposalMessage(_) => {
                eprintln!("[MLS] Unexpected ProposalMessage received for space {space_id}, ignoring");
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
    pub fn process_welcome(&self, space_id: &str, welcome_bytes: &[u8]) -> Result<MlsGroupInfo, String> {
        let mls_message_in = MlsMessageIn::tls_deserialize_exact_bytes(welcome_bytes)
            .map_err(|e| format!("Failed to deserialize welcome message: {e}"))?;
        let welcome = match mls_message_in.extract() {
            MlsMessageBodyIn::Welcome(w) => w,
            _ => return Err("Expected Welcome message but got a different MLS message type".to_string()),
        };

        let group_config = MlsGroupJoinConfig::builder()
            .use_ratchet_tree_extension(true)
            .build();

        let group = StagedWelcome::new_from_welcome(&self.provider, &group_config, welcome, None)
            .map_err(|e| format!("Failed to stage welcome: {e}"))?
            .into_group(&self.provider)
            .map_err(|e| format!("Failed to join group from welcome: {e}"))?;

        // Verify the group ID matches the expected space
        let expected_group_id = GroupId::from_slice(space_id.as_bytes());
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

    pub fn generate_key_packages(&self, count: u32) -> Result<Vec<Vec<u8>>, String> {
        let signer = self.get_signer()?;
        let credential_with_key = self.get_credential_with_key(&signer);

        let mut packages = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let bundle = KeyPackage::builder()
                .build(CIPHERSUITE, &self.provider, &signer, credential_with_key.clone())
                .map_err(|e| format!("Failed to build key package: {e}"))?;

            let bytes = bundle.key_package().tls_serialize_detached()
                .map_err(|e| format!("Failed to serialize key package: {e}"))?;
            packages.push(bytes);
        }
        Ok(packages)
    }

    /// Check if this device has an MLS group for the given space.
    pub fn has_group(&self, space_id: &str) -> bool {
        let group_id = GroupId::from_slice(space_id.as_bytes());
        matches!(MlsGroup::load(self.provider.storage(), &group_id), Ok(Some(_)))
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
            other => return Err(format!("Expected GroupInfo but got {:?}", std::mem::discriminant(&other))),
        };

        let (mut group, commit, _group_info) = MlsGroup::join_by_external_commit(
            &self.provider,
            &signer,
            None, // no ratchet tree — it's embedded in the GroupInfo
            verifiable_group_info,
            &MlsGroupJoinConfig::builder()
                .use_ratchet_tree_extension(true)
                .build(),
            None, // no required capabilities
            None, // no leaf node extensions
            &[],  // no PSKs
            credential_with_key,
        )
        .map_err(|e| format!("Failed to create external commit: {e}"))?;

        // Verify group ID matches expected space
        let expected_group_id = GroupId::from_slice(space_id.as_bytes());
        if group.group_id() != &expected_group_id {
            return Err(format!(
                "Group ID mismatch: expected {space_id} but GroupInfo contains {}",
                String::from_utf8_lossy(group.group_id().as_slice()),
            ));
        }

        group.merge_pending_commit(&self.provider)
            .map_err(|e| format!("Failed to merge external commit: {e}"))?;

        let epoch = group.epoch().as_u64();
        let key = group
            .export_secret(self.provider.crypto(), "haex-vault-sync", &[], 32)
            .map_err(|e| format!("Failed to export secret: {e}"))?;

        let commit_bytes = commit.tls_serialize_detached()
            .map_err(|e| format!("Failed to serialize commit: {e}"))?;

        Ok((commit_bytes, MlsEpochKey { epoch, key }))
    }

    fn get_signer(&self) -> Result<SignatureKeyPair, String> {
        let pub_key_bytes = self.provider.storage()
            .load_own_identity_key()
            .map_err(|e| format!("Failed to read identity: {e}"))?
            .ok_or_else(|| "No identity found. Call mls_init_identity first.".to_string())?;

        SignatureKeyPair::read(
            self.provider.storage(),
            &pub_key_bytes,
            CIPHERSUITE.signature_algorithm(),
        ).ok_or_else(|| "Signature key pair not found in storage".to_string())
    }

    pub fn find_member_index_by_did(&self, space_id: &str, target_did: &str) -> Result<Option<u32>, String> {
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
        let did_bytes = self.provider.storage()
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
