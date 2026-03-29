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

    pub fn init_identity(&self) -> Result<MlsIdentityInfo, String> {
        let credential = BasicCredential::new(vec![]);
        let signer = SignatureKeyPair::new(CIPHERSUITE.signature_algorithm())
            .map_err(|e| format!("Failed to generate signature key pair: {e}"))?;
        signer.store(self.provider.storage())
            .map_err(|e| format!("Failed to store signature key pair: {e}"))?;

        self.provider.storage().store_own_identity_key(&signer.to_public_vec())
            .map_err(|e| format!("Failed to store identity key: {e}"))?;

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

        let (commit, welcome, _group_info) = group
            .add_members(&self.provider, &signer, &[key_package])
            .map_err(|e| format!("Failed to add member: {e}"))?;

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
                Ok(Vec::new())
            }
            _ => Err("Unknown message type".to_string()),
        }
    }

    pub fn process_message(&self, space_id: &str, message: &[u8]) -> Result<Vec<u8>, String> {
        self.decrypt(space_id, message)
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

    /// Export the current epoch's sync encryption key.
    /// Uses MLS export_secret (RFC 9420 §8.5) to derive a 32-byte symmetric key,
    /// then stores it locally so it can be retrieved for decryption later.
    pub fn export_epoch_key(&self, space_id: &str) -> Result<MlsEpochKey, String> {
        let group_id = GroupId::from_slice(space_id.as_bytes());
        let group = MlsGroup::load(self.provider.storage(), &group_id)
            .map_err(|e| format!("Failed to load group: {e}"))?
            .ok_or_else(|| format!("Group not found for space: {space_id}"))?;

        let epoch = group.epoch().as_u64();
        let key = group
            .export_secret(self.provider.crypto(), "haex-vault-sync", &[], 32)
            .map_err(|e| format!("Failed to export secret: {e}"))?;

        // Persist so we can decrypt historical data from this epoch
        self.provider.storage().store_sync_key(space_id, epoch, &key)
            .map_err(|e| format!("Failed to store sync key: {e}"))?;

        Ok(MlsEpochKey { epoch, key })
    }

    /// Retrieve a previously stored sync encryption key for a specific epoch.
    pub fn get_epoch_key(&self, space_id: &str, epoch: u64) -> Result<MlsEpochKey, String> {
        let key = self.provider.storage().load_sync_key(space_id, epoch)
            .map_err(|e| format!("Failed to load sync key: {e}"))?
            .ok_or_else(|| format!("No sync key found for space {space_id} epoch {epoch}"))?;

        Ok(MlsEpochKey { epoch, key })
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

    fn get_credential_with_key(&self, signer: &SignatureKeyPair) -> CredentialWithKey {
        let credential = BasicCredential::new(vec![]);
        CredentialWithKey {
            credential: credential.into(),
            signature_key: signer.to_public_vec().into(),
        }
    }
}
