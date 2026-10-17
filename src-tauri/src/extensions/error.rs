//! Errors an extension receives over the bridge (spec 017, contracts/bridge.md §Fehlercodes).
//!
//! The codes are haex-vault 2.x's (Legacy) (`src-tauri/src/extension/error.rs` @ `8dce379`), so the vault-sdk
//! understands them unchanged — `1002` denied and `1004` prompt required drive its retry logic —
//! plus three of holzi's own. A message never names a table, file or entry outside the caller's
//! grants (FR-062): the caller learns that something is refused, not whether it exists.

use serde::Serialize;
use serde_json::Value;

/// The numeric error code of a bridge answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum ExtensionErrorCode {
    /// A form that cannot be attributed, a blocked path, an unknown frame.
    SecurityViolation = 1000,
    NotFound = 1001,
    /// `PermissionErrorCode.DENIED` of the vault-sdk.
    PermissionDenied = 1002,
    /// `PermissionErrorCode.PROMPT_REQUIRED` of the vault-sdk: it waits for
    /// `extension:permission-resolved` and retries.
    PermissionPromptRequired = 1004,
    Database = 2000,
    Filesystem = 2001,
    Http = 2002,
    Shell = 2003,
    Web = 2005,
    Manifest = 3000,
    Validation = 3001,
    /// An entry of an encrypted folder failed its checks (spec 048, contracts/bridge.md).
    Damaged = 3010,
    /// Rows, run time, size or concurrent requests over the limit (FR-031).
    LimitExceeded = 7000,
    /// The method is unknown or deliberately not offered (FR-060, FR-061).
    NotSupported = 8000,
    /// The function does not exist on this device or in this build (FR-053, FR-059, FR-066).
    NotAvailable = 8001,
    /// The extension is disabled (FR-007).
    Disabled = 8002,
}

impl ExtensionErrorCode {
    pub fn as_u16(self) -> u16 {
        self as u16
    }
}

impl Serialize for ExtensionErrorCode {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u16(self.as_u16())
    }
}

/// The `error` of a bridge answer: `{code, message, details?}` as the vault-sdk reads it.
#[derive(Debug, Clone, PartialEq, Serialize, thiserror::Error)]
#[error("{message} ({code:?})")]
pub struct BridgeError {
    pub code: ExtensionErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
    /// For a `1004` whose `details.target` is not what the user is asked about; never sent.
    #[serde(skip)]
    pub asks: Option<Box<Asks>>,
}

/// What the question behind a `1004` names when it is not `details.target`: the extension is
/// told the program as it named it, the user is asked about its canonical path (shell), so the
/// answer does not tell where a link points or whether a name is on `PATH`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Asks {
    Target(String),
    /// There is nothing a permission could name: no question, the SDK waits as for one nobody
    /// answers.
    Nothing,
    /// The question is about `details.target`, an encrypted folder, and holzi's window names it
    /// by these names, which the extension never gets (spec 048 FR-036, FR-039).
    Folder {
        storage_name: String,
        folder_name: String,
    },
}

impl BridgeError {
    pub fn new(code: ExtensionErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: None,
            asks: None,
        }
    }

    pub fn with_details(mut self, details: Value) -> Self {
        self.details = Some(details);
        self
    }

    pub fn asking(mut self, asks: Asks) -> Self {
        self.asks = Some(Box::new(asks));
        self
    }

    /// The answer for a method holzi does not offer.
    pub fn not_supported() -> Self {
        Self::new(ExtensionErrorCode::NotSupported, "not supported")
    }

    /// The answer for a function this device or build does not have.
    pub fn not_available() -> Self {
        Self::new(ExtensionErrorCode::NotAvailable, "not available")
    }

    /// `Ok` when this device has the function, else [`BridgeError::not_available`]; called with a
    /// field of [`crate::platform::capabilities`] (spec 043 FR-016).
    pub fn unless(available: bool) -> Result<(), Self> {
        if available {
            Ok(())
        } else {
            Err(Self::not_available())
        }
    }

    /// The answer for every request of a disabled extension.
    pub fn disabled() -> Self {
        Self::new(ExtensionErrorCode::Disabled, "extension disabled")
    }
}

#[cfg(test)]
#[path = "error_tests.rs"]
mod tests;
