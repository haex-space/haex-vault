//! The run of a job (spec 055 T027, T039, T041, T043; change of 2026-11-14): its steps in order, each
//! step goes to one model, asked once. holzi never tries another model on its own: when that model
//! gives nothing the guard and the format accept, or cannot be asked at all (the default model
//! cannot take the step, or is no longer available), the user is asked which other model takes the
//! step (`replacement`) and that model is asked once in turn. A
//! step with `from` gets the selected part of an earlier step's result as text. The request carries
//! no tools (FR-026); the instruction is the system prompt, the images are attachments and the
//! schema is a hint. The deadline of the job and its cancel token end the run at once: the stream
//! is dropped, which aborts the generation.

use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use serde::Serialize;
use serde_json::Value;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use super::candidates::Candidate;
use super::effort::{applied_effort, Applied};
use super::errors::{contains_sensitive, sanitize, JobErrorCode};
use super::guard;
use super::job::Step;
use super::limits::MAX_RESULT_BYTES;
use super::model_source::ModelSource;
use super::place::Place;
use super::trace;
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
    Cancelled,
}

/// One try of a model, as the provenance lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct Attempt {
    pub model: String,
    pub place: Place,
    /// The effort actually sent: `none`, `managed`, `low`, `medium`, `high`, `xhigh` or `max`.
    pub effort: String,
    pub outcome: Outcome,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// The model a step goes to first, or why none is asked: the user is then asked before any request
/// is sent (`Replace`).
#[derive(Debug, Clone)]
pub(crate) enum Start {
    Model(Box<Candidate>),
    /// The extension's default model is not asked: it cannot take the step or is not available.
    Ask(Blocked),
}

/// A default model that a step does not go to, as holzi knows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Blocked {
    pub id: String,
    /// The name and the place, while the vault still has the model.
    pub known: Option<(String, Place)>,
    pub cause: Cause,
    /// Why, for the provenance and the question ("it cannot take images").
    pub reason: String,
}

/// Why a model is not asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Cause {
    /// The model cannot do what the step needs (it takes no images).
    CannotTake,
    /// The extension may not use the model (any more), or the vault does not have it.
    NotAvailable,
}

/// The model that gave nothing, and why: what the question to the user names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Failed {
    pub model_id: String,
    /// Empty when the vault does not know the model any more.
    pub model_name: String,
    pub kind: FailedKind,
    pub reason: Option<String>,
}

/// How the model failed. The first two happen while the job runs, the others before a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FailedKind {
    Unusable,
    Unavailable,
    CannotTake,
    NotAvailable,
}

impl FailedKind {
    /// As the question to the window spells it.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unusable => "unusable",
            Self::Unavailable => "unavailable",
            Self::CannotTake => "cannot_take",
            Self::NotAvailable => "not_available",
        }
    }
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

/// Asked before every step but the first: which of the step's planned candidates the extension
/// may still use. A grant or consent the user withdrew while the job ran holds from the next step
/// on (`contracts/bridge.md` §Lebenszyklus).
#[async_trait]
pub(crate) trait Recheck: Send + Sync {
    /// `None` when the grants could not be read.
    async fn still_allowed(&self, planned: &[Candidate]) -> Option<Vec<Candidate>>;
}

/// Asked when the model of a step gave nothing usable or could not be asked: which other model
/// takes the step. The wait for the user does not count against the job's time.
#[async_trait]
pub(crate) trait Replace: Send + Sync {
    /// The model the user chose, with the effort it may use; `None` when the user declines, no
    /// window answers, no other model can do the step or the job was cancelled. `tried` are the ids
    /// of the models already named for this step, `failed` included.
    async fn replacement(
        &self,
        step: &Step,
        failed: &Failed,
        tried: &[String],
        cancel: &CancellationToken,
    ) -> Option<Candidate>;
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
        autonomy_deny: Default::default(),
        reasoning_option: applied.option_id().map(str::to_owned),
        capabilities,
        sampling: Sampling::Default,
    }
}

/// The message of a request that failed before the stream: never the body of the provider's
/// answer, which can echo the request (`contracts/bridge.md` §Ereignis).
pub(super) fn start_failure(error: &AdapterError) -> String {
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
pub(super) fn stream_failure(error: &StreamError) -> &'static str {
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
///
/// `started` is set immediately before the provider request begins so a caller can distinguish
/// cancellation before a request from cancellation while a request is in flight.
async fn answer(
    source: &dyn ModelSource,
    candidate: &Candidate,
    request: ChatRequest,
    started: &AtomicBool,
) -> Result<String, Miss> {
    let adapter = source.adapter(&candidate.model).map_err(|_| {
        trace::failed(
            &candidate.model,
            "adapter",
            "the model adapter is unavailable",
        );
        Miss::Unavailable("the model adapter is unavailable".into())
    })?;
    started.store(true, Ordering::Release);
    let mut stream = adapter.stream_chat(request).await.map_err(|error| {
        trace::failed(&candidate.model, "start", &start_failure(&error));
        Miss::Unavailable(start_failure(&error))
    })?;
    let mut text = String::new();
    let mut cut = false;
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
            Ok(StreamChunk::Done {
                finish_reason,
                completion_tokens,
                ..
            }) => {
                trace::ended(
                    &candidate.model,
                    finish_reason.as_deref(),
                    completion_tokens,
                );
                cut = trace::cut_off(finish_reason.as_deref());
                break;
            }
            Err(error) => {
                trace::failed(&candidate.model, "stream", stream_failure(&error));
                return Err(Miss::Unavailable(stream_failure(&error).into()));
            }
        }
    }
    if cut {
        return Err(Miss::Unusable("cut off at the output limit".into()));
    }
    Ok(text)
}

/// The user's answer to the question after a failure, with the time it took added to `deadline`.
async fn ask_user(
    replace: &dyn Replace,
    step: &Step,
    failed: &Failed,
    tried: &[String],
    cancel: &CancellationToken,
    deadline: &mut Instant,
) -> Option<Candidate> {
    let asked_at = Instant::now();
    let chosen = replace.replacement(step, failed, tried, cancel).await;
    *deadline += asked_at.elapsed();
    chosen
}

/// Runs `step`: the model of `start` is asked once; when it gives nothing usable, or `start` says
/// that the default model cannot be asked, `replace` asks the user which other model takes the step,
/// and that model is asked once in turn. No model is ever tried without the user's answer. The time
/// the user takes is added to `deadline`, which the next step then keeps. `handed` is the part of an
/// earlier step's result the step reads (`from`).
pub(crate) async fn run_step(
    source: &dyn ModelSource,
    step: &Step,
    start: &Start,
    replace: &dyn Replace,
    cancel: &CancellationToken,
    deadline: &mut Instant,
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
    let mut tried: Vec<String> = Vec::new();
    let mut next: Option<Candidate> = match start {
        Start::Model(candidate) => Some(Candidate::clone(candidate)),
        Start::Ask(blocked) => {
            // Nothing is sent: the model is named in the provenance and the user is asked first.
            tried.push(blocked.id.clone());
            let reason = blocked.reason.clone();
            let kind = match blocked.cause {
                Cause::CannotTake => FailedKind::CannotTake,
                Cause::NotAvailable => FailedKind::NotAvailable,
            };
            if let Some((name, place)) = &blocked.known {
                attempts.push(Attempt {
                    model: name.clone(),
                    place: *place,
                    effort: "none".into(),
                    outcome: Outcome::Unavailable,
                    reason: Some(reason.clone()),
                });
            }
            last = (JobErrorCode::NoCapableModel, reason.clone());
            let failed = Failed {
                model_id: blocked.id.clone(),
                model_name: blocked
                    .known
                    .as_ref()
                    .map(|(name, _)| name.clone())
                    .unwrap_or_default(),
                kind,
                reason: Some(reason),
            };
            let chosen = ask_user(replace, step, &failed, &tried, cancel, deadline).await;
            if cancel.is_cancelled() {
                return StepRun {
                    attempts,
                    end: StepEnd::Cancelled,
                };
            }
            chosen
        }
    };
    while let Some(candidate) = next.take() {
        let candidate = &candidate;
        tried.push(candidate.model.id.clone());
        // The effort is the user's choice for this model, never the extension's wish.
        let applied = applied_effort(candidate.ceiling, candidate.model.capabilities.as_ref());
        let secrets = candidate.model.secrets();
        let known: Vec<&str> = secrets.iter().map(String::as_str).collect();
        let attempt = |outcome, reason: Option<String>| Attempt {
            model: candidate.model.name.clone(),
            place: candidate.model.place,
            effort: applied.reported().to_owned(),
            outcome,
            reason: reason.map(|r| sanitize(&r, &known)),
        };
        let started = AtomicBool::new(false);
        let asked = answer(
            source,
            candidate,
            request(step, candidate, &applied, handed),
            &started,
        );
        // `biased`: a job cancelled or over time before this try must not start a request.
        let outcome = tokio::select! {
            biased;
            () = cancel.cancelled() => {
                if started.load(Ordering::Acquire) {
                    trace::tried(&candidate.model, applied.reported(), Outcome::Cancelled);
                    attempts.push(attempt(Outcome::Cancelled, None));
                }
                return StepRun { attempts, end: StepEnd::Cancelled };
            },
            () = tokio::time::sleep_until(*deadline) => {
                trace::tried(&candidate.model, applied.reported(), Outcome::Timeout);
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
                    trace::tried(&candidate.model, applied.reported(), Outcome::Used);
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
        let (outcome, kind, code, reason) = match miss {
            Miss::Unusable(reason) => (
                Outcome::Unusable,
                FailedKind::Unusable,
                JobErrorCode::Unusable,
                reason,
            ),
            Miss::Unavailable(reason) => (
                Outcome::Unavailable,
                FailedKind::Unavailable,
                JobErrorCode::Unavailable,
                reason,
            ),
        };
        trace::tried(&candidate.model, applied.reported(), outcome);
        if outcome == Outcome::Unusable {
            trace::failed(&candidate.model, "answer", &reason);
        }
        let entry = attempt(outcome, Some(reason));
        last = (code, entry.reason.clone().unwrap_or_default());
        // The model gave nothing: holzi does not try another on its own, the user chooses.
        let failed = Failed {
            model_id: candidate.model.id.clone(),
            model_name: candidate.model.name.clone(),
            kind,
            reason: entry.reason.clone(),
        };
        let chosen = ask_user(replace, step, &failed, &tried, cancel, deadline).await;
        attempts.push(entry);
        if cancel.is_cancelled() {
            return StepRun {
                attempts,
                end: StepEnd::Cancelled,
            };
        }
        next = chosen;
    }
    StepRun {
        attempts,
        end: StepEnd::Failed(last.0, last.1),
    }
}

/// Runs the steps of a job in order. Before every step but the first, `recheck` says whether the
/// step's planned model may still be used; when it may not, the user is asked which other model takes
/// the step (a grant withdrawn while the job ran, `contracts/bridge.md` §Lebenszyklus), and when no
/// other model can, the step ends with `no_capable_model`. A step reads the result of an earlier one
/// through `from`; when the part is missing from that result, the step ends `unusable` without trying
/// a model, since another model cannot change what the earlier step gave. The first step that ends
/// without a result ends the job.
pub(crate) async fn run_job(
    source: &dyn ModelSource,
    steps: &[(Step, Start)],
    recheck: &dyn Recheck,
    replace: &dyn Replace,
    cancel: &CancellationToken,
    deadline: Instant,
) -> JobRun {
    let mut deadline = deadline;
    let mut ran = Vec::new();
    let mut results: Vec<(String, Value)> = Vec::new();
    for (index, (step, planned)) in steps.iter().enumerate() {
        let record = |attempts: Vec<Attempt>| RanStep {
            step_id: step.id.clone(),
            format_name: step.format.name.clone(),
            format_version: step.format.version,
            attempts,
        };
        let fail = |steps: Vec<RanStep>, code, message: &str| JobRun {
            steps,
            end: JobEnd::Failed {
                step_id: step.id.clone(),
                code,
                message: message.into(),
            },
        };
        let start = match planned {
            Start::Model(candidate) if index > 0 => {
                match recheck.still_allowed(std::slice::from_ref(candidate)).await {
                    None => {
                        ran.push(record(Vec::new()));
                        return fail(
                            ran,
                            JobErrorCode::Unavailable,
                            "the grants could not be read",
                        );
                    }
                    Some(left) => match left.into_iter().next() {
                        Some(candidate) => Start::Model(Box::new(candidate)),
                        // The user decides what takes the step now.
                        None => Start::Ask(Blocked {
                            id: candidate.model.id.clone(),
                            known: Some((candidate.model.name.clone(), candidate.model.place)),
                            cause: Cause::NotAvailable,
                            reason: "a grant was withdrawn".into(),
                        }),
                    },
                }
            }
            planned => planned.clone(),
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
                    return fail(ran, JobErrorCode::Unusable, "from missing");
                };
                Some(part.to_string())
            }
        };
        let run = run_step(
            source,
            step,
            &start,
            replace,
            cancel,
            &mut deadline,
            handed.as_deref(),
        )
        .await;
        ran.push(record(run.attempts));
        match run.end {
            StepEnd::Used(value) => results.push((step.id.clone(), value)),
            StepEnd::Failed(code, message) => return fail(ran, code, &message),
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
