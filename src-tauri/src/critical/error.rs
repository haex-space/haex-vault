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
