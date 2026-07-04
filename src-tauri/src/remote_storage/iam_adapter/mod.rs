//! IAM adapter — provider-neutral trait for creating and revoking scoped
//! access credentials on an S3-compatible object storage provider.
//!
//! Consumed by the `share_storage_backend` command (Phase E). The adapter
//! wraps the vendor-specific IAM control-plane API and returns a small,
//! opaque [`ScopedCred`] the vault can pass through spaces.
//!
//! Provider support (v1):
//! - [`ProviderFlavor::Aws`]     — AWS IAM (`https://iam.amazonaws.com`, region `us-east-1`)
//! - [`ProviderFlavor::Wasabi`]  — Wasabi IAM (`https://iam.wasabisys.com`, region `us-east-1`)
//! - [`ProviderFlavor::MinIO`]   — NOT SUPPORTED — MinIO uses an admin-JSON API,
//!                                  not the AWS-IAM-XML surface. See
//!                                  `docs/plans/2026-07-04-s3-bucket-sharing-via-spaces-design.md`
//!                                  §D3 (separate task).
//!
//! We deliberately avoid `aws-sdk-iam` (see `Cargo.toml` note near the
//! `aws-sigv4` dep entry) to keep Android builds free of `aws-lc-rs`.

use crate::remote_storage::iam_policy::IamPolicy;

pub mod aws_compat;

pub use aws_compat::{AwsCompatIamAdapter, ProviderFlavor};

/// Materialised access credential produced by [`IamAdapter::create_scoped_user`].
///
/// Custom `Debug` impl redacts the secret to prevent leaks via
/// `tracing::debug!`, `dbg!`, or panic messages (mirrors [`crate::remote_storage::iam_admin_creds::IamAdminCred`]).
#[derive(Clone, PartialEq, Eq)]
pub struct ScopedCred {
    /// AWS-style access-key id.
    pub access_key_id: String,
    /// Secret access key. Never log; never expose through a public command.
    pub secret_access_key: String,
    /// The IAM user name that owns this access-key — required to call
    /// [`IamAdapter::delete_scoped_user`] later.
    pub iam_user_name: String,
}

impl std::fmt::Debug for ScopedCred {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScopedCred")
            .field("access_key_id", &"<redacted>")
            .field("secret_access_key", &"<redacted>")
            .field("iam_user_name", &self.iam_user_name)
            .finish()
    }
}

/// Errors surfaced by the IAM adapter. The variant carries just enough
/// context for the caller to distinguish "cred not found" (idempotent path)
/// from "we don't have permission" from generic transport failure.
#[derive(thiserror::Error, Debug)]
pub enum IamAdapterError {
    /// Network transport or provider-side 5xx / unexpected shape.
    #[error("network or provider error: {0}")]
    Network(String),
    /// Provider returned `AccessDenied` / `UnauthorizedOperation` — the
    /// admin credential does not have the requested IAM permission.
    #[error("access denied: {0}")]
    AccessDenied(String),
    /// Provider returned `NoSuchEntity` — treated as success on the delete
    /// path (idempotent revoke). Returned so callers can distinguish it
    /// from real failures if they need to.
    #[error("not found (idempotent path)")]
    NotFound,
    /// Anything else — malformed XML, unexpected status code, missing
    /// fields in a response we successfully received.
    #[error("unexpected: {0}")]
    Other(String),
}

/// Provider-neutral IAM control-plane surface used by the sharing feature.
///
/// All methods are `async` and require `Send + Sync` so the adapter can
/// live behind a `dyn IamAdapter` inside a tauri command handler that
/// spawns tasks.
#[async_trait::async_trait]
pub trait IamAdapter: Send + Sync {
    /// Create a scoped IAM user + inline policy + access-key, in that order.
    ///
    /// Not idempotent — call with a fresh unique `user_name` each time
    /// (partial-failure is a bug the caller must observe).
    async fn create_scoped_user(
        &self,
        user_name: &str,
        policy_name: &str,
        policy: &IamPolicy,
    ) -> Result<ScopedCred, IamAdapterError>;

    /// Revoke a previously-issued scoped user. Idempotent: a missing
    /// access-key, policy, or user is treated as success (the underlying
    /// `NoSuchEntity` response is swallowed).
    async fn delete_scoped_user(
        &self,
        user_name: &str,
        access_key_id: &str,
    ) -> Result<(), IamAdapterError>;

    /// Cheap probe to check whether the admin cred has enough IAM
    /// permission to run the share workflow. Used by the pre-share
    /// capability check.
    ///
    /// Returns `Ok(true)` on success, `Ok(false)` on structural
    /// `AccessDenied`. Transport failures propagate as
    /// [`IamAdapterError::Network`].
    async fn probe_iam_capability(&self) -> Result<bool, IamAdapterError>;
}

#[cfg(test)]
mod tests;
