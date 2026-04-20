//! MLS Storage Queries
//!
//! Centralized SQL for the `SqlCipherMlsStorage` provider. Follows the
//! `lazy_static!` + `format!` pattern established in `remote_storage/queries.rs`.

use crate::table_names::{
    COL_MLS_EPOCH_KEY_PAIRS_NO_SYNC_EPOCH_BYTES, COL_MLS_EPOCH_KEY_PAIRS_NO_SYNC_GROUP_ID,
    COL_MLS_EPOCH_KEY_PAIRS_NO_SYNC_LEAF_INDEX, COL_MLS_EPOCH_KEY_PAIRS_NO_SYNC_VALUE_BLOB,
    COL_MLS_LIST_NO_SYNC_INDEX_NUM, COL_MLS_LIST_NO_SYNC_KEY_BYTES,
    COL_MLS_LIST_NO_SYNC_STORE_TYPE, COL_MLS_LIST_NO_SYNC_VALUE_BLOB,
    COL_MLS_VALUES_NO_SYNC_KEY_BYTES, COL_MLS_VALUES_NO_SYNC_STORE_TYPE,
    COL_MLS_VALUES_NO_SYNC_VALUE_BLOB, TABLE_MLS_EPOCH_KEY_PAIRS_NO_SYNC,
    TABLE_MLS_LIST_NO_SYNC, TABLE_MLS_VALUES_NO_SYNC,
};
use lazy_static::lazy_static;

lazy_static! {
    // Generic key-value storage (haex_mls_values_no_sync)

    pub static ref SQL_UPSERT_VALUE: String = format!(
        "INSERT OR REPLACE INTO {TABLE_MLS_VALUES_NO_SYNC} \
         ({COL_MLS_VALUES_NO_SYNC_STORE_TYPE}, {COL_MLS_VALUES_NO_SYNC_KEY_BYTES}, {COL_MLS_VALUES_NO_SYNC_VALUE_BLOB}) \
         VALUES (?1, ?2, ?3)"
    );

    pub static ref SQL_SELECT_VALUE: String = format!(
        "SELECT {COL_MLS_VALUES_NO_SYNC_VALUE_BLOB} FROM {TABLE_MLS_VALUES_NO_SYNC} \
         WHERE {COL_MLS_VALUES_NO_SYNC_STORE_TYPE} = ?1 AND {COL_MLS_VALUES_NO_SYNC_KEY_BYTES} = ?2"
    );

    pub static ref SQL_DELETE_VALUE: String = format!(
        "DELETE FROM {TABLE_MLS_VALUES_NO_SYNC} \
         WHERE {COL_MLS_VALUES_NO_SYNC_STORE_TYPE} = ?1 AND {COL_MLS_VALUES_NO_SYNC_KEY_BYTES} = ?2"
    );

    // Ordered list storage (haex_mls_list_no_sync)

    pub static ref SQL_NEXT_LIST_INDEX: String = format!(
        "SELECT COALESCE(MAX({COL_MLS_LIST_NO_SYNC_INDEX_NUM}), -1) + 1 FROM {TABLE_MLS_LIST_NO_SYNC} \
         WHERE {COL_MLS_LIST_NO_SYNC_STORE_TYPE} = ?1 AND {COL_MLS_LIST_NO_SYNC_KEY_BYTES} = ?2"
    );

    pub static ref SQL_INSERT_LIST: String = format!(
        "INSERT INTO {TABLE_MLS_LIST_NO_SYNC} \
         ({COL_MLS_LIST_NO_SYNC_STORE_TYPE}, {COL_MLS_LIST_NO_SYNC_KEY_BYTES}, {COL_MLS_LIST_NO_SYNC_INDEX_NUM}, {COL_MLS_LIST_NO_SYNC_VALUE_BLOB}) \
         VALUES (?1, ?2, ?3, ?4)"
    );

    pub static ref SQL_SELECT_LIST: String = format!(
        "SELECT {COL_MLS_LIST_NO_SYNC_VALUE_BLOB} FROM {TABLE_MLS_LIST_NO_SYNC} \
         WHERE {COL_MLS_LIST_NO_SYNC_STORE_TYPE} = ?1 AND {COL_MLS_LIST_NO_SYNC_KEY_BYTES} = ?2 \
         ORDER BY {COL_MLS_LIST_NO_SYNC_INDEX_NUM}"
    );

    pub static ref SQL_DELETE_LIST: String = format!(
        "DELETE FROM {TABLE_MLS_LIST_NO_SYNC} \
         WHERE {COL_MLS_LIST_NO_SYNC_STORE_TYPE} = ?1 AND {COL_MLS_LIST_NO_SYNC_KEY_BYTES} = ?2"
    );

    pub static ref SQL_DELETE_LIST_ITEM: String = format!(
        "DELETE FROM {TABLE_MLS_LIST_NO_SYNC} \
         WHERE {COL_MLS_LIST_NO_SYNC_STORE_TYPE} = ?1 AND {COL_MLS_LIST_NO_SYNC_KEY_BYTES} = ?2 AND {COL_MLS_LIST_NO_SYNC_VALUE_BLOB} = ?3"
    );

    // Specialized identity/DID slots — store_type='_identity' / '_own_did', key_bytes=X'00'.
    // These use the same kv table but with hard-coded store_type and key so callers
    // don't have to serialize a dummy key.

    pub static ref SQL_UPSERT_OWN_IDENTITY_KEY: String = format!(
        "INSERT OR REPLACE INTO {TABLE_MLS_VALUES_NO_SYNC} \
         ({COL_MLS_VALUES_NO_SYNC_STORE_TYPE}, {COL_MLS_VALUES_NO_SYNC_KEY_BYTES}, {COL_MLS_VALUES_NO_SYNC_VALUE_BLOB}) \
         VALUES ('_identity', X'00', ?1)"
    );

    pub static ref SQL_SELECT_OWN_IDENTITY_KEY: String = format!(
        "SELECT {COL_MLS_VALUES_NO_SYNC_VALUE_BLOB} FROM {TABLE_MLS_VALUES_NO_SYNC} \
         WHERE {COL_MLS_VALUES_NO_SYNC_STORE_TYPE} = '_identity' AND {COL_MLS_VALUES_NO_SYNC_KEY_BYTES} = X'00'"
    );

    pub static ref SQL_UPSERT_OWN_DID: String = format!(
        "INSERT OR REPLACE INTO {TABLE_MLS_VALUES_NO_SYNC} \
         ({COL_MLS_VALUES_NO_SYNC_STORE_TYPE}, {COL_MLS_VALUES_NO_SYNC_KEY_BYTES}, {COL_MLS_VALUES_NO_SYNC_VALUE_BLOB}) \
         VALUES ('_own_did', X'00', ?1)"
    );

    pub static ref SQL_SELECT_OWN_DID: String = format!(
        "SELECT {COL_MLS_VALUES_NO_SYNC_VALUE_BLOB} FROM {TABLE_MLS_VALUES_NO_SYNC} \
         WHERE {COL_MLS_VALUES_NO_SYNC_STORE_TYPE} = '_own_did' AND {COL_MLS_VALUES_NO_SYNC_KEY_BYTES} = X'00'"
    );

    // Epoch key pairs (haex_mls_epoch_key_pairs_no_sync) — keyed by (group_id, epoch, leaf_index)

    pub static ref SQL_DELETE_EPOCH_KEY_PAIR: String = format!(
        "DELETE FROM {TABLE_MLS_EPOCH_KEY_PAIRS_NO_SYNC} \
         WHERE {COL_MLS_EPOCH_KEY_PAIRS_NO_SYNC_GROUP_ID} = ?1 AND {COL_MLS_EPOCH_KEY_PAIRS_NO_SYNC_EPOCH_BYTES} = ?2 AND {COL_MLS_EPOCH_KEY_PAIRS_NO_SYNC_LEAF_INDEX} = ?3"
    );

    pub static ref SQL_INSERT_EPOCH_KEY_PAIR: String = format!(
        "INSERT INTO {TABLE_MLS_EPOCH_KEY_PAIRS_NO_SYNC} \
         ({COL_MLS_EPOCH_KEY_PAIRS_NO_SYNC_GROUP_ID}, {COL_MLS_EPOCH_KEY_PAIRS_NO_SYNC_EPOCH_BYTES}, {COL_MLS_EPOCH_KEY_PAIRS_NO_SYNC_LEAF_INDEX}, {COL_MLS_EPOCH_KEY_PAIRS_NO_SYNC_VALUE_BLOB}) \
         VALUES (?1, ?2, ?3, ?4)"
    );

    pub static ref SQL_SELECT_EPOCH_KEY_PAIR: String = format!(
        "SELECT {COL_MLS_EPOCH_KEY_PAIRS_NO_SYNC_VALUE_BLOB} FROM {TABLE_MLS_EPOCH_KEY_PAIRS_NO_SYNC} \
         WHERE {COL_MLS_EPOCH_KEY_PAIRS_NO_SYNC_GROUP_ID} = ?1 AND {COL_MLS_EPOCH_KEY_PAIRS_NO_SYNC_EPOCH_BYTES} = ?2 AND {COL_MLS_EPOCH_KEY_PAIRS_NO_SYNC_LEAF_INDEX} = ?3"
    );
}
