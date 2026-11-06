//! Changing the rights group `browser` of an agent from inside a running session (spec 065): the
//! built-in agent's setting or the agent profile of an external agent (spec 083), one way for every
//! "Für diese Website merken".

use tauri::{AppHandle, Emitter, Manager, Runtime};
use uuid::Uuid;

use super::env::{builtin_rights, device, set_builtin_rights};
use crate::caller::Caller;
use crate::mcp_server::grants::BrowserRights;
use crate::AppState;

/// Changes the rights group `browser` of `caller`; `change` says whether it changed anything.
pub(super) async fn change_browser_rights<R: Runtime>(
    app: &AppHandle<R>,
    caller: &Caller,
    change: impl FnOnce(&mut BrowserRights) -> bool + Send + 'static,
) -> Result<(), String> {
    match caller {
        Caller::BuiltinAgent => {
            let mut rights = builtin_rights(app)
                .await
                .ok_or("the browser is off for the built-in agent")?;
            if change(&mut rights) {
                set_builtin_rights(app, Some(rights)).await?;
            }
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
