//! holzi's own storages (spec 038, spec 050): storage connections (protocol, provider, endpoint,
//! region, addressing; the credentials are an entry of the password manager owned by `storage`,
//! rule Z14 of spec 034) and the storages on them, one bucket or folder each. holzi speaks the
//! protocols itself; extensions are one consumer of it, spec 029 another.
//!
//! [`RemoteStore`] is the one seam to the provider: [`protocol::ProtocolStore`] hands each call to
//! the store of the connection's protocol (spec 050 research R1); [`s3::S3Store`] signs with
//! `rusty-s3` and sends with `reqwest` to the address [`address`] checked and pinned (038 research
//! R1, R8); tests use a fake (038 research R10). No error of this module carries an endpoint, a
//! header or a body of the provider.

pub mod address;
#[cfg(test)]
mod address_tests;
pub mod commands;
pub mod credentials;
#[cfg(test)]
mod credentials_tests;
mod http;
#[cfg(test)]
mod http_tests;
pub mod local;
mod local_fs;
#[cfg(test)]
mod mod_tests;
pub mod model;
pub mod probe;
#[cfg(test)]
mod probe_tests;
pub mod protocol;
#[cfg(test)]
mod protocol_tests;
mod remote_store;
pub mod s3;
mod s3_ext;
#[cfg(test)]
mod s3_ext_tests;
mod s3_multipart;
#[cfg(test)]
mod s3_tests;
pub mod service;
#[cfg(test)]
mod service_endpoint_tests;
#[cfg(test)]
mod service_protocol_tests;
#[cfg(test)]
mod service_sftp_tests;
#[cfg(test)]
mod service_tests;
pub mod sftp;
pub mod store;
#[cfg(test)]
mod store_tests;
#[cfg(test)]
pub(crate) mod test_support;
#[cfg(test)]
mod test_support_env;
mod tree;
#[cfg(test)]
mod tree_tests;
pub mod webdav;

use std::fmt;

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use zeroize::Zeroizing;

pub use remote_store::RemoteStore;
pub use tree::{case_clash, PART_PREFIX};

/// The protocol of a connection (spec 050 FR-001); it never changes once the connection is saved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    S3,
    WebDav,
    Sftp,
    /// A folder on this device (spec 048 US7, research R15): only for encrypted folders there,
    /// never saved as a connection, never offered and never sent to or taken from the window.
    #[serde(skip)]
    #[ts(skip)]
    Local,
}

impl Protocol {
    /// Whether folders are real folders on the server (spec 050 research R8): WebDAV, SFTP and a
    /// local folder.
    pub fn is_tree(self) -> bool {
        matches!(self, Self::WebDav | Self::Sftp | Self::Local)
    }

    /// Whether `kind` is a provider of this protocol (data-model.md).
    pub fn fits(self, kind: ProviderKind) -> bool {
        use ProviderKind::*;
        match self {
            Self::S3 => matches!(kind, Aws | Rustfs | Other),
            Self::WebDav => matches!(kind, HetznerStorageBox | Nextcloud | Koofr | Kdrive | Other),
            Self::Sftp => matches!(kind, HetznerStorageBox | Other),
            Self::Local => false,
        }
    }
}

/// The provider a connection was created for; it only prefills endpoint and addressing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub enum ProviderKind {
    Aws,
    Rustfs,
    HetznerStorageBox,
    Nextcloud,
    Koofr,
    Kdrive,
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
    /// The folder of a WebDAV or SFTP storage does not exist (spec 050 research R10).
    FolderMissing,
    /// The address does not answer as WebDAV (refused `PROPFIND`, a redirect).
    NotWebDav,
    /// The SFTP server showed a host key nobody confirmed yet.
    HostKeyUnknown,
    /// The SFTP server showed another host key than the confirmed one.
    HostKeyChanged,
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

impl Protocol {
    /// The text stored in the vault; `local` only shows in logs, it is never stored.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::S3 => "s3",
            Self::WebDav => "webdav",
            Self::Sftp => "sftp",
            Self::Local => "local",
        }
    }

    /// The value of a stored text; `None` for an unknown one and for `local`, so no synced row can
    /// point a storage at a folder of this device.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "s3" => Some(Self::S3),
            "webdav" => Some(Self::WebDav),
            "sftp" => Some(Self::Sftp),
            _ => None,
        }
    }
}
text_enum!(ProviderKind {
    Aws = "aws",
    Rustfs = "rustfs",
    HetznerStorageBox = "hetznerStorageBox",
    Nextcloud = "nextcloud",
    Koofr = "koofr",
    Kdrive = "kdrive",
    Other = "other",
});
text_enum!(Addressing { Path = "path", Virtual = "virtual" });
text_enum!(EndpointScope { Public = "public", Local = "local" });
text_enum!(TestOutcome {
    Passed = "passed",
    AccessDenied = "accessDenied",
    Unreachable = "unreachable",
    BucketMissing = "bucketMissing",
    MissingRight = "missingRight",
    FolderMissing = "folderMissing",
    NotWebDav = "notWebDav",
    HostKeyUnknown = "hostKeyUnknown",
    HostKeyChanged = "hostKeyChanged",
});

/// A row of `haex_storage_connections`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionRow {
    pub id: String,
    pub protocol: Protocol,
    pub provider_name: String,
    pub provider_kind: ProviderKind,
    /// Empty for `aws`: the address follows from the region.
    pub endpoint: String,
    pub endpoint_scope: EndpointScope,
    pub region: String,
    pub addressing: Addressing,
    /// The confirmed host key of an SFTP server as an OpenSSH line (spec 050 research R5).
    pub host_key: Option<String>,
    /// When the user confirmed `host_key` (RFC 3339).
    pub host_key_confirmed_at: Option<String>,
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
    /// The bucket (S3) or the folder below the connection (WebDAV, SFTP; empty for the root).
    pub bucket: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Credentials of a connection, by protocol (spec 050 data-model.md); `Debug` shows only the access
/// key id or the user name.
#[derive(Clone, Debug)]
pub enum Credentials {
    S3(S3Credentials),
    /// WebDAV, or SFTP with a password.
    Password(PasswordCredentials),
    /// SFTP with a private key.
    SshKey(SshKeyCredentials),
}

/// An S3 access key.
#[derive(Clone)]
pub struct S3Credentials {
    pub access_key_id: String,
    pub secret_access_key: Zeroizing<String>,
    pub session_token: Option<Zeroizing<String>>,
}

impl fmt::Debug for S3Credentials {
    /// Formats credentials with the secret key and any session token redacted.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("S3Credentials")
            .field("access_key_id", &self.access_key_id)
            .field("secret_access_key", &"<redacted>")
            .field(
                "session_token",
                &self.session_token.as_ref().map(|_| "<redacted>"),
            )
            .finish()
    }
}

/// A user name and a password.
#[derive(Clone)]
pub struct PasswordCredentials {
    pub username: String,
    pub password: Zeroizing<String>,
}

impl fmt::Debug for PasswordCredentials {
    /// Formats credentials with the password redacted.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PasswordCredentials")
            .field("username", &self.username)
            .field("password", &"<redacted>")
            .finish()
    }
}

/// A user name and a private SSH key (as text), optionally with its passphrase.
#[derive(Clone)]
pub struct SshKeyCredentials {
    pub username: String,
    pub private_key: Zeroizing<String>,
    pub passphrase: Option<Zeroizing<String>>,
}

impl fmt::Debug for SshKeyCredentials {
    /// Formats credentials with the key and any passphrase redacted.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SshKeyCredentials")
            .field("username", &self.username)
            .field("private_key", &"<redacted>")
            .field(
                "passphrase",
                &self.passphrase.as_ref().map(|_| "<redacted>"),
            )
            .finish()
    }
}

/// Where one bucket or folder lives: the connection's address data plus the bucket or folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    pub protocol: Protocol,
    pub provider_kind: ProviderKind,
    pub endpoint: String,
    pub endpoint_scope: EndpointScope,
    pub region: String,
    pub addressing: Addressing,
    /// The bucket (S3) or the folder (WebDAV, SFTP).
    pub bucket: String,
    /// The confirmed host key (SFTP).
    pub host_key: Option<String>,
}

impl Location {
    /// The location of `bucket` on `connection`.
    pub fn of(connection: &ConnectionRow, bucket: &str) -> Self {
        Self {
            protocol: connection.protocol,
            provider_kind: connection.provider_kind,
            endpoint: connection.endpoint.clone(),
            endpoint_scope: connection.endpoint_scope,
            region: connection.region.clone(),
            addressing: connection.addressing,
            bucket: bucket.to_owned(),
            host_key: connection.host_key.clone(),
        }
    }

    /// Whether folders are real folders on the server (WebDAV, SFTP).
    pub fn is_tree(&self) -> bool {
        self.protocol.is_tree()
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
    /// The provider's quota is used up (WebDAV 507; spec 050 research R10).
    #[error("quota exceeded")]
    QuotaExceeded,
    /// The SFTP server showed another host key than the confirmed one; nothing was sent.
    #[error("host key changed")]
    HostKeyChanged,
    /// The SFTP server showed a host key nobody confirmed yet; nothing was sent.
    #[error("host key unknown")]
    HostKeyUnknown,
    /// The protocol does not offer this call (spec 050 research R8).
    #[error("unsupported")]
    Unsupported,
    /// The server does not answer as its protocol expects: a redirect, a refused `PROPFIND`, an
    /// answer that does not parse (spec 050 research R2).
    #[error("protocol")]
    Protocol,
}
