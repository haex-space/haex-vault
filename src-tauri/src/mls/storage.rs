use std::sync::{Arc, Mutex};

use openmls_traits::storage::{StorageProvider, CURRENT_VERSION, traits};
use rusqlite::Connection;
use serde::de::DeserializeOwned;
use serde::Serialize;

#[derive(Debug, Clone)]
pub struct SqlCipherMlsStorage {
    pub conn: Arc<Mutex<Option<Connection>>>,
}

#[derive(Debug, thiserror::Error)]
pub enum MlsStorageError {
    #[error("database error: {0}")]
    Database(String),
    #[error("serialization error: {0}")]
    Serialization(String),
    #[error("no database connection")]
    NoConnection,
}

impl SqlCipherMlsStorage {
    fn with_conn<T>(&self, f: impl FnOnce(&Connection) -> Result<T, MlsStorageError>) -> Result<T, MlsStorageError> {
        let guard = self.conn.lock().map_err(|e| MlsStorageError::Database(e.to_string()))?;
        let conn = guard.as_ref().ok_or(MlsStorageError::NoConnection)?;
        f(conn)
    }

    fn serialize_key(key: &impl Serialize) -> Result<Vec<u8>, MlsStorageError> {
        serde_json::to_vec(key).map_err(|e| MlsStorageError::Serialization(e.to_string()))
    }

    fn serialize_entity(entity: &impl Serialize) -> Result<Vec<u8>, MlsStorageError> {
        serde_json::to_vec(entity).map_err(|e| MlsStorageError::Serialization(e.to_string()))
    }

    fn deserialize_entity<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, MlsStorageError> {
        serde_json::from_slice(bytes).map_err(|e| MlsStorageError::Serialization(e.to_string()))
    }

    fn write_value(&self, store_type: &str, key: &impl Serialize, value: &impl Serialize) -> Result<(), MlsStorageError> {
        let key_bytes = Self::serialize_key(key)?;
        let value_blob = Self::serialize_entity(value)?;
        self.with_conn(|conn| {
            conn.execute(
                "INSERT OR REPLACE INTO haex_mls_values_no_sync (store_type, key_bytes, value_blob) VALUES (?1, ?2, ?3)",
                rusqlite::params![store_type, key_bytes, value_blob],
            ).map_err(|e| MlsStorageError::Database(e.to_string()))?;
            Ok(())
        })
    }

    fn read_value<T: DeserializeOwned>(&self, store_type: &str, key: &impl Serialize) -> Result<Option<T>, MlsStorageError> {
        let key_bytes = Self::serialize_key(key)?;
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT value_blob FROM haex_mls_values_no_sync WHERE store_type = ?1 AND key_bytes = ?2"
            ).map_err(|e| MlsStorageError::Database(e.to_string()))?;
            let mut rows = stmt.query(rusqlite::params![store_type, key_bytes])
                .map_err(|e| MlsStorageError::Database(e.to_string()))?;
            match rows.next().map_err(|e| MlsStorageError::Database(e.to_string()))? {
                Some(row) => {
                    let blob: Vec<u8> = row.get(0).map_err(|e| MlsStorageError::Database(e.to_string()))?;
                    Ok(Some(Self::deserialize_entity(&blob)?))
                }
                None => Ok(None),
            }
        })
    }

    fn delete_value(&self, store_type: &str, key: &impl Serialize) -> Result<(), MlsStorageError> {
        let key_bytes = Self::serialize_key(key)?;
        self.with_conn(|conn| {
            conn.execute(
                "DELETE FROM haex_mls_values_no_sync WHERE store_type = ?1 AND key_bytes = ?2",
                rusqlite::params![store_type, key_bytes],
            ).map_err(|e| MlsStorageError::Database(e.to_string()))?;
            Ok(())
        })
    }

    fn append_to_list(&self, store_type: &str, key: &impl Serialize, value: &impl Serialize) -> Result<(), MlsStorageError> {
        let key_bytes = Self::serialize_key(key)?;
        let value_blob = Self::serialize_entity(value)?;
        self.with_conn(|conn| {
            let next_idx: i64 = conn.query_row(
                "SELECT COALESCE(MAX(index_num), -1) + 1 FROM haex_mls_list_no_sync WHERE store_type = ?1 AND key_bytes = ?2",
                rusqlite::params![store_type, key_bytes],
                |row| row.get(0),
            ).map_err(|e| MlsStorageError::Database(e.to_string()))?;
            conn.execute(
                "INSERT INTO haex_mls_list_no_sync (store_type, key_bytes, index_num, value_blob) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![store_type, key_bytes, next_idx, value_blob],
            ).map_err(|e| MlsStorageError::Database(e.to_string()))?;
            Ok(())
        })
    }

    fn read_list<T: DeserializeOwned>(&self, store_type: &str, key: &impl Serialize) -> Result<Vec<T>, MlsStorageError> {
        let key_bytes = Self::serialize_key(key)?;
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT value_blob FROM haex_mls_list_no_sync WHERE store_type = ?1 AND key_bytes = ?2 ORDER BY index_num"
            ).map_err(|e| MlsStorageError::Database(e.to_string()))?;
            let rows = stmt.query_map(rusqlite::params![store_type, key_bytes], |row| {
                let blob: Vec<u8> = row.get(0)?;
                Ok(blob)
            }).map_err(|e| MlsStorageError::Database(e.to_string()))?;
            let mut result = Vec::new();
            for row in rows {
                let blob = row.map_err(|e| MlsStorageError::Database(e.to_string()))?;
                result.push(Self::deserialize_entity(&blob)?);
            }
            Ok(result)
        })
    }

    fn delete_list(&self, store_type: &str, key: &impl Serialize) -> Result<(), MlsStorageError> {
        let key_bytes = Self::serialize_key(key)?;
        self.with_conn(|conn| {
            conn.execute(
                "DELETE FROM haex_mls_list_no_sync WHERE store_type = ?1 AND key_bytes = ?2",
                rusqlite::params![store_type, key_bytes],
            ).map_err(|e| MlsStorageError::Database(e.to_string()))?;
            Ok(())
        })
    }

    fn remove_from_list(&self, store_type: &str, key: &impl Serialize, item: &impl Serialize) -> Result<(), MlsStorageError> {
        let key_bytes = Self::serialize_key(key)?;
        let item_blob = Self::serialize_entity(item)?;
        self.with_conn(|conn| {
            conn.execute(
                "DELETE FROM haex_mls_list_no_sync WHERE store_type = ?1 AND key_bytes = ?2 AND value_blob = ?3",
                rusqlite::params![store_type, key_bytes, item_blob],
            ).map_err(|e| MlsStorageError::Database(e.to_string()))?;
            Ok(())
        })
    }

    /// Tables are created by the Drizzle-generated migration (src-tauri/database/migrations/).
    /// Only the sync keys table is created here (not part of OpenMLS schema).
    pub fn init_tables(&self) -> Result<(), MlsStorageError> {
        self.with_conn(|conn| {
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS haex_mls_sync_keys_no_sync (
                    space_id TEXT NOT NULL,
                    epoch    INTEGER NOT NULL,
                    key_blob BLOB NOT NULL,
                    PRIMARY KEY (space_id, epoch)
                )"
            ).map_err(|e| MlsStorageError::Database(e.to_string()))?;
            Ok(())
        })
    }

    /// Store a derived sync encryption key for a specific space + epoch.
    pub fn store_sync_key(&self, space_id: &str, epoch: u64, key: &[u8]) -> Result<(), MlsStorageError> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT OR REPLACE INTO haex_mls_sync_keys_no_sync (space_id, epoch, key_blob) VALUES (?1, ?2, ?3)",
                rusqlite::params![space_id, epoch as i64, key],
            ).map_err(|e| MlsStorageError::Database(e.to_string()))?;
            Ok(())
        })
    }

    /// Load a sync encryption key for a specific space + epoch.
    pub fn load_sync_key(&self, space_id: &str, epoch: u64) -> Result<Option<Vec<u8>>, MlsStorageError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT key_blob FROM haex_mls_sync_keys_no_sync WHERE space_id = ?1 AND epoch = ?2"
            ).map_err(|e| MlsStorageError::Database(e.to_string()))?;
            let mut rows = stmt.query(rusqlite::params![space_id, epoch as i64])
                .map_err(|e| MlsStorageError::Database(e.to_string()))?;
            match rows.next().map_err(|e| MlsStorageError::Database(e.to_string()))? {
                Some(row) => {
                    let blob: Vec<u8> = row.get(0).map_err(|e| MlsStorageError::Database(e.to_string()))?;
                    Ok(Some(blob))
                }
                None => Ok(None),
            }
        })
    }

    pub fn store_own_identity_key(&self, public_key: &[u8]) -> Result<(), MlsStorageError> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT OR REPLACE INTO haex_mls_values_no_sync (store_type, key_bytes, value_blob) VALUES ('_identity', X'00', ?1)",
                rusqlite::params![public_key],
            ).map_err(|e| MlsStorageError::Database(e.to_string()))?;
            Ok(())
        })
    }

    pub fn load_own_identity_key(&self) -> Result<Option<Vec<u8>>, MlsStorageError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT value_blob FROM haex_mls_values_no_sync WHERE store_type = '_identity' AND key_bytes = X'00'"
            ).map_err(|e| MlsStorageError::Database(e.to_string()))?;
            let mut rows = stmt.query([])
                .map_err(|e| MlsStorageError::Database(e.to_string()))?;
            match rows.next().map_err(|e| MlsStorageError::Database(e.to_string()))? {
                Some(row) => {
                    let blob: Vec<u8> = row.get(0).map_err(|e| MlsStorageError::Database(e.to_string()))?;
                    Ok(Some(blob))
                }
                None => Ok(None),
            }
        })
    }
}

// Store type constants
const ST_JOIN_CONFIG: &str = "join_config";
const ST_OWN_LEAF_NODES: &str = "own_leaf_nodes";
const ST_PROPOSALS: &str = "proposals";
const ST_PROPOSAL_REFS: &str = "proposal_refs";
const ST_TREE: &str = "tree";
const ST_INTERIM_TRANSCRIPT_HASH: &str = "interim_transcript_hash";
const ST_CONTEXT: &str = "context";
const ST_CONFIRMATION_TAG: &str = "confirmation_tag";
const ST_GROUP_STATE: &str = "group_state";
const ST_MESSAGE_SECRETS: &str = "message_secrets";
const ST_RESUMPTION_PSK_STORE: &str = "resumption_psk_store";
const ST_OWN_LEAF_INDEX: &str = "own_leaf_index";
const ST_GROUP_EPOCH_SECRETS: &str = "group_epoch_secrets";
const ST_SIGNATURE_KEY_PAIR: &str = "signature_key_pair";
const ST_ENCRYPTION_KEY_PAIR: &str = "encryption_key_pair";
const ST_KEY_PACKAGE: &str = "key_package";
const ST_PSK: &str = "psk";

impl StorageProvider<CURRENT_VERSION> for SqlCipherMlsStorage {
    type Error = MlsStorageError;

    fn write_mls_join_config<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        MlsGroupJoinConfig: traits::MlsGroupJoinConfig<CURRENT_VERSION>,
    >(&self, group_id: &GroupId, config: &MlsGroupJoinConfig) -> Result<(), Self::Error> {
        self.write_value(ST_JOIN_CONFIG, group_id, config)
    }

    fn append_own_leaf_node<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        LeafNode: traits::LeafNode<CURRENT_VERSION>,
    >(&self, group_id: &GroupId, leaf_node: &LeafNode) -> Result<(), Self::Error> {
        self.append_to_list(ST_OWN_LEAF_NODES, group_id, leaf_node)
    }

    fn queue_proposal<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ProposalRef: traits::ProposalRef<CURRENT_VERSION>,
        QueuedProposal: traits::QueuedProposal<CURRENT_VERSION>,
    >(&self, group_id: &GroupId, proposal_ref: &ProposalRef, proposal: &QueuedProposal) -> Result<(), Self::Error> {
        // Store proposal as key-value (group_id+proposal_ref -> proposal)
        let compound_key = (Self::serialize_key(group_id)?, Self::serialize_key(proposal_ref)?);
        self.write_value(ST_PROPOSALS, &compound_key, proposal)?;
        // Also track the proposal ref in the list for queued_proposal_refs
        self.append_to_list(ST_PROPOSAL_REFS, group_id, proposal_ref)
    }

    fn write_tree<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        TreeSync: traits::TreeSync<CURRENT_VERSION>,
    >(&self, group_id: &GroupId, tree: &TreeSync) -> Result<(), Self::Error> {
        self.write_value(ST_TREE, group_id, tree)
    }

    fn write_interim_transcript_hash<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        InterimTranscriptHash: traits::InterimTranscriptHash<CURRENT_VERSION>,
    >(&self, group_id: &GroupId, interim_transcript_hash: &InterimTranscriptHash) -> Result<(), Self::Error> {
        self.write_value(ST_INTERIM_TRANSCRIPT_HASH, group_id, interim_transcript_hash)
    }

    fn write_context<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        GroupContext: traits::GroupContext<CURRENT_VERSION>,
    >(&self, group_id: &GroupId, group_context: &GroupContext) -> Result<(), Self::Error> {
        self.write_value(ST_CONTEXT, group_id, group_context)
    }

    fn write_confirmation_tag<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ConfirmationTag: traits::ConfirmationTag<CURRENT_VERSION>,
    >(&self, group_id: &GroupId, confirmation_tag: &ConfirmationTag) -> Result<(), Self::Error> {
        self.write_value(ST_CONFIRMATION_TAG, group_id, confirmation_tag)
    }

    fn write_group_state<
        GroupState: traits::GroupState<CURRENT_VERSION>,
        GroupId: traits::GroupId<CURRENT_VERSION>,
    >(&self, group_id: &GroupId, group_state: &GroupState) -> Result<(), Self::Error> {
        self.write_value(ST_GROUP_STATE, group_id, group_state)
    }

    fn write_message_secrets<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        MessageSecrets: traits::MessageSecrets<CURRENT_VERSION>,
    >(&self, group_id: &GroupId, message_secrets: &MessageSecrets) -> Result<(), Self::Error> {
        self.write_value(ST_MESSAGE_SECRETS, group_id, message_secrets)
    }

    fn write_resumption_psk_store<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ResumptionPskStore: traits::ResumptionPskStore<CURRENT_VERSION>,
    >(&self, group_id: &GroupId, resumption_psk_store: &ResumptionPskStore) -> Result<(), Self::Error> {
        self.write_value(ST_RESUMPTION_PSK_STORE, group_id, resumption_psk_store)
    }

    fn write_own_leaf_index<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        LeafNodeIndex: traits::LeafNodeIndex<CURRENT_VERSION>,
    >(&self, group_id: &GroupId, own_leaf_index: &LeafNodeIndex) -> Result<(), Self::Error> {
        self.write_value(ST_OWN_LEAF_INDEX, group_id, own_leaf_index)
    }

    fn write_group_epoch_secrets<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        GroupEpochSecrets: traits::GroupEpochSecrets<CURRENT_VERSION>,
    >(&self, group_id: &GroupId, group_epoch_secrets: &GroupEpochSecrets) -> Result<(), Self::Error> {
        self.write_value(ST_GROUP_EPOCH_SECRETS, group_id, group_epoch_secrets)
    }

    fn write_signature_key_pair<
        SignaturePublicKey: traits::SignaturePublicKey<CURRENT_VERSION>,
        SignatureKeyPair: traits::SignatureKeyPair<CURRENT_VERSION>,
    >(&self, public_key: &SignaturePublicKey, signature_key_pair: &SignatureKeyPair) -> Result<(), Self::Error> {
        self.write_value(ST_SIGNATURE_KEY_PAIR, public_key, signature_key_pair)
    }

    fn write_encryption_key_pair<
        EncryptionKey: traits::EncryptionKey<CURRENT_VERSION>,
        HpkeKeyPair: traits::HpkeKeyPair<CURRENT_VERSION>,
    >(&self, public_key: &EncryptionKey, key_pair: &HpkeKeyPair) -> Result<(), Self::Error> {
        self.write_value(ST_ENCRYPTION_KEY_PAIR, public_key, key_pair)
    }

    fn write_encryption_epoch_key_pairs<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        EpochKey: traits::EpochKey<CURRENT_VERSION>,
        HpkeKeyPair: traits::HpkeKeyPair<CURRENT_VERSION>,
    >(&self, group_id: &GroupId, epoch: &EpochKey, leaf_index: u32, key_pairs: &[HpkeKeyPair]) -> Result<(), Self::Error> {
        let group_id_bytes = Self::serialize_key(group_id)?;
        let epoch_bytes = Self::serialize_key(epoch)?;
        self.with_conn(|conn| {
            // Delete existing pairs for this combination
            conn.execute(
                "DELETE FROM haex_mls_epoch_key_pairs_no_sync WHERE group_id = ?1 AND epoch_bytes = ?2 AND leaf_index = ?3",
                rusqlite::params![group_id_bytes, epoch_bytes, leaf_index],
            ).map_err(|e| MlsStorageError::Database(e.to_string()))?;
            // Insert new pairs serialized together
            let value_blob = Self::serialize_entity(&key_pairs)?;
            conn.execute(
                "INSERT INTO haex_mls_epoch_key_pairs_no_sync (group_id, epoch_bytes, leaf_index, value_blob) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![group_id_bytes, epoch_bytes, leaf_index, value_blob],
            ).map_err(|e| MlsStorageError::Database(e.to_string()))?;
            Ok(())
        })
    }

    fn write_key_package<
        HashReference: traits::HashReference<CURRENT_VERSION>,
        KeyPackage: traits::KeyPackage<CURRENT_VERSION>,
    >(&self, hash_ref: &HashReference, key_package: &KeyPackage) -> Result<(), Self::Error> {
        self.write_value(ST_KEY_PACKAGE, hash_ref, key_package)
    }

    fn write_psk<
        PskId: traits::PskId<CURRENT_VERSION>,
        PskBundle: traits::PskBundle<CURRENT_VERSION>,
    >(&self, psk_id: &PskId, psk: &PskBundle) -> Result<(), Self::Error> {
        self.write_value(ST_PSK, psk_id, psk)
    }

    // --- Read operations ---

    fn mls_group_join_config<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        MlsGroupJoinConfig: traits::MlsGroupJoinConfig<CURRENT_VERSION>,
    >(&self, group_id: &GroupId) -> Result<Option<MlsGroupJoinConfig>, Self::Error> {
        self.read_value(ST_JOIN_CONFIG, group_id)
    }

    fn own_leaf_nodes<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        LeafNode: traits::LeafNode<CURRENT_VERSION>,
    >(&self, group_id: &GroupId) -> Result<Vec<LeafNode>, Self::Error> {
        self.read_list(ST_OWN_LEAF_NODES, group_id)
    }

    fn queued_proposal_refs<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ProposalRef: traits::ProposalRef<CURRENT_VERSION>,
    >(&self, group_id: &GroupId) -> Result<Vec<ProposalRef>, Self::Error> {
        self.read_list(ST_PROPOSAL_REFS, group_id)
    }

    fn queued_proposals<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ProposalRef: traits::ProposalRef<CURRENT_VERSION>,
        QueuedProposal: traits::QueuedProposal<CURRENT_VERSION>,
    >(&self, group_id: &GroupId) -> Result<Vec<(ProposalRef, QueuedProposal)>, Self::Error> {
        let refs: Vec<ProposalRef> = self.read_list(ST_PROPOSAL_REFS, group_id)?;
        let mut result = Vec::new();
        for proposal_ref in refs {
            let compound_key = (Self::serialize_key(group_id)?, Self::serialize_key(&proposal_ref)?);
            if let Some(proposal) = self.read_value::<QueuedProposal>(ST_PROPOSALS, &compound_key)? {
                result.push((proposal_ref, proposal));
            }
        }
        Ok(result)
    }

    fn tree<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        TreeSync: traits::TreeSync<CURRENT_VERSION>,
    >(&self, group_id: &GroupId) -> Result<Option<TreeSync>, Self::Error> {
        self.read_value(ST_TREE, group_id)
    }

    fn group_context<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        GroupContext: traits::GroupContext<CURRENT_VERSION>,
    >(&self, group_id: &GroupId) -> Result<Option<GroupContext>, Self::Error> {
        self.read_value(ST_CONTEXT, group_id)
    }

    fn interim_transcript_hash<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        InterimTranscriptHash: traits::InterimTranscriptHash<CURRENT_VERSION>,
    >(&self, group_id: &GroupId) -> Result<Option<InterimTranscriptHash>, Self::Error> {
        self.read_value(ST_INTERIM_TRANSCRIPT_HASH, group_id)
    }

    fn confirmation_tag<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ConfirmationTag: traits::ConfirmationTag<CURRENT_VERSION>,
    >(&self, group_id: &GroupId) -> Result<Option<ConfirmationTag>, Self::Error> {
        self.read_value(ST_CONFIRMATION_TAG, group_id)
    }

    fn group_state<
        GroupState: traits::GroupState<CURRENT_VERSION>,
        GroupId: traits::GroupId<CURRENT_VERSION>,
    >(&self, group_id: &GroupId) -> Result<Option<GroupState>, Self::Error> {
        self.read_value(ST_GROUP_STATE, group_id)
    }

    fn message_secrets<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        MessageSecrets: traits::MessageSecrets<CURRENT_VERSION>,
    >(&self, group_id: &GroupId) -> Result<Option<MessageSecrets>, Self::Error> {
        self.read_value(ST_MESSAGE_SECRETS, group_id)
    }

    fn resumption_psk_store<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ResumptionPskStore: traits::ResumptionPskStore<CURRENT_VERSION>,
    >(&self, group_id: &GroupId) -> Result<Option<ResumptionPskStore>, Self::Error> {
        self.read_value(ST_RESUMPTION_PSK_STORE, group_id)
    }

    fn own_leaf_index<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        LeafNodeIndex: traits::LeafNodeIndex<CURRENT_VERSION>,
    >(&self, group_id: &GroupId) -> Result<Option<LeafNodeIndex>, Self::Error> {
        self.read_value(ST_OWN_LEAF_INDEX, group_id)
    }

    fn group_epoch_secrets<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        GroupEpochSecrets: traits::GroupEpochSecrets<CURRENT_VERSION>,
    >(&self, group_id: &GroupId) -> Result<Option<GroupEpochSecrets>, Self::Error> {
        self.read_value(ST_GROUP_EPOCH_SECRETS, group_id)
    }

    fn signature_key_pair<
        SignaturePublicKey: traits::SignaturePublicKey<CURRENT_VERSION>,
        SignatureKeyPair: traits::SignatureKeyPair<CURRENT_VERSION>,
    >(&self, public_key: &SignaturePublicKey) -> Result<Option<SignatureKeyPair>, Self::Error> {
        self.read_value(ST_SIGNATURE_KEY_PAIR, public_key)
    }

    fn encryption_key_pair<
        HpkeKeyPair: traits::HpkeKeyPair<CURRENT_VERSION>,
        EncryptionKey: traits::EncryptionKey<CURRENT_VERSION>,
    >(&self, public_key: &EncryptionKey) -> Result<Option<HpkeKeyPair>, Self::Error> {
        self.read_value(ST_ENCRYPTION_KEY_PAIR, public_key)
    }

    fn encryption_epoch_key_pairs<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        EpochKey: traits::EpochKey<CURRENT_VERSION>,
        HpkeKeyPair: traits::HpkeKeyPair<CURRENT_VERSION>,
    >(&self, group_id: &GroupId, epoch: &EpochKey, leaf_index: u32) -> Result<Vec<HpkeKeyPair>, Self::Error> {
        let group_id_bytes = Self::serialize_key(group_id)?;
        let epoch_bytes = Self::serialize_key(epoch)?;
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT value_blob FROM haex_mls_epoch_key_pairs_no_sync WHERE group_id = ?1 AND epoch_bytes = ?2 AND leaf_index = ?3"
            ).map_err(|e| MlsStorageError::Database(e.to_string()))?;
            let mut rows = stmt.query(rusqlite::params![group_id_bytes, epoch_bytes, leaf_index])
                .map_err(|e| MlsStorageError::Database(e.to_string()))?;
            match rows.next().map_err(|e| MlsStorageError::Database(e.to_string()))? {
                Some(row) => {
                    let blob: Vec<u8> = row.get(0).map_err(|e| MlsStorageError::Database(e.to_string()))?;
                    Self::deserialize_entity(&blob)
                }
                None => Ok(Vec::new()),
            }
        })
    }

    fn key_package<
        KeyPackageRef: traits::HashReference<CURRENT_VERSION>,
        KeyPackage: traits::KeyPackage<CURRENT_VERSION>,
    >(&self, hash_ref: &KeyPackageRef) -> Result<Option<KeyPackage>, Self::Error> {
        self.read_value(ST_KEY_PACKAGE, hash_ref)
    }

    fn psk<
        PskBundle: traits::PskBundle<CURRENT_VERSION>,
        PskId: traits::PskId<CURRENT_VERSION>,
    >(&self, psk_id: &PskId) -> Result<Option<PskBundle>, Self::Error> {
        self.read_value(ST_PSK, psk_id)
    }

    // --- Delete operations ---

    fn remove_proposal<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ProposalRef: traits::ProposalRef<CURRENT_VERSION>,
    >(&self, group_id: &GroupId, proposal_ref: &ProposalRef) -> Result<(), Self::Error> {
        let compound_key = (Self::serialize_key(group_id)?, Self::serialize_key(proposal_ref)?);
        self.delete_value(ST_PROPOSALS, &compound_key)?;
        self.remove_from_list(ST_PROPOSAL_REFS, group_id, proposal_ref)
    }

    fn delete_own_leaf_nodes<
        GroupId: traits::GroupId<CURRENT_VERSION>,
    >(&self, group_id: &GroupId) -> Result<(), Self::Error> {
        self.delete_list(ST_OWN_LEAF_NODES, group_id)
    }

    fn delete_group_config<
        GroupId: traits::GroupId<CURRENT_VERSION>,
    >(&self, group_id: &GroupId) -> Result<(), Self::Error> {
        self.delete_value(ST_JOIN_CONFIG, group_id)
    }

    fn delete_tree<
        GroupId: traits::GroupId<CURRENT_VERSION>,
    >(&self, group_id: &GroupId) -> Result<(), Self::Error> {
        self.delete_value(ST_TREE, group_id)
    }

    fn delete_confirmation_tag<
        GroupId: traits::GroupId<CURRENT_VERSION>,
    >(&self, group_id: &GroupId) -> Result<(), Self::Error> {
        self.delete_value(ST_CONFIRMATION_TAG, group_id)
    }

    fn delete_group_state<
        GroupId: traits::GroupId<CURRENT_VERSION>,
    >(&self, group_id: &GroupId) -> Result<(), Self::Error> {
        self.delete_value(ST_GROUP_STATE, group_id)
    }

    fn delete_context<
        GroupId: traits::GroupId<CURRENT_VERSION>,
    >(&self, group_id: &GroupId) -> Result<(), Self::Error> {
        self.delete_value(ST_CONTEXT, group_id)
    }

    fn delete_interim_transcript_hash<
        GroupId: traits::GroupId<CURRENT_VERSION>,
    >(&self, group_id: &GroupId) -> Result<(), Self::Error> {
        self.delete_value(ST_INTERIM_TRANSCRIPT_HASH, group_id)
    }

    fn delete_message_secrets<
        GroupId: traits::GroupId<CURRENT_VERSION>,
    >(&self, group_id: &GroupId) -> Result<(), Self::Error> {
        self.delete_value(ST_MESSAGE_SECRETS, group_id)
    }

    fn delete_all_resumption_psk_secrets<
        GroupId: traits::GroupId<CURRENT_VERSION>,
    >(&self, group_id: &GroupId) -> Result<(), Self::Error> {
        self.delete_value(ST_RESUMPTION_PSK_STORE, group_id)
    }

    fn delete_own_leaf_index<
        GroupId: traits::GroupId<CURRENT_VERSION>,
    >(&self, group_id: &GroupId) -> Result<(), Self::Error> {
        self.delete_value(ST_OWN_LEAF_INDEX, group_id)
    }

    fn delete_group_epoch_secrets<
        GroupId: traits::GroupId<CURRENT_VERSION>,
    >(&self, group_id: &GroupId) -> Result<(), Self::Error> {
        self.delete_value(ST_GROUP_EPOCH_SECRETS, group_id)
    }

    fn clear_proposal_queue<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ProposalRef: traits::ProposalRef<CURRENT_VERSION>,
    >(&self, group_id: &GroupId) -> Result<(), Self::Error> {
        // First get all proposal refs to delete the proposals themselves
        let refs: Vec<ProposalRef> = self.read_list(ST_PROPOSAL_REFS, group_id)?;
        for proposal_ref in &refs {
            let compound_key = (Self::serialize_key(group_id)?, Self::serialize_key(proposal_ref)?);
            self.delete_value(ST_PROPOSALS, &compound_key)?;
        }
        self.delete_list(ST_PROPOSAL_REFS, group_id)
    }

    fn delete_signature_key_pair<
        SignaturePublicKey: traits::SignaturePublicKey<CURRENT_VERSION>,
    >(&self, public_key: &SignaturePublicKey) -> Result<(), Self::Error> {
        self.delete_value(ST_SIGNATURE_KEY_PAIR, public_key)
    }

    fn delete_encryption_key_pair<
        EncryptionKey: traits::EncryptionKey<CURRENT_VERSION>,
    >(&self, public_key: &EncryptionKey) -> Result<(), Self::Error> {
        self.delete_value(ST_ENCRYPTION_KEY_PAIR, public_key)
    }

    fn delete_encryption_epoch_key_pairs<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        EpochKey: traits::EpochKey<CURRENT_VERSION>,
    >(&self, group_id: &GroupId, epoch: &EpochKey, leaf_index: u32) -> Result<(), Self::Error> {
        let group_id_bytes = Self::serialize_key(group_id)?;
        let epoch_bytes = Self::serialize_key(epoch)?;
        self.with_conn(|conn| {
            conn.execute(
                "DELETE FROM haex_mls_epoch_key_pairs_no_sync WHERE group_id = ?1 AND epoch_bytes = ?2 AND leaf_index = ?3",
                rusqlite::params![group_id_bytes, epoch_bytes, leaf_index],
            ).map_err(|e| MlsStorageError::Database(e.to_string()))?;
            Ok(())
        })
    }

    fn delete_key_package<
        KeyPackageRef: traits::HashReference<CURRENT_VERSION>,
    >(&self, hash_ref: &KeyPackageRef) -> Result<(), Self::Error> {
        self.delete_value(ST_KEY_PACKAGE, hash_ref)
    }

    fn delete_psk<
        PskKey: traits::PskId<CURRENT_VERSION>,
    >(&self, psk_id: &PskKey) -> Result<(), Self::Error> {
        self.delete_value(ST_PSK, psk_id)
    }
}
