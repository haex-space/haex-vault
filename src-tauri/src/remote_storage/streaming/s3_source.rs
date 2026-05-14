//! S3 streaming source.
//!
//! Wraps a `rust-s3` `Bucket` and translates `StreamingSource` calls into
//! `get_object_range` / `head_object`. The bucket is built via the shared
//! `build_s3_bucket` helper in `backend.rs` so streaming and CRUD stay in
//! sync on endpoint/path-prefix quirks.

use async_trait::async_trait;
use s3::bucket::Bucket;
use serde_json::Value as JsonValue;

use super::source::{ByteRange, StreamingError, StreamingSource};
use crate::database::core;
use crate::database::row::get_string;
use crate::database::DbConnection;
use crate::remote_storage::backend::build_s3_bucket;
use crate::remote_storage::queries::SQL_GET_BACKEND_CONFIG;
use crate::remote_storage::types::S3Config;

pub struct S3StreamingSource {
    bucket: Box<Bucket>,
    key: String,
}

impl S3StreamingSource {
    /// Resolve a backend by ID, load its config from the DB, and build a
    /// streaming source for `key`.
    ///
    /// Fails fast if the backend doesn't exist or isn't an S3 backend —
    /// other target types route to other source impls in the protocol
    /// handler factory, so reaching this constructor with a non-S3 backend
    /// is a programmer error in the URL scheme parsing.
    pub async fn from_backend_id(
        db: &DbConnection,
        backend_id: &str,
        key: &str,
    ) -> Result<Self, StreamingError> {
        let rows = core::select_with_crdt(
            SQL_GET_BACKEND_CONFIG.clone(),
            vec![JsonValue::String(backend_id.to_string())],
            db,
        )
        .map_err(|e| StreamingError::Backend(format!("DB lookup: {e}")))?;

        let row = rows.first().ok_or_else(|| {
            StreamingError::NotFound(format!("backend {backend_id}"))
        })?;

        let backend_type = get_string(row, 0);
        if backend_type != "s3" {
            return Err(StreamingError::BadRequest(format!(
                "backend {backend_id} is type {backend_type:?}, not s3"
            )));
        }

        let config_str = get_string(row, 1);
        let config: S3Config = serde_json::from_str(&config_str)
            .map_err(|e| StreamingError::Backend(format!("parse config: {e}")))?;

        let setup = build_s3_bucket(&config)
            .map_err(|e| StreamingError::Backend(format!("build bucket: {e}")))?;

        Ok(Self {
            bucket: setup.bucket,
            key: key.to_string(),
        })
    }
}

#[async_trait]
impl StreamingSource for S3StreamingSource {
    async fn size(&self) -> Result<u64, StreamingError> {
        let (head, _status) = self
            .bucket
            .head_object(&self.key)
            .await
            .map_err(|e| {
                let msg = e.to_string();
                if msg.contains("404") || msg.contains("NoSuchKey") {
                    StreamingError::NotFound(self.key.clone())
                } else {
                    StreamingError::Backend(format!("head_object: {e}"))
                }
            })?;
        head.content_length
            .and_then(|n| u64::try_from(n).ok())
            .ok_or_else(|| {
                StreamingError::Backend(format!(
                    "S3 returned no/invalid Content-Length for {}",
                    self.key
                ))
            })
    }

    async fn read_range(&self, range: ByteRange) -> Result<Vec<u8>, StreamingError> {
        // rust-s3's `get_object_range` takes (start, Some(end)) as an
        // inclusive range — matches HTTP `Range: bytes=start-end` exactly.
        let response = self
            .bucket
            .get_object_range(&self.key, range.start, Some(range.end))
            .await
            .map_err(|e| StreamingError::Backend(format!("get_object_range: {e}")))?;

        Ok(response.bytes().to_vec())
    }

    async fn content_type(&self) -> Option<String> {
        // Re-issue HEAD only when actually asked. Most range requests don't
        // need the content type to be re-fetched per range — protocol
        // handler caches the response from the first call within a single
        // request.
        let (head, _status) = self.bucket.head_object(&self.key).await.ok()?;
        head.content_type
    }
}
