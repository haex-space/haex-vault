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
