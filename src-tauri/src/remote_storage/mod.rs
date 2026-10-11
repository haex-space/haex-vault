//! holzi's own S3 (spec 038, plan.md "Structure Decision"): storage connections (provider,
//! endpoint, region, addressing; the credentials are an entry of the password manager owned by
//! `storage`, rule Z14 of spec 034) and the storages on them, one bucket each. holzi speaks the
//! protocol itself; extensions are one consumer of it (PR D), spec 029 another.
//!
//! [`RemoteStore`] is the one seam to the provider: [`s3::S3Store`] signs with `rusty-s3` and sends
//! with `reqwest` to the address [`address`] checked and pinned (research R1, R8); tests use a fake
//! (research R10). No error of this module carries an endpoint, a header or a body of the provider.

pub mod address;
#[cfg(test)]
mod address_tests;
pub mod commands;
pub mod credentials;
#[cfg(test)]
mod credentials_tests;
pub mod model;
pub mod probe;
#[cfg(test)]
mod probe_tests;
pub mod s3;
mod s3_ext;
#[cfg(test)]
mod s3_ext_tests;
mod s3_multipart;
#[cfg(test)]
mod s3_tests;
pub mod service;
#[cfg(test)]
mod service_tests;
pub mod store;
#[cfg(test)]
mod store_tests;
#[cfg(test)]
pub(crate) mod test_support;

use std::fmt;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::time::Instant;
use ts_rs::TS;
use zeroize::Zeroizing;

/// The provider a connection was created for; it only prefills endpoint and addressing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub enum ProviderKind {
    Aws,
    Rustfs,
    Other,
}

/// How a bucket appears in the address: `https://host/<bucket>/<key>` or
/// `https://<bucket>.host/<key>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub enum Addressing {
    Path,
    Virtual,
}

/// The addresses an endpoint may reach (research R8, FR-009b), fixed when the endpoint is set:
/// `local` when its host resolves to loopback or private addresses, else `public`. Every call
/// checks each resolved address against it, so a public name that later points into the local
/// network (DNS rebinding) reaches nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub enum EndpointScope {
    Public,
    Local,
}

/// The result of a connection test (research R9), stored per device in `storage_tests_no_sync`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub enum TestOutcome {
    Passed,
    AccessDenied,
    Unreachable,
    BucketMissing,
    MissingRight,
}

/// Whether the credentials entry of a connection is on this device (data-model.md): `syncing`
/// when it has not arrived yet, `missing` when the user deleted it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub enum CredentialsState {
    Present,
    Syncing,
    Missing,
}

macro_rules! text_enum {
    ($ty:ty { $($variant:ident = $text:literal),+ $(,)? }) => {
        impl $ty {
            /// The text stored in the vault.
            pub fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $text),+ }
            }

            /// The value of a stored text; `None` for an unknown one.
            pub fn parse(text: &str) -> Option<Self> {
                match text { $($text => Some(Self::$variant),)+ _ => None }
            }
        }
    };
}

text_enum!(ProviderKind { Aws = "aws", Rustfs = "rustfs", Other = "other" });
text_enum!(Addressing { Path = "path", Virtual = "virtual" });
text_enum!(EndpointScope { Public = "public", Local = "local" });
text_enum!(TestOutcome {
    Passed = "passed",
    AccessDenied = "accessDenied",
    Unreachable = "unreachable",
    BucketMissing = "bucketMissing",
    MissingRight = "missingRight",
});

/// A row of `haex_storage_connections`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionRow {
    pub id: String,
    pub provider_name: String,
    pub provider_kind: ProviderKind,
    /// Empty for `aws`: the address follows from the region.
    pub endpoint: String,
    pub endpoint_scope: EndpointScope,
    pub region: String,
    pub addressing: Addressing,
    pub credentials_item_id: String,
    pub created_at: String,
    pub updated_at: String,
}

/// A row of `haex_storages`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageRow {
    pub id: String,
    pub connection_id: String,
    pub name: String,
    pub bucket: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Credentials of a connection; `Debug` shows only the access key id.
#[derive(Clone)]
pub struct Credentials {
    pub access_key_id: String,
    pub secret_access_key: Zeroizing<String>,
    pub session_token: Option<Zeroizing<String>>,
}

impl fmt::Debug for Credentials {
    /// Formats credentials with the secret key and any session token redacted.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Credentials")
            .field("access_key_id", &self.access_key_id)
            .field("secret_access_key", &"<redacted>")
            .field(
                "session_token",
                &self.session_token.as_ref().map(|_| "<redacted>"),
            )
            .finish()
    }
}

/// Where one bucket lives: the connection's address data plus the bucket.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    pub provider_kind: ProviderKind,
    pub endpoint: String,
    pub endpoint_scope: EndpointScope,
    pub region: String,
    pub addressing: Addressing,
    pub bucket: String,
}

impl Location {
    /// The location of `bucket` on `connection`.
    pub fn of(connection: &ConnectionRow, bucket: &str) -> Self {
        Self {
            provider_kind: connection.provider_kind,
            endpoint: connection.endpoint.clone(),
            endpoint_scope: connection.endpoint_scope,
            region: connection.region.clone(),
            addressing: connection.addressing,
            bucket: bucket.to_owned(),
        }
    }
}

/// What a call needs to reach a bucket.
#[derive(Debug, Clone)]
pub struct Access {
    pub location: Location,
    pub credentials: Credentials,
}

/// One object of a listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectInfo {
    pub key: String,
    pub size: u64,
    pub last_modified: String,
    /// The provider's ETag, quotes included; changes with the content (spec 048, research R6).
    pub etag: String,
}

/// What a `HEAD` tells about an object (spec 044 FR-034).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectHead {
    pub size: u64,
    /// The provider's `Last-Modified` (an HTTP date), when it sent one.
    pub last_modified: Option<String>,
    /// The provider's ETag, when it sent one.
    pub etag: Option<String>,
}

/// One level of a bucket below a prefix (spec 044 FR-034): the objects right there and the
/// prefixes of deeper ones, as S3 answers with `delimiter=/`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DirListing {
    /// Without the folder marker (the object named like the prefix itself).
    pub objects: Vec<ObjectInfo>,
    /// Each ending in `/`.
    pub prefixes: Vec<String>,
}

/// What went wrong at the provider, without its text (research R7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum StorageError {
    /// The object or the bucket does not exist.
    #[error("not found")]
    NotFound,
    /// The provider refused the credentials (wrong key, wrong signature, expired token).
    #[error("access denied")]
    AccessDenied,
    /// The credentials are valid but lack the right for this action (research R9 `missingRight`).
    #[error("missing right")]
    MissingRight,
    /// The provider could not be reached, answered with a server error or a redirect, or its
    /// address is not allowed (research R8).
    #[error("network")]
    Network,
    /// An answer or a listing is larger than the limit.
    #[error("too large")]
    TooLarge,
    /// The deadline passed.
    #[error("timed out")]
    TimedOut,
}

/// The provider behind a storage (research R10): the S3 implementation, or a fake in tests. Every
/// call ends at `deadline`.
#[async_trait]
pub trait RemoteStore: Send + Sync {
    /// Writes `body` to `key` in the bucket described by `access`, before `deadline`.
    async fn put(
        &self,
        access: &Access,
        key: &str,
        body: Vec<u8>,
        deadline: Instant,
    ) -> Result<(), StorageError>;

    /// The object, read up to `max_bytes` (more is [`StorageError::TooLarge`]).
    async fn get(
        &self,
        access: &Access,
        key: &str,
        max_bytes: usize,
        deadline: Instant,
    ) -> Result<Vec<u8>, StorageError>;

    /// The objects under `prefix`, following the provider's pages up to `max` (more is
    /// [`StorageError::TooLarge`]).
    async fn list(
        &self,
        access: &Access,
        prefix: &str,
        max: usize,
        deadline: Instant,
    ) -> Result<Vec<ObjectInfo>, StorageError>;

    /// Deletes `key` from the bucket described by `access`, before `deadline`.
    async fn delete(
        &self,
        access: &Access,
        key: &str,
        deadline: Instant,
    ) -> Result<(), StorageError>;

    /// Size and time of `key` (spec 044).
    async fn head(
        &self,
        access: &Access,
        key: &str,
        deadline: Instant,
    ) -> Result<ObjectHead, StorageError>;

    /// A reader for `len` bytes of `key` from `start` (spec 044 FR-012, FR-013). `deadline` holds
    /// until the answer starts; the body then streams without one (a film plays for hours) and is
    /// never held whole.
    async fn get_range(
        &self,
        access: &Access,
        key: &str,
        start: u64,
        len: u64,
        deadline: Instant,
    ) -> Result<Box<dyn tokio::io::AsyncRead + Send + Unpin>, StorageError>;

    /// One level below `prefix` (spec 044 FR-034), following pages up to `max` entries (more is
    /// [`StorageError::TooLarge`]).
    async fn list_dir(
        &self,
        access: &Access,
        prefix: &str,
        max: usize,
        deadline: Instant,
    ) -> Result<DirListing, StorageError>;

    /// Copies `from` to `to` in the same bucket on the provider (spec 044: renaming, moving within
    /// a storage); objects up to 5 GB, the limit of S3's `CopyObject`.
    async fn copy(
        &self,
        access: &Access,
        from: &str,
        to: &str,
        deadline: Instant,
    ) -> Result<(), StorageError>;
    /// Starts an upload in parts of `key` (spec 044 FR-020); returns its id.
    async fn create_multipart(
        &self,
        access: &Access,
        key: &str,
        deadline: Instant,
    ) -> Result<String, StorageError>;

    /// Uploads part `number` (from 1) of an upload; returns the part's ETag.
    async fn upload_part(
        &self,
        access: &Access,
        key: &str,
        upload_id: &str,
        number: u16,
        body: Vec<u8>,
        deadline: Instant,
    ) -> Result<String, StorageError>;

    /// Joins the parts (their ETags in order) into the object.
    async fn complete_multipart(
        &self,
        access: &Access,
        key: &str,
        upload_id: &str,
        etags: &[String],
        deadline: Instant,
    ) -> Result<(), StorageError>;

    /// Drops an upload and its parts, so nothing half stays at the provider (FR-020).
    async fn abort_multipart(
        &self,
        access: &Access,
        key: &str,
        upload_id: &str,
        deadline: Instant,
    ) -> Result<(), StorageError>;
}
