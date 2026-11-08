//! Provider-agnostic chat types shared by every adapter.
//!
//! The local llama.cpp path and the remote `api_key` path both funnel
//! their generated tokens through the same [`AdapterStream`] so the chat
//! dispatch in `chat/commands.rs` does not special-case where the tokens
//! come from.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::cli_delegate::autonomy::AutonomyMode;
use crate::model_capabilities::ModelCapabilities;

/// Which speaker a message belongs to. `System` is passed separately in
/// [`ChatRequest::system_prompt`] because both local and Anthropic adapters
/// treat it as an out-of-band field rather than a message role.
///
/// `ToolCall`/`ToolResult` carry their own payload directly (data-model.md)
/// rather than reusing [`ChatMessage::content`] — each is its own flat
/// entry in [`ChatRequest::messages`]; an adapter groups consecutive
/// entries of the same kind into one wire-level message (research.md §1/§2,
/// data-model.md's two-call example).
#[derive(Debug, Clone, PartialEq)]
pub enum ChatRole {
    User,
    Assistant,
    ToolCall {
        id: String,
        name: String,
        input: Value,
    },
    ToolResult {
        call_id: String,
        content: String,
        is_error: bool,
    },
}

/// A file the user attached to a message (spec 011-composer-toolbar-parity).
/// Built by `chat::attachments::read_attachment_content` from a path the
/// frontend picked; never persisted (spec.md Out of scope) — populated only
/// on the current turn's user message, `content` and `media_type` already
/// resolved so adapters never need to touch the filesystem themselves.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttachmentKind {
    Image,
    Document,
    Text,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Attachment {
    pub name: String,
    pub kind: AttachmentKind,
    /// e.g. `"image/png"`, `"application/pdf"`, `"text/plain"`.
    pub media_type: String,
    pub bytes: Vec<u8>,
}

/// `content` is only meaningful for `role: ChatRole::User | Assistant` —
/// `ToolCall`/`ToolResult` carry their payload on the role variant itself
/// and leave this empty. `attachments` is only ever populated on the
/// current turn's `User` message (spec 011-composer-toolbar-parity);
/// historical rows always carry an empty list.
#[derive(Debug, Clone, PartialEq)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
    pub attachments: Vec<Attachment>,
}

/// A tool an adapter may offer the model this step. Reused as-is for
/// Anthropic's `input_schema` and the local template's tool schema — both
/// are JSON-Schema-shaped (research.md §1/§2).
#[derive(Debug, Clone, PartialEq)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

/// One tool call the model produced. Reconstructed by an adapter from its
/// own streamed wire format (buffered `input_json_delta`s for Anthropic,
/// llama.cpp's parsed tool blocks for local models) into this
/// provider-neutral shape.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub input: Value,
}

/// Complete chat request handed to an adapter. `model_id` is the raw
/// remote id (e.g. `"claude-opus-5"`); the composite `<provider>:<remote>`
/// split happens above the adapter so each adapter only sees the id its
/// provider expects. Local adapters ignore this field — their model is
/// bound at construction.
#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub model_id: String,
    /// The holzi conversation id used for delegate approval events.
    pub thread_id: Option<uuid::Uuid>,
    pub system_prompt: Option<String>,
    pub messages: Vec<ChatMessage>,
    /// Whether the selected model's cached capabilities say it reasons
    /// (user-selectable or model-managed). Adapters enable their
    /// provider-specific reasoning mode only when this is true; the
    /// frontend never sets this flag directly.
    pub reasoning_requested: bool,
    /// Cap on completion tokens. `None` uses each adapter's default;
    /// Anthropic Messages API requires the field, so the Anthropic
    /// adapter substitutes a conservative default in that case.
    pub max_new_tokens: Option<usize>,
    /// Tools offered this step. Empty when no tools are registered, or
    /// when the loaded local model's chat template does not support tool
    /// calling (research.md §2 caveat) — an adapter never errors on an
    /// empty list, it just never emits `StreamChunk::ToolCalls`.
    pub tools: Vec<ToolSpec>,
    /// Per-request autonomy posture for `cli_delegate` adapters (spec
    /// 009-autonomous-delegate-mode); every other adapter receives it like
    /// any other field but never reads it. Defaults to `Standard`, which is
    /// byte-for-byte today's shipped behavior.
    pub autonomy_mode: AutonomyMode,
    /// What `GatedPermissive` still refuses: the deny rules of the agent profile of the model
    /// (spec 083 FR-030); empty for any other posture.
    pub autonomy_deny: Vec<crate::adapters::cli_delegate::autonomy::DenyCategory>,
    /// The user's selected reasoning option as a provider-native id, already
    /// validated by `send_message` against this model's cached options
    /// (spec 012 FR-013). `None` means "no override" — the direct-API
    /// adapter omits `output_config.effort` (API default applies) and the
    /// Claude Code delegate omits `--effort` (CLI default applies).
    /// Independent of `reasoning_requested`/`max_new_tokens` and of the
    /// local adapter, which never reads this field.
    pub reasoning_option: Option<String>,
    /// The selected model's cached capabilities, resolved once by
    /// `send_message` so no adapter re-derives them from the model id.
    /// `None` when the model has no row or nothing is determined.
    pub capabilities: Option<ModelCapabilities>,
    /// How the model picks its next token. `Default` is what every chat uses; `Deterministic`
    /// is for the evaluation of tool use (spec 032 FR-022), which compares runs of one model.
    /// Only the local adapter can honor it; every other adapter ignores it.
    pub sampling: Sampling,
}

/// What the chat template of a local model says about tools (spec 032 R8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolTemplateProbe {
    /// The template renders the same text with and without a tool, or refuses tools: this model
    /// cannot be shown any.
    IgnoresTools,
    /// The template takes tools, or nothing could be told. Whether the model uses them well is
    /// for the self-test.
    Inconclusive,
}

/// Sampling mode of one request (see [`ChatRequest::sampling`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sampling {
    /// The adapter's or the model's own defaults.
    #[default]
    Default,
    /// Greedy decoding, so two runs of one model give the same answers.
    Deterministic,
}

/// One event on an [`AdapterStream`]. `Delta` carries either content,
/// reasoning, or both; `ToolCalls` precedes `Done` when the model stopped
/// specifically to call one or more tools; `Done` closes the stream with
/// token counts and timing.
#[derive(Debug, Clone)]
pub enum StreamChunk {
    Delta {
        content: String,
        /// Chain-of-thought text from Harmony-format local models or
        /// Anthropic `thinking_delta` events. `None` for adapters that
        /// do not surface reasoning.
        reasoning: Option<String>,
    },
    ToolCalls(Vec<ToolCall>),
    /// Live sub-agent activity for a Claude Code delegate response (spec
    /// 011-composer-toolbar-parity) — only ever produced by that adapter;
    /// every other adapter's `StreamChunk` stream simply never contains
    /// this variant. `batch_size` is set only on the chunk where a new
    /// batch of that many sub-agents was just confirmed dispatched.
    AgentActivity {
        active_count: usize,
        batch_size: Option<usize>,
    },
    Done {
        finish_reason: Option<String>,
        prompt_tokens: Option<usize>,
        completion_tokens: Option<usize>,
        /// Time to first non-empty content chunk. `None` when no
        /// content arrived (empty completion or immediate error).
        ttft_ms: Option<u64>,
        total_ms: u64,
    },
}

/// Errors delivered on the stream's `Err` branch. Not all are terminal —
/// see [`StreamError::is_transient`] (spec.md FR-012, tasks.md T034).
#[derive(Debug, thiserror::Error, Clone)]
pub enum StreamError {
    #[error("validation error: {0}")]
    Validation(String),
    #[error("model error: {0}")]
    Model(String),
    #[error("internal error: {0}")]
    Internal(String),
    #[error("stream ended without a Done frame")]
    UnexpectedEnd,
    #[error("failed to start generation: {0}")]
    StartFailed(String),
    /// A provider-reported condition classified transient at the source
    /// (e.g. Anthropic's `overloaded_error`/`rate_limit_error`/`api_error`
    /// SSE `error` events) — distinct from `Model`, which is a
    /// deterministic-for-this-request provider error.
    #[error("transient provider error: {0}")]
    Transient(String),
}

impl StreamError {
    /// Eligible for the bounded automatic retry (spec.md FR-012): a
    /// transport-level hiccup (`Internal` — e.g. an SSE decode failure from
    /// a dropped connection; `UnexpectedEnd` — the stream closed before a
    /// `Done` frame) or a provider-reported transient condition
    /// (`Transient`). `Model`/`Validation`/`StartFailed` reproduce
    /// deterministically for the same request, so retrying would not help.
    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            StreamError::Transient(_) | StreamError::Internal(_) | StreamError::UnexpectedEnd
        )
    }
}

/// A live generation. Consume via [`next`](Self::next); cancel by
/// dropping the value or invoking the [`abort_handle`](Self::abort_handle)
/// out of band. Dropping aborts the underlying task, so callers who need
/// to hand the abort out to shared state should clone the handle first.
#[derive(Clone)]
pub struct AbortHandle {
    task: tokio::task::AbortHandle,
    cancellation: Option<CancellationToken>,
}

impl AbortHandle {
    pub fn abort(&self) {
        if let Some(cancellation) = &self.cancellation {
            cancellation.cancel();
        } else {
            self.task.abort();
        }
    }
}

/// A stream of chunks from an adapter-owned generation task.
pub struct AdapterStream {
    rx: mpsc::UnboundedReceiver<Result<StreamChunk, StreamError>>,
    abort: AbortHandle,
}

impl AdapterStream {
    /// Constructs from a sender/receiver pair. Adapters spawn a task
    /// that writes to the sender and pass its abort handle here.
    pub fn new(
        rx: mpsc::UnboundedReceiver<Result<StreamChunk, StreamError>>,
        abort: tokio::task::AbortHandle,
    ) -> Self {
        Self {
            rx,
            abort: AbortHandle {
                task: abort,
                cancellation: None,
            },
        }
    }

    /// Constructs a stream whose process-owning task cooperatively cleans up
    /// before it exits. This is used by delegate subprocesses so cancellation
    /// can terminate and reap their process group instead of aborting cleanup.
    pub fn new_with_cancellation(
        rx: mpsc::UnboundedReceiver<Result<StreamChunk, StreamError>>,
        abort: tokio::task::AbortHandle,
        cancellation: CancellationToken,
    ) -> Self {
        Self {
            rx,
            abort: AbortHandle {
                task: abort,
                cancellation: Some(cancellation),
            },
        }
    }

    /// Await the next chunk. Returns `None` after a `Done` frame or an
    /// error has been delivered.
    pub async fn next(&mut self) -> Option<Result<StreamChunk, StreamError>> {
        self.rx.recv().await
    }

    /// Cloneable handle for out-of-band cancellation. Chat state stashes
    /// this so `abort_current_generation` can cancel without owning the
    /// full `AdapterStream`.
    pub fn abort_handle(&self) -> AbortHandle {
        self.abort.clone()
    }
}

impl Drop for AdapterStream {
    fn drop(&mut self) {
        // Dropping the receiver also closes the channel, which lets the
        // spawned task exit on its next `send` — but on CUDA a single
        // decode step can take tens of ms, so aborting is quicker.
        self.abort.abort();
    }
}
