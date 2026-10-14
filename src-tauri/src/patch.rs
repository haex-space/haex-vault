//! A field in a partial update, shared by the services that take partial updates (password
//! manager, spec 034; address book, spec 051).

use std::fmt;

use serde::{Deserialize, Deserializer};

/// A field in a partial update: absent keeps the stored value, `null` clears it, a value replaces
/// it (contracts/tauri-commands.md `passwords_update_item`).
#[derive(Clone, PartialEq, Eq)]
pub enum Patch<T> {
    Keep,
    Clear,
    Set(T),
}

// Derived `Default` would require `T: Default`, which a `Keep` does not need.
#[allow(clippy::derivable_impls)]
impl<T> Default for Patch<T> {
    fn default() -> Self {
        Patch::Keep
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Patch<T> {
    /// Only reached when the field is present (an absent field takes `Default`): `null` clears.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(match Option::<T>::deserialize(deserializer)? {
            Some(value) => Patch::Set(value),
            None => Patch::Clear,
        })
    }
}

impl<T> fmt::Debug for Patch<T> {
    /// Never prints the value: a patch may carry a secret.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Patch::Keep => "Keep",
            Patch::Clear => "Clear",
            Patch::Set(_) => "Set(<redacted>)",
        })
    }
}
