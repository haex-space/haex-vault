//! Why the agent browser could not do something (spec 065, contracts/browser-tools.md "Fehler",
//! 021 contracts/mcp-surface.md). The codes are what an agent sees; the text is for the log and
//! the settings page.

use serde::Serialize;

/// An error of the agent browser.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BrowserError {
    /// No usable browser on this device: none found, too old, or one that cannot be steered.
    #[error("no agent browser on this device: {0}")]
    NotFound(String),
    /// The browser did not start, did not answer in time, or ended.
    #[error("the agent browser is not available: {0}")]
    Unavailable(String),
    /// The browser answered a call with an error.
    #[error("the agent browser refused the call: {0}")]
    Protocol(String),
    /// This device has no agent browser (Android, iOS; research R13).
    #[error("no agent browser on this device")]
    Unsupported,
    /// The vault is closed or closing.
    #[error("the vault is not open")]
    VaultClosed,
    /// Reading or writing the profile failed.
    #[error("the profile of the agent browser: {0}")]
    Profile(String),
}

impl BrowserError {
    /// The code an agent sees: `browser_unavailable` of 065, `unavailable` and `failed` of 021.
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotFound(_) | Self::Unavailable(_) | Self::Unsupported | Self::Profile(_) => {
                "browser_unavailable"
            }
            Self::VaultClosed => "unavailable",
            Self::Protocol(_) => "failed",
        }
    }
}

impl Serialize for BrowserError {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

pub type Result<T> = std::result::Result<T, BrowserError>;
