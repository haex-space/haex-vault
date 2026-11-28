//! The model a turn answers with (spec 068 T012, research R2).
//!
//! `ChatState::session` is the model of the conversation that is open in the window: the window
//! loads it when a conversation opens (`use_session_model`), and a pick in the model menu replaces
//! it. A turn must not depend on it staying that way: while conversation A answers, the person opens
//! conversation B and picks another cloud model there, and a send to A from the voice assistant, from
//! another window or from the agent-control server would read B's model. So a turn resolves its
//! adapter from the model of *its own conversation* when it starts, and holds it until it ends.
//!
//! - A conversation whose model is a provider's (or a delegate's) gets an adapter of its own,
//!   unless the open model already is that one.
//! - A conversation whose model is a local one uses the engine in memory when it is that model: the
//!   local model is one for all conversations, a second one is not loaded beside it.
//! - Anything else (a new conversation, a model that cannot be built any more) uses the model of
//!   the open session, as before.

use tauri::{AppHandle, State};
use uuid::Uuid;

use super::delegate_context::delegate_chat_context;
use super::model_loading::load_api_key_model;
use super::session::{ActiveSession, ChatState};
use crate::error::{HaexError, Result};
use crate::state::AppState;
use crate::state_utils::active_database;
use crate::storage::chat_threads as thread_store;

/// Where a turn takes its session from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TurnModel {
    /// The model of the open session.
    Open,
    /// An adapter built for this provider model (`<provider>:<id>`).
    Provider(String),
    /// The local engine in memory, when it is this model.
    Local(String),
}

/// Decides where a turn takes its model from, given the model its conversation answered with last
/// (`None` for a new conversation) and the open session's model.
pub(crate) fn decide(thread_model: Option<&str>, open: Option<&str>) -> TurnModel {
    match thread_model.filter(|id| !id.is_empty()) {
        Some(id) if open == Some(id) => TurnModel::Open,
        Some(id) if id.contains(':') => TurnModel::Provider(id.to_string()),
        Some(id) => TurnModel::Local(id.to_string()),
        None => TurnModel::Open,
    }
}

/// The session a turn in `thread_id` (`None`: a new conversation) runs with.
pub(crate) async fn session_for_turn(
    app: &AppHandle,
    state: &State<'_, AppState>,
    chat: &ChatState,
    thread_id: Option<Uuid>,
) -> Result<ActiveSession> {
    let open = chat
        .session
        .lock()
        .map_err(|e| HaexError::CrdtInit {
            reason: format!("chat.session mutex poisoned: {e}"),
        })?
        .clone();
    let thread_model = match thread_id {
        Some(id) => {
            let db = active_database(state)?;
            db.read(move |r| Ok(thread_store::get_thread(r, id)?.and_then(|t| t.last_model_id)))
                .await?
        }
        None => None,
    };
    let own = match decide(
        thread_model.as_deref(),
        open.as_ref().map(|s| s.model_id.as_str()),
    ) {
        TurnModel::Open => None,
        TurnModel::Local(id) => chat.session_for_local_model(&id),
        TurnModel::Provider(id) => match id.split_once(':') {
            Some((provider, _)) => {
                let context = delegate_chat_context(app, state, chat)?;
                load_api_key_model(state, &id, provider, Some(context))
                    .await
                    .inspect_err(|error| {
                        log::warn!(
                            "turn model {id} cannot be built, using the open model: {error}"
                        );
                    })
                    .ok()
            }
            None => None,
        },
    };
    own.or(open).ok_or_else(|| HaexError::InvalidInput {
        reason: "no model loaded".into(),
    })
}
