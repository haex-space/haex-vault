//! Current MLS epoch-key resolution for shared credential fanout.

use crate::file_sync::crypto::key_resolver::{self, KeyError};

/// Resolver seam used by the share flow and its failure-path tests.
pub trait EpochResolver: Send + Sync {
    fn resolve_latest(
        &self,
        db: &crate::database::DbConnection,
        space_id: &str,
    ) -> Result<(u64, [u8; 32]), KeyError>;
}

/// Production resolver backed by the current MLS group and key history.
pub struct DefaultEpochResolver;

impl EpochResolver for DefaultEpochResolver {
    fn resolve_latest(
        &self,
        db: &crate::database::DbConnection,
        space_id: &str,
    ) -> Result<(u64, [u8; 32]), KeyError> {
        key_resolver::resolve_latest(space_id, db)
    }
}
