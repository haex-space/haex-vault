//! Provider adapters — one implementation per provider kind.
//!
//! The `ProviderAdapter` trait is what the refresh and chat paths call
//! against. Both `list_models` (Etappe-2 refresh path) and `stream_chat`
//! (Etappe-3 chat dispatch) go through the trait, so `ChatState` sees a
//! single `Arc<dyn ProviderAdapter>` regardless of whether the model
//! runs in-process (`LocalAdapter`) or over HTTP (`AnthropicAdapter`).
//! `openai_compatible` covers every server that speaks the chat-completions format (OpenAI,
//! Gemini's compatibility endpoint, Ollama, vLLM, a school server); other `api_key` vendors
//! would land as their own adapters using the same trait surface. `cli_delegate` adapters
//! (Claude Code, Codex — spec 007-cli-delegate) live in [`cli_delegate`];
//! they drive an external CLI subprocess to completion per `stream_chat`
//! call instead of a direct HTTP/in-process request, but otherwise
//! implement the same trait.

pub mod anthropic;
mod anthropic_capabilities;
#[cfg(test)]
mod anthropic_stream_tests;
#[cfg(test)]
mod anthropic_tests;
pub mod cli_delegate;
#[cfg(feature = "llm-cpu")]
pub mod local;
pub mod openai_compatible;
#[cfg(test)]
mod openai_compatible_tests;
mod openai_request;
#[cfg(test)]
mod openai_request_tests;
pub mod request;
#[cfg(test)]
mod request_tests;
pub mod tls;
pub mod types;

pub use types::{
    AbortHandle, AdapterStream, Attachment, AttachmentKind, ChatMessage, ChatRequest, ChatRole,
    StreamChunk, StreamError, ToolTemplateProbe,
};

use async_trait::async_trait;

use crate::model_capabilities::ModelCapabilities;

/// One model as reported by a provider. Provider-agnostic subset of
/// what a listing endpoint returns; storage enrichment (composite id,
/// fetched_at) lives adjacent in `providers::mod` so this stays free of
/// persistence concerns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderModel {
    /// Remote id as returned by the provider (e.g. `"claude-opus-5"`).
    /// The persisted `models.id` composes this with the provider UUID
    /// per plan §"Datenmodell" (`<provider_id>:<remote_model_id>`).
    pub remote_id: String,
    /// Human-readable name (e.g. `"Claude Opus 5"`) that appears in
    /// the model picker.
    pub display_name: String,
    /// Maximum input context in tokens, if reported. `None` when the
    /// provider does not surface it. Persisted into
    /// `models.context_window`.
    pub context_window: Option<i64>,
    /// What the provider says this model supports. The default record
    /// (`ModelCapabilities::default()`) means everything is not determined
    /// — an adapter that cannot tell says so rather than claiming
    /// "unsupported". Persisted into `models.capabilities_json`.
    pub capabilities: ModelCapabilities,
}

/// Errors an adapter can return from the pre-stream request path. All
/// variants are terminal — the caller must not retry, especially on
/// 4xx. Plan §"Anbietermodelle" requires that "Ungültige Zugangsdaten
/// führen zu einer verständlichen Meldung": `InvalidCredentials` is
/// mapped from 401/403 explicitly so the frontend can render it as a
/// targeted message instead of a generic HTTP error. Cache updates
/// only run on `Ok(_)`; a failed refresh leaves the previous cache
/// intact.
///
/// Errors that occur mid-stream (after `stream_chat` returns Ok) are
/// delivered on the [`AdapterStream`] as [`StreamError`] instead.
#[derive(Debug, thiserror::Error)]
pub enum AdapterError {
    #[error("http transport failure: {reason}")]
    Http { reason: String },
    #[error("provider returned status {status}: {body}")]
    Status { status: u16, body: String },
    #[error("invalid credentials")]
    InvalidCredentials,
    #[error("failed to parse provider response: {reason}")]
    Parse { reason: String },
    /// The backend itself isn't usable right now — distinct from
    /// `InvalidCredentials` (spec 007-cli-delegate FR-008/SC-006): a
    /// `cli_delegate` binary that isn't installed/on `PATH`, for
    /// example. Reproduces deterministically for the same host state,
    /// so never worth retrying.
    #[error("backend unavailable: {reason}")]
    Unavailable { reason: String },
    /// The provider's certificate is not trusted by the system (spec 043 FR-024): a self-signed
    /// or otherwise unknown certificate. The same on every attempt, so never retried.
    #[error("certificate not trusted: {reason}")]
    UntrustedCertificate { reason: String },
}

/// A failure to reach the provider: an untrusted certificate is told apart, everything else is a
/// transport failure. `context` names the request (`POST <endpoint>`).
pub fn transport_error(context: String, error: reqwest::Error) -> AdapterError {
    let reason = format!("{context}: {error}");
    if tls::is_untrusted_certificate(&error) {
        AdapterError::UntrustedCertificate { reason }
    } else {
        AdapterError::Http { reason }
    }
}

impl AdapterError {
    /// Eligible for the bounded automatic retry when re-starting a step
    /// mid-turn after a transient [`StreamError`](crate::adapters::StreamError)
    /// (spec.md FR-012, tasks.md T034): a transport failure, a rate limit
    /// (429), or a 5xx. `InvalidCredentials`/`Parse`/`Unavailable` and
    /// other 4xx statuses reproduce deterministically for the same
    /// request.
    pub fn is_transient(&self) -> bool {
        match self {
            AdapterError::Http { .. } => true,
            AdapterError::Status { status, .. } => *status == 429 || (500..600).contains(status),
            AdapterError::InvalidCredentials
            | AdapterError::Parse { .. }
            | AdapterError::Unavailable { .. }
            | AdapterError::UntrustedCertificate { .. } => false,
        }
    }
}

/// Contract every provider adapter implements. Both the listing and the
/// streaming paths funnel through this trait so `ChatState` can hold
/// `Arc<dyn ProviderAdapter>` without knowing which vendor sits behind
/// it.
#[async_trait]
pub trait ProviderAdapter: Send + Sync {
    /// Fetches the provider's current model listing. Must paginate
    /// internally if the provider chunks the response so callers
    /// receive a complete list. On failure the caller's cache is
    /// preserved. `LocalAdapter` returns an empty `Vec` — local models
    /// enter the catalog via the download / import commands, not
    /// through refresh.
    async fn list_models(&self) -> Result<Vec<ProviderModel>, AdapterError>;

    /// Starts a streaming chat generation. Returns immediately with an
    /// [`AdapterStream`]; failures that surface before the first byte
    /// (transport, non-2xx, credential rejection) come back through
    /// [`AdapterError`], while failures mid-stream ride the stream's
    /// `Err` branch as [`StreamError`].
    async fn stream_chat(&self, req: ChatRequest) -> Result<AdapterStream, AdapterError>;

    /// Asks the adapter whether its model's chat template takes tools at all (spec 032 R8). Only
    /// a local model has a template to look at; every other adapter cannot tell.
    async fn probe_tool_template(&self) -> ToolTemplateProbe {
        ToolTemplateProbe::Inconclusive
    }
}
