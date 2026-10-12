//! Kinds, actions, states and scopes of extension permissions (contracts/permissions.md).

use uuid::Uuid;

use super::target::{RequestTarget, Target};
use crate::identity::VAULT_SCOPE_UUID;

/// The `vault_device_uuid` of a permission that holds on every own device (ADR-0001 sentinel).
pub const VAULT_WIDE: Uuid = VAULT_SCOPE_UUID;

/// What a permission is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PermissionKind {
    Database,
    Filesystem,
    Web,
    Notifications,
    Passwords,
    RemoteStorage,
    Mail,
    Shell,
    /// One encrypted folder of a storage (spec 048 FR-036): granted only by the user when asked,
    /// never from a manifest (FR-037).
    EncryptedFolder,
}

impl PermissionKind {
    pub const ALL: [PermissionKind; 9] = [
        Self::Database,
        Self::Filesystem,
        Self::Web,
        Self::Notifications,
        Self::Passwords,
        Self::RemoteStorage,
        Self::Mail,
        Self::Shell,
        Self::EncryptedFolder,
    ];

    /// The stored name; anything else is not a kind holzi knows (FR-022).
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == value)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Database => "database",
            Self::Filesystem => "filesystem",
            Self::Web => "web",
            Self::Notifications => "notifications",
            Self::Passwords => "passwords",
            Self::RemoteStorage => "remoteStorage",
            Self::Mail => "mail",
            Self::Shell => "shell",
            Self::EncryptedFolder => "encryptedFolder",
        }
    }

    /// Kinds whose remembered permissions hold only on the device that granted them
    /// (Clarifications 2026-10-06): programs differ per device. Every other kind, the file system
    /// included, holds on every own device; nobody chooses.
    pub fn is_device_scoped(self) -> bool {
        matches!(self, Self::Shell)
    }

    /// The `vault_device_uuid` a remembered permission of this kind is written with on `device`.
    pub fn scope_on(self, device: Uuid) -> Uuid {
        if self.is_device_scoped() {
            device
        } else {
            VAULT_WIDE
        }
    }

    /// Whether a remembered permission of this kind may have the scope `vault_device_uuid`: a
    /// device for a device-scoped kind, vault-wide for every other.
    pub fn fits(self, vault_device_uuid: Uuid) -> bool {
        self.is_device_scoped() == (vault_device_uuid != VAULT_WIDE)
    }
}

/// What the extension wants to do.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Action {
    Read,
    ReadWrite,
    /// One HTTP method (upper case, a token such as `GET` or `PROPFIND`).
    Method(String),
    /// Every HTTP method.
    AnyMethod,
    Show,
    Fetch,
    Send,
    Poll,
    Execute,
    /// Propose a new storage endpoint on a host (spec 038 FR-009b); `readWrite` does not cover it.
    Add,
}

impl Action {
    /// Parses the stored action of `kind`; an action the kind does not have is absent (FR-022).
    pub fn parse(kind: PermissionKind, value: &str) -> Option<Self> {
        use PermissionKind as K;
        match (kind, value) {
            (
                K::Database | K::Filesystem | K::Passwords | K::RemoteStorage | K::EncryptedFolder,
                "read",
            ) => Some(Self::Read),
            (
                K::Database | K::Filesystem | K::Passwords | K::RemoteStorage | K::EncryptedFolder,
                "readWrite",
            ) => Some(Self::ReadWrite),
            (K::Web, "*") => Some(Self::AnyMethod),
            (K::Web, method) if is_http_method(method) => Some(Self::Method(method.to_owned())),
            (K::Notifications, "show") => Some(Self::Show),
            (K::Mail, "fetch") => Some(Self::Fetch),
            (K::Mail, "send") => Some(Self::Send),
            (K::Mail, "poll") => Some(Self::Poll),
            (K::Shell, "execute") => Some(Self::Execute),
            (K::RemoteStorage, "add") => Some(Self::Add),
            _ => None,
        }
    }

    /// The stored form, the inverse of [`Action::parse`].
    pub fn as_string(&self) -> String {
        match self {
            Self::Read => "read".to_owned(),
            Self::ReadWrite => "readWrite".to_owned(),
            Self::Method(method) => method.clone(),
            Self::AnyMethod => "*".to_owned(),
            Self::Show => "show".to_owned(),
            Self::Fetch => "fetch".to_owned(),
            Self::Send => "send".to_owned(),
            Self::Poll => "poll".to_owned(),
            Self::Execute => "execute".to_owned(),
            Self::Add => "add".to_owned(),
        }
    }

    /// Whether a permission for `self` covers a request for `requested`: `readWrite` covers
    /// `read`, `*` covers every method, everything else only itself.
    pub fn covers(&self, requested: &Action) -> bool {
        match (self, requested) {
            (Self::ReadWrite, Self::Read) => true,
            (Self::AnyMethod, Self::Method(_)) => true,
            _ => self == requested,
        }
    }
}

/// An HTTP method token: 1 to 20 upper-case ASCII letters.
fn is_http_method(value: &str) -> bool {
    (1..=20).contains(&value.len()) && value.bytes().all(|b| b.is_ascii_uppercase())
}

/// The state of a permission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionStatus {
    Granted,
    Denied,
    Ask,
}

impl PermissionStatus {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "granted" => Some(Self::Granted),
            "denied" => Some(Self::Denied),
            "ask" => Some(Self::Ask),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Granted => "granted",
            Self::Denied => "denied",
            Self::Ask => "ask",
        }
    }
}

/// Where a remembered permission holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrantScope {
    /// On every own device.
    Vault,
    /// Only on this device (`vault_device_uuid`).
    Device(Uuid),
}

impl GrantScope {
    pub fn from_device_column(vault_device_uuid: Uuid) -> Self {
        if vault_device_uuid == VAULT_WIDE {
            Self::Vault
        } else {
            Self::Device(vault_device_uuid)
        }
    }

    pub fn device_column(self) -> Uuid {
        match self {
            Self::Vault => VAULT_WIDE,
            Self::Device(uuid) => uuid,
        }
    }

    /// Whether the permission holds on `device`.
    pub fn holds_on(self, device: Uuid) -> bool {
        match self {
            Self::Vault => true,
            Self::Device(uuid) => uuid == device,
        }
    }
}

/// One permission as stored or held for the session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Permission {
    pub kind: PermissionKind,
    pub action: Action,
    pub target: Target,
    pub status: PermissionStatus,
    pub scope: GrantScope,
}

impl Permission {
    /// Parses a stored row. A row with an unknown kind, action, target or status is **absent**,
    /// never read as another permission (FR-022).
    pub fn from_row(
        kind: &str,
        action: &str,
        target: &str,
        status: &str,
        vault_device_uuid: Uuid,
    ) -> Option<Self> {
        let kind = PermissionKind::parse(kind)?;
        Some(Self {
            kind,
            action: Action::parse(kind, action)?,
            target: Target::parse(kind, target)?,
            status: PermissionStatus::parse(status)?,
            scope: GrantScope::from_device_column(vault_device_uuid),
        })
    }

    /// Whether this permission is about `request`: same kind, an action that covers it and a
    /// target that matches it. The state is not looked at here.
    pub fn applies_to(&self, request: &PermissionRequest) -> bool {
        self.kind == request.kind
            && self.action.covers(&request.action)
            && self.target.matches(&request.target)
    }
}

/// What an extension asks for at one moment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionRequest {
    pub kind: PermissionKind,
    pub action: Action,
    pub target: RequestTarget,
}
