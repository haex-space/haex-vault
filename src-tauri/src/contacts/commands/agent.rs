//! Commands of the built-in agent's contact actions (spec 051 US6, contracts/tauri-commands.md
//! §Agenten): `contacts.search`, `contacts.get` and `contacts.update` of the action catalog. The
//! caller is fixed here, `Caller::BuiltinAgent`; holzi asks the user for a grant it lacks and the
//! command waits for the answer (at most a minute).

use serde::Deserialize;
use tauri::State;
use tokio_util::sync::CancellationToken;
use ts_rs::TS;

use crate::contacts::agent::ContactsAgent;
use crate::contacts::model::ContactPatch;
use crate::contacts::outside::{OutsideContact, OutsideSummary};
use crate::error::Result;
use crate::files::agent::prompt::PermissionPrompt;
use crate::state::AppState;
use crate::state_utils::active_database;

#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/", optional_fields)]
#[serde(rename_all = "camelCase")]
pub struct ContactsAgentSearchArgs {
    pub query: String,
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct ContactsAgentGetArgs {
    pub id: String,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct ContactsAgentUpdateArgs {
    pub id: String,
    pub patch: ContactPatch,
}

fn agent(
    state: &State<'_, AppState>,
    prompt: &State<'_, PermissionPrompt>,
) -> Result<ContactsAgent> {
    Ok(ContactsAgent::builtin(
        active_database(state)?,
        PermissionPrompt::clone(prompt),
    ))
}

#[tauri::command]
pub async fn contacts_agent_search(
    state: State<'_, AppState>,
    prompt: State<'_, PermissionPrompt>,
    args: ContactsAgentSearchArgs,
) -> Result<Vec<OutsideSummary>> {
    agent(&state, &prompt)?
        .search(&args.query, args.limit, &CancellationToken::new())
        .await
}

#[tauri::command]
pub async fn contacts_agent_get(
    state: State<'_, AppState>,
    prompt: State<'_, PermissionPrompt>,
    args: ContactsAgentGetArgs,
) -> Result<OutsideContact> {
    agent(&state, &prompt)?
        .get(&args.id, &CancellationToken::new())
        .await
}

#[tauri::command]
pub async fn contacts_agent_update(
    state: State<'_, AppState>,
    prompt: State<'_, PermissionPrompt>,
    args: ContactsAgentUpdateArgs,
) -> Result<OutsideContact> {
    agent(&state, &prompt)?
        .update(&args.id, args.patch, &CancellationToken::new())
        .await
}
