// src-tauri/src/storage/error.rs
//!
//! Storage Error Types
//!

use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error, Serialize)]
#[serde(tag = "type", content = "details")]
pub enum StorageError {
    #[error("Backend not found: {id}")]
    BackendNotFound { id: String },

    #[error("Backend connection failed: {reason}")]
    ConnectionFailed { reason: String },

    #[error("Upload failed: {reason}")]
    UploadFailed { reason: String },

    #[error("Download failed: {reason}")]
    DownloadFailed { reason: String },

    #[error("Delete failed: {reason}")]
    DeleteFailed { reason: String },

    #[error("Object not found: {key}")]
    ObjectNotFound { key: String },

    #[error("Invalid configuration: {reason}")]
    InvalidConfig { reason: String },

    #[error("Database error: {reason}")]
    DatabaseError { reason: String },

    #[error("Internal error: {reason}")]
    Internal { reason: String },

    // --- share_storage_backend variants -------------------------------------
    /// Command arguments did not pass structural validation. Frontend should
    /// surface the reason to the user; not a retriable error.
    #[error("Invalid arguments: {reason}")]
    InvalidArgs { reason: String },

    /// The owner-side S3 backend row referenced by `storage_id` was not found
    /// (or is not in state `origin_type = 'owned'`). Distinct from
    /// [`StorageError::BackendNotFound`] so the frontend can distinguish the
    /// share flow's precondition failure from a generic missing backend.
    #[error("Storage backend not found: {storage_id}")]
    StorageNotFound { storage_id: String },

    /// The share flow requires long-lived IAM admin credentials, but none are
    /// stored for this backend. Frontend catches this variant, prompts the
    /// user for the cred, then re-invokes the command with
    /// `iamAdminCredHint` populated.
    #[error("IAM admin credential missing for storage {storage_id}")]
    IamAdminCredMissing { storage_id: String },

    /// The admin credential is present but does not have enough IAM
    /// permission to run the share workflow (structural `AccessDenied` on the
    /// probe, or `probe_iam_capability` returned `Ok(false)`).
    #[error("IAM admin credential lacks required permissions")]
    IamAdminInsufficient,

    /// The `providerType` on the loaded IAM admin cred is not one this build
    /// can drive. `"aws"` and `"wasabi"` are supported today; MinIO uses a
    /// separate JSON admin API (deferred task).
    #[error("Unsupported IAM provider: {provider_type}")]
    UnsupportedProvider { provider_type: String },

    /// The IAM adapter reported a failure after we handed off to it — network
    /// error, unexpected XML, or provider-side 5xx. The scoped user may or
    /// may not exist on the provider; the adapter is responsible for its own
    /// best-effort rollback before returning.
    ///
    /// `operation` identifies the specific IAM operation that failed (e.g.
    /// `"create_scoped_user"`, `"delete_scoped_user"`, `"probe_iam_capability"`),
    /// so operators can distinguish share- from revoke- from probe-path failures
    /// without parsing the free-form `reason` string.
    #[error("IAM operation {operation} failed: {reason}")]
    IamOperationFailed { operation: String, reason: String },

    /// Object-scoped shares (a single S3 key) are not yet wired end-to-end.
    /// v1 supports whole-bucket and prefix-scoped shares only; see
    /// `docs/plans/2026-07-04-s3-bucket-sharing-via-spaces-design.md` for the
    /// follow-up scope-kind column task.
    #[error("Object-scope share is not yet supported")]
    ObjectScopeNotYetSupported,

    // --- revoke_storage_share variants --------------------------------------
    /// The row referenced by `sharedBackendId` exists but is not a
    /// `origin_type = 'shared_from_space'` row. Callers should not attempt to
    /// revoke an owned backend — those must be removed through the normal
    /// storage-backend delete path.
    #[error("Backend row is not a shared-from-space row (origin_type = {origin_type})")]
    NotAShareRow { origin_type: String },

    /// The shared row exists but its `parent_backend_id` does not resolve to
    /// a row in `haex_s3_backends`. Indicates data corruption or a race with
    /// a concurrent owner-side delete.
    #[error("Parent backend not found: {parent_backend_id}")]
    ParentBackendMissing { parent_backend_id: String },
}

impl From<rusqlite::Error> for StorageError {
    fn from(e: rusqlite::Error) -> Self {
        StorageError::DatabaseError {
            reason: e.to_string(),
        }
    }
}
