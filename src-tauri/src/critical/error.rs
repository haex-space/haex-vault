//! Error type returned by [`crate::critical::lock_or_fail`].

use super::CriticalFailureCode;

/// Returned when a mutex was found poisoned. Carries the
/// [`CriticalFailureCode`] discriminator and the source location of the
/// failing `lock_or_fail` call so propagation chains stay diagnosable.
///
/// `Copy` because both fields are `Copy` and the struct is meant to flow
/// up through error chains without cloning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MutexPoisonError {
    pub code: CriticalFailureCode,
    pub location: &'static str,
}

impl std::fmt::Display for MutexPoisonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "mutex poisoned at {} (code={:?})", self.location, self.code)
    }
}

impl std::error::Error for MutexPoisonError {}

/// Convert a [`MutexPoisonError`] into the existing `DatabaseError`
/// shape so callers that previously wrote
/// `.lock().map_err(|_| DatabaseError::MutexPoisoned { reason: ... })?`
/// can migrate to a single `state.lock_or_fail(...)?` call. The Display
/// of `MutexPoisonError` produces "mutex poisoned at <location> (code=
/// <code>)", which is more informative than the original ad-hoc reason
/// strings.
impl From<MutexPoisonError> for crate::database::error::DatabaseError {
    fn from(err: MutexPoisonError) -> Self {
        Self::MutexPoisoned {
            reason: err.to_string(),
        }
    }
}

/// Same shape conversion for `ExtensionError`, used by the
/// extension/* call sites that wrap mutex poison in their own error
/// type before propagating to the frontend.
impl From<MutexPoisonError> for crate::extension::error::ExtensionError {
    fn from(err: MutexPoisonError) -> Self {
        Self::MutexPoisoned {
            reason: err.to_string(),
        }
    }
}

/// Conversion for `DeliveryError` (space_delivery/local) — sites in
/// `commands.rs`, `invite_tokens.rs`, `leader.rs`, `push_invite.rs`
/// previously wrote `state.hlc.lock().map_err(|_| DeliveryError::Database
/// { reason: ... })?` and now use `state.lock_or_fail(...)?`.
impl From<MutexPoisonError> for crate::space_delivery::local::error::DeliveryError {
    fn from(err: MutexPoisonError) -> Self {
        Self::Database {
            reason: err.to_string(),
        }
    }
}

/// Conversion for `DeviceError` (device/*) — `device::mod` HLC lock
/// sites that previously mapped to `DeviceError::Database`.
impl From<MutexPoisonError> for crate::device::error::DeviceError {
    fn from(err: MutexPoisonError) -> Self {
        Self::Database {
            reason: err.to_string(),
        }
    }
}

/// Conversion for `StorageError` (remote_storage/*). Maps to the
/// `Internal` variant rather than `DatabaseError`, matching the existing
/// call-site convention in `remote_storage::commands` which treats
/// HLC-lock failure as an internal-state error.
impl From<MutexPoisonError> for crate::remote_storage::error::StorageError {
    fn from(err: MutexPoisonError) -> Self {
        Self::Internal {
            reason: err.to_string(),
        }
    }
}

/// Conversion for `PeerStorageError` — `peer_storage::commands` HLC
/// lock sites that previously mapped to `PeerStorageError::Database`.
impl From<MutexPoisonError> for crate::peer_storage::error::PeerStorageError {
    fn from(err: MutexPoisonError) -> Self {
        Self::Database {
            reason: err.to_string(),
        }
    }
}
