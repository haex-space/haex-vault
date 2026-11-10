//! The task a tool call of an agent runs in (spec 048 FR-030a, FR-031): which turn it belongs to
//! and where its model runs. A turn of the chat sets it around its tool calls; the file actions
//! read it to keep an answer without "Erlaubnis merken" for this turn only and to ask again when a
//! grant for local models meets a cloud model. Outside a turn there is none, and a caller treats
//! the model as a cloud model.

use std::future::Future;

use crate::agent_profiles::Subject;
use crate::chat::session::ActiveSession;
use crate::files::access::Locality;
use crate::storage::providers::ProviderKind;

tokio::task_local! {
    static TASK: AgentTask;
}

/// One task of an agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentTask {
    /// Unique per task: the turn's assistant message.
    pub id: String,
    /// The model's name as the user knows it.
    pub model: String,
    pub locality: Locality,
    /// The model as an agent profile sees it (spec 083); `None` for a delegate, which is judged
    /// over MCP as itself.
    pub subject: Option<Subject>,
}

impl AgentTask {
    /// The task of a turn with `session`'s model. Only a model of this device is local; an API
    /// key's model and a delegate run elsewhere (FR-030a).
    pub fn of_turn(turn_id: &str, session: &ActiveSession) -> Self {
        let locality = match session.provider_kind {
            ProviderKind::Local => Locality::Local,
            ProviderKind::ApiKey | ProviderKind::CliDelegate => Locality::Cloud,
        };
        Self {
            id: turn_id.to_owned(),
            model: model_name(&session.model_id),
            locality,
            subject: crate::agent_profiles::local::subject_of_session(session),
        }
    }
}

/// `<provider_id>:<remote_id>` names the model by its remote id; a local model by its catalog id.
fn model_name(model_id: &str) -> String {
    model_id
        .split_once(':')
        .map_or(model_id, |(_, remote)| remote)
        .to_owned()
}

/// Runs `work` as part of `task`.
pub async fn within<F: Future>(task: AgentTask, work: F) -> F::Output {
    crate::agent_profiles::local::note_turn(task.subject.as_ref());
    TASK.scope(task, work).await
}

/// Runs browser work under the model that owns a page, even when its CDP callback is a spawned
/// task and no longer inherits the turn's task-local value.
pub async fn with_subject<F: Future>(subject: Option<Subject>, work: F) -> F::Output {
    let mut task = current().unwrap_or_else(|| AgentTask {
        id: String::new(),
        model: String::new(),
        locality: Locality::Cloud,
        subject: None,
    });
    task.subject = subject;
    TASK.scope(task, work).await
}

/// Runs a frontend bridge check with the model and locality captured by the originating action
/// call. A fresh id keeps its temporary permission answer inside this one check.
pub async fn with_origin<F: Future>(
    model: Option<String>,
    locality: Locality,
    work: F,
) -> F::Output {
    let subject = model.clone().map(Subject::Model);
    TASK.scope(
        AgentTask {
            id: uuid::Uuid::new_v4().to_string(),
            model: model.unwrap_or_default(),
            locality,
            subject,
        },
        work,
    )
    .await
}

/// The task the running call belongs to, if any.
pub fn current() -> Option<AgentTask> {
    TASK.try_with(Clone::clone).ok()
}

#[cfg(test)]
#[path = "agent_task_tests.rs"]
mod tests;
