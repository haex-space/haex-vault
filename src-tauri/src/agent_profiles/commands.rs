//! Commands of "KI → Agentenprofile" (spec 083, contracts/commands-and-actions.md). Only the
//! window calls them, behind the guardrail actions `settings.agents.profiles.*`; no agent reaches
//! one. A write changes the tables; the live MCP sessions follow from the table feed
//! (`mcp_server::attach::tell_catalog_changes`), so no command refreshes them itself. Each write
//! emits `EVENT_AGENTS_CHANGED` for the pages. The errors are codes in the reason:
//! `name_empty`, `standard_fixed`, `profile_missing`, `subject_unknown`.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Runtime, State};
use ts_rs::TS;
use uuid::Uuid;

use super::subjects::{self, AgentProfileView, AgentSubjectView};
use super::{choices, resolve, store, Subject, STANDARD_PROFILE_ID};
use crate::error::{HolziError, Result};
use crate::mcp_server::grants::{self, Rights};
use crate::mcp_server::prompts::EVENT_AGENTS_CHANGED;
use crate::state::AppState;

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct AgentProfileArgs {
    pub id: Uuid,
}

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct AgentProfileCreateArgs {
    pub name: String,
}

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct AgentProfileRenameArgs {
    pub id: Uuid,
    pub name: String,
}

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct AgentProfileRightsArgs {
    pub id: Uuid,
    pub rights: Rights,
}

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct AgentAssignArgs {
    /// `agent:<id>` or `model:<id>`, as `agent_subjects_list` gives it.
    pub subject: String,
    pub profile_id: Uuid,
}

/// A profile with its rights and the subjects in it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct AgentProfileDetail {
    pub id: String,
    pub name: String,
    pub is_standard: bool,
    pub rights: Rights,
    pub members: Vec<AgentSubjectView>,
}

/// What deleting a profile did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct AgentProfileDeleted {
    /// The subjects that were in the profile and are in "Standard" now.
    #[ts(type = "number")]
    pub moved: usize,
}

fn refused(code: &str) -> HolziError {
    HolziError::InvalidInput {
        reason: code.to_owned(),
    }
}

fn named(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(refused("name_empty"));
    }
    Ok(name.to_owned())
}

fn changed<R: Runtime>(app: &AppHandle<R>) {
    let _ = app.emit(EVENT_AGENTS_CHANGED, ());
}

/// Every profile, "Standard" first, with how many agents it holds.
#[tauri::command]
pub async fn agent_profiles_list(state: State<'_, AppState>) -> Result<Vec<AgentProfileView>> {
    Ok(state
        .database()?
        .read(|q| subjects::profile_views(q))
        .await?)
}

/// One profile with its rights and members.
#[tauri::command]
pub async fn agent_profile_get(
    state: State<'_, AppState>,
    args: AgentProfileArgs,
) -> Result<AgentProfileDetail> {
    let id = args.id;
    let found = state
        .database()?
        .read(move |q| {
            let Some(profile) = subjects::profile_views(q)?
                .into_iter()
                .find(|profile| profile.id == id.to_string())
            else {
                return Ok(None);
            };
            let rights = resolve::profile_rights(q, id)?;
            let members = subjects::list_subjects(q)?
                .into_iter()
                .filter(|subject| subject.profile_id == id.to_string())
                .collect();
            Ok(Some(AgentProfileDetail {
                id: profile.id,
                name: profile.name,
                is_standard: profile.is_standard,
                rights,
                members,
            }))
        })
        .await?;
    found.ok_or_else(|| refused("profile_missing"))
}

/// A new profile that gives nothing until the user sets rights (FR-002). A name may repeat.
#[tauri::command]
pub async fn agent_profile_create<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    args: AgentProfileCreateArgs,
) -> Result<AgentProfileView> {
    let name = named(&args.name)?;
    let id = Uuid::new_v4();
    let stored = name.clone();
    state
        .database()?
        .write(move |tx| store::create_profile(tx, id, &stored).map_err(Into::into))
        .await?;
    changed(&app);
    Ok(AgentProfileView {
        id: id.to_string(),
        name,
        is_standard: false,
        member_count: 0,
    })
}

#[tauri::command]
pub async fn agent_profile_rename<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    args: AgentProfileRenameArgs,
) -> Result<()> {
    if args.id == STANDARD_PROFILE_ID {
        return Err(refused("standard_fixed"));
    }
    let name = named(&args.name)?;
    let id = args.id;
    let found = state
        .database()?
        .write(move |tx| {
            if store::profile(tx, id)?.is_none() {
                return Ok(false);
            }
            store::rename_profile(tx, id, &name)?;
            Ok(true)
        })
        .await?;
    if !found {
        return Err(refused("profile_missing"));
    }
    changed(&app);
    Ok(())
}

/// Deletes a profile; its members are in "Standard" afterwards, in the same write (FR-009).
#[tauri::command]
pub async fn agent_profile_delete<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    args: AgentProfileArgs,
) -> Result<AgentProfileDeleted> {
    if args.id == STANDARD_PROFILE_ID {
        return Err(refused("standard_fixed"));
    }
    let id = args.id;
    let moved = state
        .database()?
        .write(move |tx| store::delete_profile(tx, id).map_err(Into::into))
        .await?;
    changed(&app);
    Ok(AgentProfileDeleted { moved })
}

/// New rights for a profile; every agent in it uses them from its next call (FR-005).
#[tauri::command]
pub async fn agent_profile_rights_set<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    args: AgentProfileRightsArgs,
) -> Result<()> {
    grants::validate(&args.rights).map_err(|error| HolziError::InvalidInput {
        reason: error.to_string(),
    })?;
    let AgentProfileRightsArgs { id, rights } = args;
    let found = state
        .database()?
        .write(move |tx| {
            if !choices::exists(tx, id)? {
                return Ok(false);
            }
            store::set_rights(tx, id, &rights)?;
            Ok(true)
        })
        .await?;
    if !found {
        return Err(refused("profile_missing"));
    }
    changed(&app);
    Ok(())
}

/// Puts a subject into a profile; "Standard" takes it out of every other one.
#[tauri::command]
pub async fn agent_assign<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    args: AgentAssignArgs,
) -> Result<()> {
    let subject = Subject::parse(&args.subject).ok_or_else(|| refused("subject_unknown"))?;
    let profile = args.profile_id;
    let found = state
        .database()?
        .write(move |tx| {
            if !choices::exists(tx, profile)? {
                return Ok(false);
            }
            store::assign(tx, &subject, profile)?;
            Ok(true)
        })
        .await?;
    if !found {
        return Err(refused("profile_missing"));
    }
    changed(&app);
    Ok(())
}

/// The agents of every device and the models that are assigned, each with its profile.
#[tauri::command]
pub async fn agent_subjects_list(state: State<'_, AppState>) -> Result<Vec<AgentSubjectView>> {
    Ok(state
        .database()?
        .read(|q| subjects::list_subjects(q))
        .await?)
}
