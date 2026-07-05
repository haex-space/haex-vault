// src-tauri/src/storage/queries.rs
//!
//! Storage Backend Database Queries
//!

use crate::table_names::{
    COL_S3_BACKENDS_CONFIG, COL_S3_BACKENDS_CREATED_AT, COL_S3_BACKENDS_ENABLED,
    COL_S3_BACKENDS_ID, COL_S3_BACKENDS_NAME, COL_S3_BACKENDS_ORIGIN_TYPE,
    COL_S3_BACKENDS_SHARE_ACCESS_FLAGS, COL_S3_BACKENDS_TYPE, COL_SHARED_SPACE_SYNC_ROW_PKS,
    COL_SHARED_SPACE_SYNC_SPACE_ID, COL_SHARED_SPACE_SYNC_TABLE_NAME, COL_SPACES_ID,
    COL_SPACES_NAME, TABLE_S3_BACKENDS, TABLE_SHARED_SPACE_SYNC, TABLE_SPACES,
};
use lazy_static::lazy_static;

lazy_static! {
    /// List all storage backends. The LEFT JOIN against
    /// `haex_shared_space_sync` + `haex_spaces` decorates each row with its
    /// share provenance in one round-trip, so the frontend can render the
    /// "aus <space>" chip without a follow-up lookup. Owned rows produce
    /// NULL columns for the joined side.
    ///
    /// The mapping rows written by `share_command::persist_shared_backend`
    /// carry `row_pks = '["<backend_id>"]'` (a single-element JSON array) —
    /// `json_extract(..., '$[0]')` is the cheap lookup key. If a shared
    /// backend appears in multiple mappings (schema currently permits it
    /// though the share command never writes more than one), the JOIN picks
    /// an arbitrary row; multi-space UX is a v1 limitation.
    pub static ref SQL_LIST_BACKENDS: String = format!(
        "SELECT s3.{COL_S3_BACKENDS_ID}, s3.{COL_S3_BACKENDS_TYPE}, s3.{COL_S3_BACKENDS_NAME}, \
         s3.{COL_S3_BACKENDS_ENABLED}, s3.{COL_S3_BACKENDS_CREATED_AT}, s3.{COL_S3_BACKENDS_CONFIG}, \
         s3.{COL_S3_BACKENDS_ORIGIN_TYPE}, s3.{COL_S3_BACKENDS_SHARE_ACCESS_FLAGS}, \
         mapping.{COL_SHARED_SPACE_SYNC_SPACE_ID}, spaces.{COL_SPACES_NAME} \
         FROM {TABLE_S3_BACKENDS} AS s3 \
         LEFT JOIN {TABLE_SHARED_SPACE_SYNC} AS mapping \
           ON mapping.{COL_SHARED_SPACE_SYNC_TABLE_NAME} = '{TABLE_S3_BACKENDS}' \
          AND json_extract(mapping.{COL_SHARED_SPACE_SYNC_ROW_PKS}, '$[0]') = s3.{COL_S3_BACKENDS_ID} \
         LEFT JOIN {TABLE_SPACES} AS spaces \
           ON spaces.{COL_SPACES_ID} = mapping.{COL_SHARED_SPACE_SYNC_SPACE_ID} \
         ORDER BY s3.{COL_S3_BACKENDS_NAME}"
    );

    /// Get a single backend by ID (with config)
    pub static ref SQL_GET_BACKEND: String = format!(
        "SELECT {COL_S3_BACKENDS_ID}, {COL_S3_BACKENDS_TYPE}, {COL_S3_BACKENDS_NAME}, \
         {COL_S3_BACKENDS_CONFIG}, {COL_S3_BACKENDS_ENABLED}, {COL_S3_BACKENDS_CREATED_AT} \
         FROM {TABLE_S3_BACKENDS} WHERE {COL_S3_BACKENDS_ID} = ?1"
    );

    /// Get backend config only
    pub static ref SQL_GET_BACKEND_CONFIG: String = format!(
        "SELECT {COL_S3_BACKENDS_TYPE}, {COL_S3_BACKENDS_CONFIG} \
         FROM {TABLE_S3_BACKENDS} WHERE {COL_S3_BACKENDS_ID} = ?1"
    );

    /// Insert a new backend
    pub static ref SQL_INSERT_BACKEND: String = format!(
        "INSERT INTO {TABLE_S3_BACKENDS} \
         ({COL_S3_BACKENDS_ID}, {COL_S3_BACKENDS_TYPE}, {COL_S3_BACKENDS_NAME}, \
          {COL_S3_BACKENDS_CONFIG}, {COL_S3_BACKENDS_ENABLED}) \
         VALUES (?1, ?2, ?3, ?4, 1) \
         RETURNING {COL_S3_BACKENDS_ID}, {COL_S3_BACKENDS_TYPE}, {COL_S3_BACKENDS_NAME}, \
         {COL_S3_BACKENDS_ENABLED}, {COL_S3_BACKENDS_CREATED_AT}"
    );

    /// Delete a backend
    pub static ref SQL_DELETE_BACKEND: String = format!(
        "DELETE FROM {TABLE_S3_BACKENDS} WHERE {COL_S3_BACKENDS_ID} = ?1"
    );

    /// Update backend enabled status
    pub static ref SQL_UPDATE_BACKEND_ENABLED: String = format!(
        "UPDATE {TABLE_S3_BACKENDS} SET {COL_S3_BACKENDS_ENABLED} = ?2 \
         WHERE {COL_S3_BACKENDS_ID} = ?1"
    );

    /// Update backend name and config
    pub static ref SQL_UPDATE_BACKEND: String = format!(
        "UPDATE {TABLE_S3_BACKENDS} SET {COL_S3_BACKENDS_NAME} = ?2, {COL_S3_BACKENDS_CONFIG} = ?3 \
         WHERE {COL_S3_BACKENDS_ID} = ?1 \
         RETURNING {COL_S3_BACKENDS_ID}, {COL_S3_BACKENDS_TYPE}, {COL_S3_BACKENDS_NAME}, \
         {COL_S3_BACKENDS_ENABLED}, {COL_S3_BACKENDS_CREATED_AT}, {COL_S3_BACKENDS_CONFIG}"
    );
}
