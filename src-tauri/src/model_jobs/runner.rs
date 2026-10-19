//! The run of a job (spec 055 T027, T039, T041, T043): its steps in order, each step's candidates
//! in order and each at most once, until one gives an output the guard and the format accept. A
//! step with `from` gets the selected part of an earlier step's result as text. The request carries
//! no tools (FR-026); the instruction is the system prompt, the images are attachments and the
//! schema is a hint. The deadline of the job and its cancel token end the run at once: the stream
//! is dropped, which aborts the generation.

use serde::Serialize;
use serde_json::Value;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use super::candidates::Candidate;
use super::effort::{effective, Applied};
use super::errors::{contains_sensitive, sanitize, JobErrorCode};
use super::guard;
use super::job::Step;
use super::limits::MAX_RESULT_BYTES;
use super::model_source::ModelSource;
use super::place::Place;
use crate::adapters::types::{
    Attachment, AttachmentKind, ChatMessage, ChatRequest, ChatRole, Sampling, StreamChunk,
    StreamError,
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

/// One step of a job as it ran, for the provenance.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RanStep {
    pub step_id: String,
    pub format_name: String,
    pub format_version: u32,
    pub attempts: Vec<Attempt>,
}

/// How a job ended.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum JobEnd {
    /// Every step's result, in the order of the steps.
    Completed(Vec<(String, Value)>),
    Failed {
        step_id: String,
        code: JobErrorCode,
        message: String,
    },
    Cancelled,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct JobRun {
    pub steps: Vec<RanStep>,
    pub end: JobEnd,
}

/// Why one try gave nothing.
enum Miss {
    Unusable(String),
    Unavailable(String),
}

/// The text the model reads besides the instruction: the input text, the part `handed` over from
/// an earlier step (as JSON), and the format.
fn user_text(step: &Step, handed: Option<&str>) -> String {
    let schema = step.format.schema.to_string();
    step.input
        .text
        .iter()
        .map(String::as_str)
        .chain(handed)
        .chain([format!("{FORMAT_SENTENCE}\n{schema}").as_str()])
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// The request for one candidate. No tools, ever (FR-026).
pub(crate) fn request(
    step: &Step,
    candidate: &Candidate,
    applied: &Applied,
    handed: Option<&str>,
) -> ChatRequest {
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
            content: user_text(step, handed),
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
        AdapterError::InvalidCredentials => "invalid provider credentials".into(),
        AdapterError::Http { .. } => "the provider transport failed".into(),
        AdapterError::Parse { .. } => "the provider response could not be parsed".into(),
        AdapterError::Unavailable { .. } => "the provider is unavailable".into(),
        AdapterError::UntrustedCertificate { .. } => {
            "the provider certificate is not trusted".into()
        }
    }
}

/// Stream errors may contain provider response bodies. Keep their category, never their details:
/// the extension must not receive an answer body or arbitrary provider text.
fn stream_failure(error: &StreamError) -> &'static str {
    match error {
        StreamError::Validation(_) => "the provider response was invalid",
        StreamError::Model(_) => "the provider rejected the request",
        StreamError::Internal(_) => "the provider stream failed",
        StreamError::UnexpectedEnd => "the provider stream ended unexpectedly",
        StreamError::StartFailed(_) => "the model failed to start",
        StreamError::Transient(_) => "the provider is temporarily unavailable",
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
        .map_err(|_| Miss::Unavailable("the model adapter is unavailable".into()))?;
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
            Err(error) => return Err(Miss::Unavailable(stream_failure(&error).into())),
        }
    }
    Ok(text)
}

/// Runs `step` over `candidates` until one gives a usable output, the deadline passes or the job
/// is cancelled. `handed` is the part of an earlier step's result the step reads (`from`).
pub(crate) async fn run_step(
    source: &dyn ModelSource,
    step: &Step,
    candidates: &[Candidate],
    cancel: &CancellationToken,
    deadline: Instant,
    handed: Option<&str>,
) -> StepRun {
    // The scripts of everything the model reads may come back in its answer.
    let context = [
        Some(step.instruction.as_str()),
        step.input.text.as_deref(),
        handed,
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join("\n");
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
        let asked = answer(
            source,
            candidate,
            request(step, candidate, &applied, handed),
        );
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
                Ok(value) if contains_sensitive(&value, &known) => {
                    Miss::Unusable("sensitive output".into())
                }
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

/// Runs the steps of a job in order. A step reads the result of an earlier one through `from`;
/// when the part is missing from that result, the step ends `unusable` without trying a model,
/// since another model cannot change what the earlier step gave. The first step that ends without
/// a result ends the job.
pub(crate) async fn run_job(
    source: &dyn ModelSource,
    steps: &[(Step, Vec<Candidate>)],
    cancel: &CancellationToken,
    deadline: Instant,
) -> JobRun {
    let mut ran = Vec::new();
    let mut results: Vec<(String, Value)> = Vec::new();
    for (step, candidates) in steps {
        let record = |attempts: Vec<Attempt>| RanStep {
            step_id: step.id.clone(),
            format_name: step.format.name.clone(),
            format_version: step.format.version,
            attempts,
        };
        let handed = match &step.input.from {
            None => None,
            Some(from) => {
                let part = results
                    .iter()
                    .find(|(id, _)| *id == from.step)
                    .and_then(|(_, result)| result.pointer(&from.pointer));
                let Some(part) = part else {
                    ran.push(record(Vec::new()));
                    return JobRun {
                        steps: ran,
                        end: JobEnd::Failed {
                            step_id: step.id.clone(),
                            code: JobErrorCode::Unusable,
                            message: "from missing".into(),
                        },
                    };
                };
                Some(part.to_string())
            }
        };
        let run = run_step(
            source,
            step,
            candidates,
            cancel,
            deadline,
            handed.as_deref(),
        )
        .await;
        ran.push(record(run.attempts));
        match run.end {
            StepEnd::Used(value) => results.push((step.id.clone(), value)),
            StepEnd::Failed(code, message) => {
                return JobRun {
                    steps: ran,
                    end: JobEnd::Failed {
                        step_id: step.id.clone(),
                        code,
                        message,
                    },
                }
            }
            StepEnd::Cancelled => {
                return JobRun {
                    steps: ran,
                    end: JobEnd::Cancelled,
                }
            }
        }
    }
    JobRun {
        steps: ran,
        end: JobEnd::Completed(results),
    }
}
