//! Changing the rights group `browser` of an agent from inside a running session (spec 065): the
//! built-in agent's setting or the agent profile of an external agent (spec 083), one way for every
//! "Für diese Website merken".

use tauri::{AppHandle, Emitter, Manager, Runtime};
use uuid::Uuid;

use super::env::device;
use crate::caller::Caller;
use crate::mcp_server::grants::BrowserRights;
use crate::AppState;

/// Changes the rights group `browser` of `caller`; `change` says whether it changed anything.
pub(super) async fn change_browser_rights<R: Runtime>(
    app: &AppHandle<R>,
    caller: &Caller,
    expected_profile: Option<String>,
    change: impl FnOnce(&mut BrowserRights) -> bool + Send + 'static,
) -> Result<(), String> {
    match caller {
        Caller::BuiltinAgent => {
            // The model of the turn is in an agent profile; what is remembered goes there (spec
            // 083 FR-015, US5).
            let subject = crate::agent_profiles::local::builtin_subject();
            let db = app
                .state::<AppState>()
                .database()
                .map_err(|error| error.to_string())?;
            db.write(move |tx| {
                if let Some(expected) = expected_profile.as_deref() {
                    let actual = match subject.as_ref() {
                        Some(subject) => {
                            crate::agent_profiles::resolve::profile_for_remember(tx, subject)?
                                .ok_or_else(|| {
                                    haex_crdt::Error::consumer("agent profile is unreadable")
                                })?
                                .to_string()
                        }
                        None => crate::agent_profiles::STANDARD_PROFILE_ID.to_string(),
                    };
                    if actual != expected {
                        return Err(haex_crdt::Error::consumer(
                            "agent profile changed while approval was open",
                        ));
                    }
                }
                crate::agent_profiles::remember::change_browser_of(tx, subject.as_ref(), change)
            })
            .await
            .map_err(|error| error.to_string())?;
            let _ = app.emit(crate::mcp_server::prompts::EVENT_AGENTS_CHANGED, ());
            Ok(())
        }
        Caller::ExternalAgent { id } => {
            let agent = Uuid::parse_str(id).map_err(|error| error.to_string())?;
            let device = device(app).await?;
            let db = app
                .state::<AppState>()
                .database()
                .map_err(|error| error.to_string())?;
            // What is remembered goes into the agent profile of the agent, so it holds for every
            // agent in it (spec 083 FR-015); "Standard" gets its row on the first change.
            db.write(move |tx| {
                if let Some(expected) = expected_profile.as_deref() {
                    let subject = crate::mcp_server::registry::subject_of_id(tx, device, agent)?;
                    let actual =
                        crate::agent_profiles::resolve::profile_for_remember(tx, &subject)?
                            .ok_or_else(|| {
                                haex_crdt::Error::consumer("agent profile is unreadable")
                            })?
                            .to_string();
                    if actual != expected {
                        return Err(haex_crdt::Error::consumer(
                            "agent profile changed while approval was open",
                        ));
                    }
                }
                crate::agent_profiles::remember::change_browser(tx, device, agent, change)
            })
            .await
            .map_err(|error| error.to_string())?;
            let _ = app.emit(crate::mcp_server::prompts::EVENT_AGENTS_CHANGED, ());
            Ok(())
        }
        _ => Err("not an agent".to_owned()),
    }
}
