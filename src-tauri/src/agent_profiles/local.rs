//! The local agent in agent profiles (spec 083 US5, research R6): the in-app chat agent has no
//! registration, its subject is the model of the turn. The model is read from the task a turn runs
//! its tool calls in (`chat::tools::agent_task`), never from the global chat session, so a call of
//! one turn never gets the rights of another model's. A call outside a turn is "Standard".

use super::{resolve, Subject};
use crate::chat::session::ActiveSession;
use crate::chat::tools::agent_task;
use crate::mcp_server::grants::Rights;
use crate::storage::providers::ProviderKind;
use crate::storage::query::Query;

use std::sync::Mutex;

/// The model of the latest turn that ran tool calls: what a background task of the agent browser
/// (a page request, a new tab) falls back to, because a spawned task does not inherit the task of
/// its turn. `None` before any turn.
static LATEST: Mutex<Option<Subject>> = Mutex::new(None);

/// The subject of a turn with `session`'s model. A CLI delegate is an Abo-Agent that reaches
/// holzi over MCP as itself and is judged there, so it has no subject here.
pub fn subject_of_session(session: &ActiveSession) -> Option<Subject> {
    match session.provider_kind {
        ProviderKind::CliDelegate => None,
        ProviderKind::Local | ProviderKind::ApiKey => {
            Some(Subject::Model(session.model_id.clone()))
        }
    }
}

/// Remembers `subject` as the model of the latest turn (see [`LATEST`]).
pub(crate) fn note_turn(subject: Option<&Subject>) {
    if let Some(subject) = subject {
        *LATEST
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(subject.clone());
    }
}

/// The subject the running call belongs to: the model of its turn; a call in a task spawned from a
/// turn gets the model of the latest turn. `None` is "Standard".
pub fn builtin_subject() -> Option<Subject> {
    match agent_task::current() {
        Some(task) => task.subject,
        None => LATEST
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone(),
    }
}

/// What the running call of the local agent may do.
pub fn current_rights(q: &mut impl Query) -> haex_crdt::Result<Rights> {
    rights_for(q, builtin_subject().as_ref())
}

/// What the local agent may do as `subject`; "Standard" for none.
pub fn rights_for(q: &mut impl Query, subject: Option<&Subject>) -> haex_crdt::Result<Rights> {
    match subject {
        Some(subject) => resolve::rights_of(q, subject),
        None => resolve::standard_rights(q),
    }
}
