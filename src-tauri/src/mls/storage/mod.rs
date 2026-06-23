use std::sync::{Arc, Mutex};

use rusqlite::Connection;
use serde::de::DeserializeOwned;
use serde::Serialize;

use super::queries::{
    SQL_DELETE_LIST, SQL_DELETE_LIST_ITEM, SQL_DELETE_VALUE, SQL_INSERT_LIST, SQL_NEXT_LIST_INDEX,
    SQL_SELECT_LIST, SQL_SELECT_OWN_DID, SQL_SELECT_OWN_IDENTITY_KEY, SQL_SELECT_VALUE,
    SQL_UPSERT_OWN_DID, SQL_UPSERT_OWN_IDENTITY_KEY, SQL_UPSERT_VALUE,
};

mod trait_impl;

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
    fn with_conn<T>(
        &self,
        f: impl FnOnce(&Connection) -> Result<T, MlsStorageError>,
    ) -> Result<T, MlsStorageError> {
        let guard = self
            .conn
            .lock()
            .map_err(|e| MlsStorageError::Database(e.to_string()))?;
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

    fn write_value(
        &self,
        store_type: &str,
        key: &impl Serialize,
        value: &impl Serialize,
    ) -> Result<(), MlsStorageError> {
        let key_bytes = Self::serialize_key(key)?;
        let value_blob = Self::serialize_entity(value)?;
        self.with_conn(|conn| {
            conn.execute(
                &SQL_UPSERT_VALUE,
                rusqlite::params![store_type, key_bytes, value_blob],
            )
            .map_err(|e| MlsStorageError::Database(e.to_string()))?;
            Ok(())
        })
    }

    fn read_value<T: DeserializeOwned>(
        &self,
        store_type: &str,
        key: &impl Serialize,
    ) -> Result<Option<T>, MlsStorageError> {
        let key_bytes = Self::serialize_key(key)?;
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare(&SQL_SELECT_VALUE)
                .map_err(|e| MlsStorageError::Database(e.to_string()))?;
            let mut rows = stmt
                .query(rusqlite::params![store_type, key_bytes])
                .map_err(|e| MlsStorageError::Database(e.to_string()))?;
            match rows
                .next()
                .map_err(|e| MlsStorageError::Database(e.to_string()))?
            {
                Some(row) => {
                    let blob: Vec<u8> = row
                        .get(0)
                        .map_err(|e| MlsStorageError::Database(e.to_string()))?;
                    Ok(Some(Self::deserialize_entity(&blob)?))
                }
                None => Ok(None),
            }
        })
    }

    fn delete_value(&self, store_type: &str, key: &impl Serialize) -> Result<(), MlsStorageError> {
        let key_bytes = Self::serialize_key(key)?;
        self.with_conn(|conn| {
            conn.execute(&SQL_DELETE_VALUE, rusqlite::params![store_type, key_bytes])
                .map_err(|e| MlsStorageError::Database(e.to_string()))?;
            Ok(())
        })
    }

    fn append_to_list(
        &self,
        store_type: &str,
        key: &impl Serialize,
        value: &impl Serialize,
    ) -> Result<(), MlsStorageError> {
        let key_bytes = Self::serialize_key(key)?;
        let value_blob = Self::serialize_entity(value)?;
        self.with_conn(|conn| {
            let next_idx: i64 = conn
                .query_row(
                    &SQL_NEXT_LIST_INDEX,
                    rusqlite::params![store_type, key_bytes],
                    |row| row.get(0),
                )
                .map_err(|e| MlsStorageError::Database(e.to_string()))?;
            conn.execute(
                &SQL_INSERT_LIST,
                rusqlite::params![store_type, key_bytes, next_idx, value_blob],
            )
            .map_err(|e| MlsStorageError::Database(e.to_string()))?;
            Ok(())
        })
    }

    fn read_list<T: DeserializeOwned>(
        &self,
        store_type: &str,
        key: &impl Serialize,
    ) -> Result<Vec<T>, MlsStorageError> {
        let key_bytes = Self::serialize_key(key)?;
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare(&SQL_SELECT_LIST)
                .map_err(|e| MlsStorageError::Database(e.to_string()))?;
            let rows = stmt
                .query_map(rusqlite::params![store_type, key_bytes], |row| {
                    let blob: Vec<u8> = row.get(0)?;
                    Ok(blob)
                })
                .map_err(|e| MlsStorageError::Database(e.to_string()))?;
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
            conn.execute(&SQL_DELETE_LIST, rusqlite::params![store_type, key_bytes])
                .map_err(|e| MlsStorageError::Database(e.to_string()))?;
            Ok(())
        })
    }

    fn remove_from_list(
        &self,
        store_type: &str,
        key: &impl Serialize,
        item: &impl Serialize,
    ) -> Result<(), MlsStorageError> {
        let key_bytes = Self::serialize_key(key)?;
        let item_blob = Self::serialize_entity(item)?;
        self.with_conn(|conn| {
            conn.execute(
                &SQL_DELETE_LIST_ITEM,
                rusqlite::params![store_type, key_bytes, item_blob],
            )
            .map_err(|e| MlsStorageError::Database(e.to_string()))?;
            Ok(())
        })
    }

    /// Tables are created by the Drizzle-generated migration (src-tauri/database/migrations/).
    /// This is a no-op kept for API compatibility.
    pub fn init_tables(&self) -> Result<(), MlsStorageError> {
        Ok(())
    }

    pub fn store_own_identity_key(&self, public_key: &[u8]) -> Result<(), MlsStorageError> {
        self.with_conn(|conn| {
            conn.execute(&SQL_UPSERT_OWN_IDENTITY_KEY, rusqlite::params![public_key])
                .map_err(|e| MlsStorageError::Database(e.to_string()))?;
            Ok(())
        })
    }

    pub fn load_own_identity_key(&self) -> Result<Option<Vec<u8>>, MlsStorageError> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare(&SQL_SELECT_OWN_IDENTITY_KEY)
                .map_err(|e| MlsStorageError::Database(e.to_string()))?;
            let mut rows = stmt
                .query([])
                .map_err(|e| MlsStorageError::Database(e.to_string()))?;
            match rows
                .next()
                .map_err(|e| MlsStorageError::Database(e.to_string()))?
            {
                Some(row) => {
                    let blob: Vec<u8> = row
                        .get(0)
                        .map_err(|e| MlsStorageError::Database(e.to_string()))?;
                    Ok(Some(blob))
                }
                None => Ok(None),
            }
        })
    }

    pub fn store_own_did(&self, did: &str) -> Result<(), MlsStorageError> {
        self.with_conn(|conn| {
            conn.execute(&SQL_UPSERT_OWN_DID, rusqlite::params![did.as_bytes()])
                .map_err(|e| MlsStorageError::Database(e.to_string()))?;
            Ok(())
        })
    }

    pub fn load_own_did(&self) -> Result<Option<String>, MlsStorageError> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare(&SQL_SELECT_OWN_DID)
                .map_err(|e| MlsStorageError::Database(e.to_string()))?;
            let mut rows = stmt
                .query([])
                .map_err(|e| MlsStorageError::Database(e.to_string()))?;
            match rows
                .next()
                .map_err(|e| MlsStorageError::Database(e.to_string()))?
            {
                Some(row) => {
                    let blob: Vec<u8> = row
                        .get(0)
                        .map_err(|e| MlsStorageError::Database(e.to_string()))?;
                    Ok(Some(String::from_utf8(blob).map_err(|e| {
                        MlsStorageError::Serialization(format!("Invalid DID UTF-8: {e}"))
                    })?))
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
