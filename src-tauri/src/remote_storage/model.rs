//! What the settings send and get back (spec 038 and spec 050, contracts/tauri-commands.md). No
//! answer carries a secret; the secret only travels in [`CredentialsInput`], from the window to Rust.

use std::fmt;

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use zeroize::Zeroizing;

use super::{Addressing, CredentialsState, EndpointScope, Protocol, ProviderKind, TestOutcome};

/// Credentials as typed by the user, by kind (spec 050 contracts/tauri-commands.md): `s3` only for
/// S3 connections, `password` for WebDAV and SFTP, `sshKey` and `generatedKey` only for SFTP; or
/// `entry`, an entry of the password manager for any protocol (spec 052).
/// `Debug` shows only the access key id or the user name.
#[derive(Clone, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CredentialsInput {
    #[serde(rename_all = "camelCase")]
    S3 {
        access_key_id: String,
        #[ts(type = "string")]
        secret_access_key: Zeroizing<String>,
        #[ts(type = "string | null", optional)]
        #[serde(default)]
        session_token: Option<Zeroizing<String>>,
    },
    #[serde(rename_all = "camelCase")]
    Password {
        username: String,
        #[ts(type = "string")]
        password: Zeroizing<String>,
    },
    #[serde(rename_all = "camelCase")]
    SshKey {
        username: String,
        #[ts(type = "string")]
        private_key: Zeroizing<String>,
        #[ts(type = "string | null", optional)]
        #[serde(default)]
        passphrase: Option<Zeroizing<String>>,
    },
    /// A key holzi generated (`storage_ssh_key_generate`); its private half never left Rust.
    #[serde(rename_all = "camelCase")]
    GeneratedKey { username: String, key_id: String },
    /// An entry of the password manager the user chose (spec 052): the connection points at it,
    /// its values never travel through the window.
    #[serde(rename_all = "camelCase")]
    Entry { item_id: String },
}

impl fmt::Debug for CredentialsInput {
    /// Formats credential input with only the access key ID or the user name, omitting secrets.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::S3 { access_key_id, .. } => f
                .debug_struct("CredentialsInput::S3")
                .field("access_key_id", access_key_id)
                .finish_non_exhaustive(),
            Self::Password { username, .. } => f
                .debug_struct("CredentialsInput::Password")
                .field("username", username)
                .finish_non_exhaustive(),
            Self::SshKey { username, .. } => f
                .debug_struct("CredentialsInput::SshKey")
                .field("username", username)
                .finish_non_exhaustive(),
            Self::GeneratedKey { username, .. } => f
                .debug_struct("CredentialsInput::GeneratedKey")
                .field("username", username)
                .finish_non_exhaustive(),
            Self::Entry { item_id } => f
                .debug_struct("CredentialsInput::Entry")
                .field("item_id", item_id)
                .finish(),
        }
    }
}

/// `storage_connection_save`: a new connection without `id`, otherwise a change of one.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct ConnectionInput {
    #[ts(optional)]
    #[serde(default)]
    pub id: Option<String>,
    /// The protocol; a change of a saved connection must keep it (spec 050 FR-001).
    #[serde(default = "s3")]
    pub protocol: Protocol,
    pub provider_name: String,
    pub provider_kind: ProviderKind,
    /// Empty or absent only for `aws`.
    #[ts(optional)]
    #[serde(default)]
    pub endpoint: Option<String>,
    pub region: String,
    pub addressing: Addressing,
    /// New credentials; required for a new connection.
    #[ts(optional)]
    #[serde(default)]
    pub credentials: Option<CredentialsInput>,
    /// The bucket or folder the test before saving uses (FR-003).
    pub bucket_for_test: String,
    /// Create that folder when the test finds it missing (WebDAV, SFTP; spec 050 FR-006): the
    /// user said so after a first test answered `folderMissing`.
    #[ts(as = "Option<bool>", optional)]
    #[serde(default)]
    pub create_folder: bool,
    /// The host key (an OpenSSH line) the user confirmed after `hostKeyUnknown` (SFTP, spec 050
    /// research R5); holzi then accepts exactly this key.
    #[ts(optional)]
    #[serde(default)]
    pub host_key: Option<String>,
}

/// The protocol of a connection input that names none (the settings before spec 050).
fn s3() -> Protocol {
    Protocol::S3
}

/// `storage_save`: a new storage without `id`, otherwise a change of one.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct StorageInput {
    #[ts(optional)]
    #[serde(default)]
    pub id: Option<String>,
    pub connection_id: String,
    pub name: String,
    pub bucket: String,
    /// Create the folder when the test finds it missing (spec 050 FR-006), on the user's word.
    #[ts(as = "Option<bool>", optional)]
    #[serde(default)]
    pub create_folder: bool,
}

/// A connection as the settings show it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct ConnectionView {
    pub id: String,
    pub protocol: Protocol,
    pub provider_name: String,
    pub provider_kind: ProviderKind,
    pub endpoint: String,
    pub region: String,
    pub addressing: Addressing,
    /// The endpoint sends without encryption (`http`).
    pub insecure: bool,
    /// Whether the endpoint lies in the local network or on this device (research R8).
    pub endpoint_scope: EndpointScope,
    pub credentials: CredentialsState,
    /// The non-secret entry currently linked to this connection, when it is still visible.
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credentials_entry: Option<CredentialsEntry>,
    /// The confirmed host key of an SFTP connection (spec 050 research R5).
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host_key: Option<ConfirmedHostKey>,
}

/// The password-manager entry linked to a connection, without any secret value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct CredentialsEntry {
    pub id: String,
    pub title: Option<String>,
    pub owned: bool,
}

/// The host key a user confirmed for an SFTP connection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct ConfirmedHostKey {
    /// `SHA256:…`, as OpenSSH shows it.
    pub fingerprint: String,
    pub confirmed_at: String,
}

/// The host key an SFTP server showed when it was not the confirmed one (spec 050 research R5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct ShownHostKey {
    /// The OpenSSH line, which the window sends back once the user confirmed it.
    pub key: String,
    /// `SHA256:…`, to compare with what the provider publishes.
    pub fingerprint: String,
}

/// The key `storage_ssh_key_generate` made: its id and its public key line, never its private key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct GeneratedKey {
    pub key_id: String,
    /// `ssh-ed25519 AAAA… holzi`, for the provider's list of keys.
    pub public_key: String,
}

/// An entry of the password manager that fits a protocol, for the choice in the form (spec 052
/// contracts/tauri-commands.md): never a secret, for S3 `username` is the access key id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct CredentialsCandidate {
    pub id: String,
    pub title: Option<String>,
    pub username: String,
    pub url: Option<String>,
    /// It carries the tag of cloud storage credentials.
    pub tagged: bool,
    /// holzi created it for a connection (owner `storage`).
    pub owned: bool,
}

/// The last test of a storage on this device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct LastTest {
    pub at: String,
    pub outcome: TestOutcome,
}

/// A storage as the settings show it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct StorageView {
    pub id: String,
    pub connection_id: String,
    pub name: String,
    /// The bucket (S3) or the folder (WebDAV, SFTP).
    pub bucket: String,
    pub last_test: Option<LastTest>,
    /// The extensions with a permission for this storage.
    pub extensions: Vec<String>,
    /// Only in the answer of `storage_save` (spec 050 FR-003).
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<StorageWarning>,
}

/// What `storage_save` warns about without refusing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub enum StorageWarning {
    /// Another storage of the connection has its folder inside or around this one.
    NestedFolder,
}

/// `storage_list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct StorageOverview {
    pub connections: Vec<ConnectionView>,
    pub storages: Vec<StorageView>,
}

/// `storage_test`: the outcome, and the key of a test object holzi could not delete (SC-004).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct TestResult {
    pub outcome: TestOutcome,
    pub leftover_key: Option<String>,
    /// The key an SFTP server showed at `hostKeyUnknown` or `hostKeyChanged`.
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host_key: Option<ShownHostKey>,
}

/// `storage_removal_preview`: one of the two ids.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct RemovalTarget {
    #[ts(optional)]
    #[serde(default)]
    pub connection_id: Option<String>,
    #[ts(optional)]
    #[serde(default)]
    pub storage_id: Option<String>,
}

/// What a removal takes with it (FR-007): the storages and the extensions that lose access.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct RemovalPreview {
    pub storages: Vec<String>,
    pub extensions: Vec<String>,
}
