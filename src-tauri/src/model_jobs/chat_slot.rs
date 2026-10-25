//! holzi's own [`LocalSlot`]: the single local-model place of the chat (spec 055 FR-033, R15).
//!
//! A job on a local model takes one of three ways, by what the chat holds:
//!
//! - The chat holds that very model: the job uses its adapter at once.
//! - The chat holds no local model (nothing, or a model on a server): nothing is in the way in
//!   memory, so the job loads the model for itself and lets it go when it is done.
//! - The chat holds another local model: the job waits until no chat turn or load runs, takes the
//!   place, and gives it back by loading the chat's model again when the job is done. While it
//!   holds the place the chat turns away new messages, as it does during any load.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tauri::{AppHandle, Manager};

use super::local_slot::{Loan, LocalSlot};
use crate::chat::events::emit_load_error;
use crate::chat::model_loading::{load_local_session, load_model_inner, LoadIdentity};
use crate::chat::session::ChatState;
use crate::state::AppState;

/// How often a job looks again whether the chat has let go of its place.
const RETRY: Duration = Duration::from_millis(250);

pub(crate) struct ChatSlot {
    app: AppHandle,
}

impl ChatSlot {
    pub(crate) fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

/// The chat's loaded model when it is a local one with this id.
fn loaded_adapter(
    chat: &ChatState,
    model_id: &str,
) -> Option<Arc<dyn crate::adapters::ProviderAdapter>> {
    let session = chat.session.lock().unwrap_or_else(|e| e.into_inner());
    session
        .as_ref()
        .filter(|s| s.provider_id.is_none() && s.model_id == model_id)
        .map(|s| Arc::clone(&s.adapter))
}

/// The id of the chat's loaded model when it is a local one.
fn local_model_of(chat: &ChatState) -> Option<String> {
    let session = chat.session.lock().unwrap_or_else(|e| e.into_inner());
    session
        .as_ref()
        .filter(|s| s.provider_id.is_none())
        .map(|s| s.model_id.clone())
}

#[async_trait]
impl LocalSlot for ChatSlot {
    async fn lend(&self, model_id: &str) -> Result<Loan, String> {
        let chat = self.app.state::<ChatState>();
        let state = self.app.state::<AppState>();

        // The local adapter is shared with the chat. Keep the operation slot until the job's
        // stream ends so a chat turn cannot use it concurrently, even when it is the same model.
        let operation = loop {
            if let Ok(guard) = chat.acquire_operation() {
                break guard;
            }
            tokio::time::sleep(RETRY).await;
        };
        if let Some(adapter) = loaded_adapter(&chat, model_id) {
            return Ok(Loan::until(adapter, move || drop(operation)));
        }
        if local_model_of(&chat).is_none() {
            let session = load_local_session(&self.app, &state, model_id)
                .await
                .map_err(|error| error.to_string())?;
            return Ok(Loan::until(session.adapter, move || drop(operation)));
        }

        // The chat may have loaded the wanted model, or let go of its own, while the job waited.
        if let Some(adapter) = loaded_adapter(&chat, model_id) {
            return Ok(Loan::until(adapter, move || drop(operation)));
        }
        let previous = local_model_of(&chat);

        let generation = chat.vault_generation();
        chat.cancel_tool_check();
        // Let go of the chat's model first: both do not fit in memory.
        let displaced = chat
            .session
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        drop(displaced);
        let app = self.app.clone();
        let release = move || {
            // Without a model of its own the chat has nothing to get back; the place is free.
            let Some(previous) = previous else {
                drop(operation);
                return;
            };
            tauri::async_runtime::spawn(give_back(app, previous, generation, operation));
        };
        let mut loan = Loan::pending(release);
        match load_local_session(&self.app, &state, model_id).await {
            Ok(session) => {
                loan.install(session.adapter);
                Ok(loan)
            }
            Err(error) => Err(error.to_string()),
        }
    }
}

/// Loads the model the chat had before the job took its place, and frees the place when done.
/// A vault that closed in between has nothing to give back.
async fn give_back(
    app: AppHandle,
    model_id: String,
    vault_generation: u64,
    operation: tokio::sync::OwnedMutexGuard<()>,
) {
    let chat = app.state::<ChatState>();
    let state = app.state::<AppState>();
    if chat.vault_generation() != vault_generation {
        return;
    }
    let (generation, load_id) = chat.begin_model_load();
    let identity = LoadIdentity {
        vault_generation: generation,
        load_id,
    };
    let load = load_model_inner(&app, &state, &chat, &model_id, identity, None, false);
    let failed = match state.gate().run(load).await {
        Ok(Ok(_)) => false,
        Ok(Err(error)) => {
            log::warn!("giving the chat its model back after a job failed: {error}");
            true
        }
        Err(_) => true,
    };
    if failed {
        chat.set_model_error(
            generation,
            load_id,
            Some(model_id.clone()),
            None,
            "model_load_failed".into(),
        );
        emit_load_error(
            &app,
            generation,
            load_id,
            Some(model_id),
            None,
            "model_load_failed",
        );
    }
    drop(operation);
}
