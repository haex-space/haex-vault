// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

use super::super::provider::SyncProviderError;

#[derive(Debug, thiserror::Error)]
pub enum SyncEngineError {
    #[error("Provider error: {0}")]
    Provider(#[from] SyncProviderError),

    /// Source manifest could not be fetched (peer offline, network down,
    /// cloud bucket unreachable, …). Treated as a transient condition: the
    /// loop keeps retrying with exponential backoff and never auto-pauses
    /// the rule. Sync simply resumes when the source becomes reachable.
    #[error("Source unavailable: {0}")]
    SourceUnavailable(SyncProviderError),

    /// Target manifest could not be fetched. Same semantics as
    /// `SourceUnavailable` — the target may equally be a phone, peer, or
    /// cloud bucket that goes offline temporarily, so the loop retries
    /// indefinitely with backoff rather than disabling the rule.
    #[error("Target unavailable: {0}")]
    TargetUnavailable(SyncProviderError),

    #[error("Database error: {0}")]
    Database(String),

    #[error("Cancelled")]
    Cancelled,
}

impl serde::Serialize for SyncEngineError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}
