//! The vault passphrase, held so that it is erased when dropped and never printed.
//!
//! The passphrase is passed verbatim as the SQLCipher key, so it is the most sensitive value in
//! the app (spec 013, FR-013 to FR-016). This type is the only way argument structs carry it: it
//! erases its buffer on drop, has no `Clone` so no copy is made by accident, and prints as
//! `<redacted>` in every `Debug` output, so it cannot reach a log line through a derived `Debug`.
//! Copies that the frontend runtime, the IPC layer and SQLCipher itself hold are outside what this
//! type can wipe; the process ending on close is the guarantee for those.

use serde::Deserialize;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use crate::error::{HaexError, Result};

/// Minimum passphrase length in bytes. Kept low for the MVP; can be tightened
/// later without a wire-contract change.
pub(crate) const MIN_PASSPHRASE_LEN: usize = 8;

/// A vault passphrase that erases its buffer on drop, has no `Clone` and never prints its value.
#[derive(Deserialize)]
#[serde(transparent)]
pub struct Passphrase(Zeroizing<String>);

impl Passphrase {
    /// The passphrase text, for the one place that hands it to SQLCipher.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Checks a passphrase that is about to become a vault's key: on creation, on joining a link
    /// and on a change (spec 042).
    pub fn validate_new(&self) -> Result<()> {
        if self.0.len() < MIN_PASSPHRASE_LEN {
            return Err(HaexError::WeakPassphrase {
                reason: format!("passphrase must be at least {MIN_PASSPHRASE_LEN} characters"),
            });
        }
        Ok(())
    }
}

impl std::fmt::Debug for Passphrase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("<redacted>")
    }
}

impl From<&str> for Passphrase {
    fn from(value: &str) -> Self {
        Self(Zeroizing::new(value.to_owned()))
    }
}

impl From<String> for Passphrase {
    fn from(value: String) -> Self {
        Self(Zeroizing::new(value))
    }
}

impl Zeroize for Passphrase {
    fn zeroize(&mut self) {
        self.0.zeroize();
    }
}

// The inner `Zeroizing<String>` erases the buffer when this value drops.
impl ZeroizeOnDrop for Passphrase {}

#[cfg(test)]
#[path = "passphrase_tests.rs"]
mod passphrase_tests;
