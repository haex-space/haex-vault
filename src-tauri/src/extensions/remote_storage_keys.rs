//! An extension's area in a bucket (spec 038 FR-010, FR-011, SC-003, research R4, R5).
//!
//! Every key of an extension lies under `holzi-ext/<vault_id>/<extension_id>/`: the vault id is
//! derived from the vault's identity ([`crate::extensions::ids::storage_vault_id`]), the extension
//! id from publisher key and name, so both are the same on every own device and after a new
//! install, and two vaults on one bucket never share an area. A development version gets
//! `holzi-ext-dev/<vault_id>/<dev_extension_id>/`: its manifest is not signed and could name an
//! installed extension's key and name, so it never reaches that extension's objects.
//!
//! Keys and list prefixes are checked before every call; keys of the provider's answers outside
//! the area are dropped (a second guard).

use uuid::Uuid;

use super::bridge::dispatch::CallContext;
use super::bridge::frames::FrameSource;
use super::error::{BridgeError, ExtensionErrorCode};
use super::ids::storage_vault_id;

/// The longest key S3 stores, prefix included.
pub const MAX_KEY_BYTES: usize = 1024;

/// The longest name of one folder or file on a server with real folders (spec 050 research R9).
const MAX_PART_BYTES: usize = 255;

/// Characters common file systems refuse in a name (spec 050 research R9).
const REFUSED_BY_FILE_SYSTEMS: [char; 7] = [':', '*', '?', '"', '<', '>', '|'];

/// The area of one extension in every bucket.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Area {
    prefix: String,
}

fn invalid(what: &str) -> BridgeError {
    BridgeError::new(ExtensionErrorCode::Validation, format!("invalid {what}"))
}

/// The rules of research R5. A list prefix may be empty and may end in `/`. Paths in encrypted
/// folders of extensions follow them too (spec 048, contracts/bridge.md).
pub(super) fn valid(text: &str, list: bool) -> bool {
    if text.is_empty() {
        return list;
    }
    if text.starts_with('/')
        || text.contains('\\')
        || text.chars().any(|c| c < '\u{20}' || c == '\u{7f}')
    {
        return false;
    }
    let body = if list {
        text.strip_suffix('/').unwrap_or(text)
    } else {
        text
    };
    !body.is_empty()
        && body
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

/// Whether a key or list prefix that passed [`valid`] can be stored on a server with real folders
/// (WebDAV, SFTP; spec 050 research R9): no part longer than 255 bytes, none with a character
/// common file systems refuse, none named like holzi's temporary files.
pub(super) fn fits_tree(text: &str) -> bool {
    text.split('/').all(|part| {
        part.len() <= MAX_PART_BYTES
            && !part.contains(REFUSED_BY_FILE_SYSTEMS)
            && !part.starts_with(crate::remote_storage::PART_PREFIX)
    })
}

impl Area {
    /// The area of `extension_id` in the vault `vault_id`; `dev` for a development version.
    pub fn new(vault_id: Uuid, extension_id: Uuid, dev: bool) -> Self {
        let root = if dev { "holzi-ext-dev" } else { "holzi-ext" };
        Self {
            prefix: format!("{root}/{vault_id}/{extension_id}/"),
        }
    }

    /// The area of the calling frame's extension: who calls comes from the session, never from
    /// the extension.
    pub fn of(ctx: &CallContext) -> Result<Self, BridgeError> {
        let pubkey = ctx
            .db
            .read_blocking(|q| crate::sync::keys::vault_pubkey(q))
            .map_err(|_| BridgeError::new(ExtensionErrorCode::Database, "database unavailable"))?
            .ok_or_else(BridgeError::not_available)?;
        Ok(Self::new(
            storage_vault_id(&pubkey),
            ctx.session.extension_id,
            ctx.session.source == FrameSource::DevServer,
        ))
    }

    pub fn prefix(&self) -> &str {
        &self.prefix
    }

    /// The full key of the extension's `key`, or 3001.
    pub fn key(&self, key: &str) -> Result<String, BridgeError> {
        if !valid(key, false) || self.prefix.len() + key.len() > MAX_KEY_BYTES {
            return Err(invalid("key"));
        }
        Ok(format!("{}{key}", self.prefix))
    }

    /// The full prefix of a listing under the extension's `prefix` (empty: the whole area).
    pub fn list_prefix(&self, prefix: &str) -> Result<String, BridgeError> {
        if !valid(prefix, true) || self.prefix.len() + prefix.len() > MAX_KEY_BYTES {
            return Err(invalid("prefix"));
        }
        Ok(format!("{}{prefix}", self.prefix))
    }

    /// The extension's key of a provider's `full` key; `None` outside the area.
    pub fn strip<'a>(&self, full: &'a str) -> Option<&'a str> {
        full.strip_prefix(self.prefix.as_str())
            .filter(|key| !key.is_empty())
    }
}

#[cfg(test)]
#[path = "remote_storage_keys_tests.rs"]
mod tests;
