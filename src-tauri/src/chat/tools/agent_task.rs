//! The task a tool call of an agent runs in (spec 048 FR-030a, FR-031): which turn it belongs to
//! and where its model runs. A turn of the chat sets it around its tool calls; the file actions
//! read it to keep an answer without "Erlaubnis merken" for this turn only and to ask again when a
//! grant for local models meets a cloud model. Outside a turn there is none, and a caller treats
//! the model as a cloud model.

use std::future::Future;

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
    TASK.scope(task, work).await
}

/// The task the running call belongs to, if any.
pub fn current() -> Option<AgentTask> {
    TASK.try_with(Clone::clone).ok()
}

#[cfg(test)]
#[path = "agent_task_tests.rs"]
mod tests;
