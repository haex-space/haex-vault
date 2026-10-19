//! The run of one step (spec 055 T027, T039, T043): the candidates are tried in order, each at
//! most once, until one gives an output the guard and the format accept. The request carries no
//! tools (FR-026); the instruction is the system prompt, the images are attachments and the schema
//! is a hint. The deadline of the job and its cancel token end the run at once: the stream is
//! dropped, which aborts the generation.

use serde::Serialize;
use serde_json::Value;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use super::candidates::Candidate;
use super::effort::{effective, Applied};
use super::errors::{sanitize, JobErrorCode};
use super::guard;
use super::job::Step;
use super::limits::MAX_RESULT_BYTES;
use super::model_source::ModelSource;
use super::place::Place;
use crate::adapters::types::{
    Attachment, AttachmentKind, ChatMessage, ChatRequest, ChatRole, Sampling, StreamChunk,
};
use crate::adapters::AdapterError;

/// The fixed sentence holzi adds after the input (`contracts/job-format.md` §Auftrag).
const FORMAT_SENTENCE: &str =
    "Answer with valid JSON only, without any other text, matching this JSON schema:";

/// How one try of a model ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Outcome {
    Used,
    Unusable,
    Unavailable,
    Timeout,
}

/// One try of a model, as the provenance lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct Attempt {
    pub model: String,
    pub place: Place,
    /// The effort actually sent: `none`, `managed`, `low`, `medium` or `high`.
    pub effort: String,
    pub outcome: Outcome,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// How a step ended.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum StepEnd {
    /// The output, checked against the format.
    Used(Value),
    /// No candidate gave a usable output; the code and message of the last reason.
    Failed(JobErrorCode, String),
    Cancelled,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct StepRun {
    pub attempts: Vec<Attempt>,
    pub end: StepEnd,
}

/// Why one try gave nothing.
enum Miss {
    Unusable(String),
    Unavailable(String),
}

/// The text the model reads besides the instruction: the input and the format.
fn user_text(step: &Step) -> String {
    let schema = step.format.schema.to_string();
    match &step.input.text {
        Some(text) => format!("{text}\n\n{FORMAT_SENTENCE}\n{schema}"),
        None => format!("{FORMAT_SENTENCE}\n{schema}"),
    }
}

/// The request for one candidate. No tools, ever (FR-026).
pub(crate) fn request(step: &Step, candidate: &Candidate, applied: &Applied) -> ChatRequest {
    let attachments = step
        .input
        .images
        .iter()
        .enumerate()
        .map(|(n, image)| Attachment {
            name: format!("image-{}", n + 1),
            kind: AttachmentKind::Image,
            media_type: image.mime_type.clone(),
            bytes: image.bytes.clone(),
        })
        .collect();
    let capabilities = candidate.model.capabilities.clone();
    let reasons = capabilities
        .as_ref()
        .and_then(|c| c.reasoning.as_ref())
        .is_some_and(|control| control.reasons());
    let switched_off = matches!(applied.option_id(), Some("none" | "off"));
    ChatRequest {
        model_id: candidate.model.remote_id.clone(),
        thread_id: None,
        system_prompt: Some(step.instruction.clone()),
        messages: vec![ChatMessage {
            role: ChatRole::User,
            content: user_text(step),
            attachments,
        }],
        reasoning_requested: reasons && !switched_off,
        max_new_tokens: Some(step.max_output_tokens),
        tools: Vec::new(),
        autonomy_mode: Default::default(),
        reasoning_option: applied.option_id().map(str::to_owned),
        capabilities,
        sampling: Sampling::Default,
    }
}

/// The message of a request that failed before the stream: never the body of the provider's
/// answer, which can echo the request (`contracts/bridge.md` §Ereignis).
fn start_failure(error: &AdapterError) -> String {
    match error {
        AdapterError::Status { status, .. } => {
            format!("the provider answered with status {status}")
        }
        other => other.to_string(),
    }
}

/// Asks the model and collects its answer as text.
async fn answer(
    source: &dyn ModelSource,
    candidate: &Candidate,
    request: ChatRequest,
) -> Result<String, Miss> {
    let adapter = source
        .adapter(&candidate.model)
        .map_err(Miss::Unavailable)?;
    let mut stream = adapter
        .stream_chat(request)
        .await
        .map_err(|error| Miss::Unavailable(start_failure(&error)))?;
    let mut text = String::new();
    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(StreamChunk::Delta { content, .. }) => {
                text.push_str(&content);
                if text.len() > MAX_RESULT_BYTES {
                    return Err(Miss::Unusable("too long".into()));
                }
            }
            // The request offered no tool; a model that calls one anyway gave no answer.
            Ok(StreamChunk::ToolCalls(_)) => return Err(Miss::Unusable("tool call".into())),
            Ok(StreamChunk::AgentActivity { .. }) => {}
            Ok(StreamChunk::Done { .. }) => break,
            Err(error) => return Err(Miss::Unavailable(error.to_string())),
        }
    }
    Ok(text)
}

/// Runs `step` over `candidates` until one gives a usable output, the deadline passes or the job
/// is cancelled.
pub(crate) async fn run_step(
    source: &dyn ModelSource,
    step: &Step,
    candidates: &[Candidate],
    cancel: &CancellationToken,
    deadline: Instant,
) -> StepRun {
    let context = match &step.input.text {
        Some(text) => format!("{}\n{text}", step.instruction),
        None => step.instruction.clone(),
    };
    let mut attempts = Vec::new();
    let mut last = (JobErrorCode::Unavailable, "no model was tried".to_owned());
    for candidate in candidates {
        let applied = effective(
            step.effort,
            candidate.ceiling,
            candidate.model.capabilities.as_ref(),
        );
        let secrets = candidate.model.secrets();
        let known: Vec<&str> = secrets.iter().map(String::as_str).collect();
        let attempt = |outcome, reason: Option<String>| Attempt {
            model: candidate.model.name.clone(),
            place: candidate.model.place,
            effort: applied.reported().to_owned(),
            outcome,
            reason: reason.map(|r| sanitize(&r, &known)),
        };
        let asked = answer(source, candidate, request(step, candidate, &applied));
        // `biased`: a job cancelled or over time before this try must not start a request.
        let outcome = tokio::select! {
            biased;
            () = cancel.cancelled() => return StepRun { attempts, end: StepEnd::Cancelled },
            () = tokio::time::sleep_until(deadline) => {
                attempts.push(attempt(Outcome::Timeout, None));
                return StepRun {
                    attempts,
                    end: StepEnd::Failed(JobErrorCode::Timeout, "the job took too long".into()),
                };
            }
            outcome = asked => outcome,
        };
        let miss = match outcome {
            Ok(text) => match guard::check(&text, &context, &step.format.schema) {
                Ok(value) => {
                    attempts.push(attempt(Outcome::Used, None));
                    return StepRun {
                        attempts,
                        end: StepEnd::Used(value),
                    };
                }
                Err(unusable) => Miss::Unusable(unusable.reason()),
            },
            Err(miss) => miss,
        };
        let (outcome, code, reason) = match miss {
            Miss::Unusable(reason) => (Outcome::Unusable, JobErrorCode::Unusable, reason),
            Miss::Unavailable(reason) => (Outcome::Unavailable, JobErrorCode::Unavailable, reason),
        };
        let entry = attempt(outcome, Some(reason));
        last = (code, entry.reason.clone().unwrap_or_default());
        attempts.push(entry);
    }
    StepRun {
        attempts,
        end: StepEnd::Failed(last.0, last.1),
    }
}
