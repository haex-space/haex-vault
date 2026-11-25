//! Commands for folders, moving and tags (spec 034, US2, `contracts/tauri-commands.md` §Schreiben:
//! Ordnung). Each calls only the service as `Caller::User`.

use serde::{Deserialize, Serialize};
use tauri::State;
use ts_rs::TS;

use super::service;
use crate::error::Result;
use crate::passwords::access::Caller;
use crate::passwords::model::{GroupPatch, Target};
use crate::state::AppState;

#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/", optional_fields)]
#[serde(rename_all = "camelCase")]
pub struct CreateGroupArgs {
    pub name: String,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub parent_id: Option<String>,
}

#[derive(Debug, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct CreateGroupResult {
    pub group_id: String,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct UpdateGroupArgs {
    pub group_id: String,
    pub patch: GroupPatch,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct ReorderGroupsArgs {
    /// The folder whose children are reordered; `null` is the top level.
    pub parent_id: Option<String>,
    /// All siblings of that level, in the new order.
    pub ordered_ids: Vec<String>,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct MoveArgs {
    pub targets: Vec<Target>,
    /// The folder to move into; `null` is the top level.
    pub to_group_id: Option<String>,
}

#[derive(Debug, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct MoveResult {
    pub moved: u32,
}

#[derive(Debug, Default, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/", optional_fields)]
#[serde(rename_all = "camelCase", default)]
pub struct SetTagsArgs {
    pub item_ids: Vec<String>,
    pub add: Vec<String>,
    pub remove: Vec<String>,
}

#[derive(Debug, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct SetTagsResult {
    pub changed: u32,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct RenameTagArgs {
    pub tag_ids: Vec<String>,
    pub name: String,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct SetTagColorArgs {
    pub tag_ids: Vec<String>,
    pub color: Option<String>,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct TagIdArgs {
    pub tag_id: String,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/", optional_fields)]
#[serde(rename_all = "camelCase")]
pub struct TagIdsArgs {
    pub tag_ids: Vec<String>,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct MergeTagsArgs {
    pub source_ids: Vec<String>,
    pub target_ids: Vec<String>,
    /// The name of the merged tag; `None` keeps the name of the target.
    pub name: Option<String>,
}

#[tauri::command]
pub async fn passwords_create_group(
    state: State<'_, AppState>,
    args: CreateGroupArgs,
) -> Result<CreateGroupResult> {
    let group_id = service(&state)?
        .create_group(
            &Caller::User,
            args.name,
            args.description,
            args.icon,
            args.color,
            args.parent_id,
        )
        .await?;
    Ok(CreateGroupResult { group_id })
}

#[tauri::command]
pub async fn passwords_update_group(
    state: State<'_, AppState>,
    args: UpdateGroupArgs,
) -> Result<()> {
    service(&state)?
        .update_group(&Caller::User, args.group_id, args.patch)
        .await
}

#[tauri::command]
pub async fn passwords_reorder_groups(
    state: State<'_, AppState>,
    args: ReorderGroupsArgs,
) -> Result<()> {
    service(&state)?
        .reorder_groups(&Caller::User, args.parent_id, args.ordered_ids)
        .await
}

#[tauri::command]
pub async fn passwords_move(state: State<'_, AppState>, args: MoveArgs) -> Result<MoveResult> {
    let moved = service(&state)?
        .move_targets(&Caller::User, args.targets, args.to_group_id)
        .await?;
    Ok(MoveResult { moved })
}

#[tauri::command]
pub async fn passwords_set_tags(
    state: State<'_, AppState>,
    args: SetTagsArgs,
) -> Result<SetTagsResult> {
    let changed = service(&state)?
        .set_tags(&Caller::User, args.item_ids, args.add, args.remove)
        .await?;
    Ok(SetTagsResult { changed })
}

#[tauri::command]
pub async fn passwords_rename_tag(state: State<'_, AppState>, args: RenameTagArgs) -> Result<()> {
    service(&state)?
        .rename_tag(&Caller::User, args.tag_ids, args.name)
        .await
}

#[tauri::command]
pub async fn passwords_set_tag_color(
    state: State<'_, AppState>,
    args: SetTagColorArgs,
) -> Result<()> {
    service(&state)?
        .set_tag_color(&Caller::User, args.tag_ids, args.color)
        .await
}

#[tauri::command]
pub async fn passwords_delete_tag(state: State<'_, AppState>, args: TagIdsArgs) -> Result<()> {
    service(&state)?
        .delete_tag(&Caller::User, args.tag_ids)
        .await
}

#[tauri::command]
pub async fn passwords_merge_tags(state: State<'_, AppState>, args: MergeTagsArgs) -> Result<()> {
    service(&state)?
        .merge_tags(&Caller::User, args.source_ids, args.target_ids, args.name)
        .await
}

#[tauri::command]
pub async fn passwords_tag_agent_profiles(
    state: State<'_, AppState>,
    args: TagIdArgs,
) -> Result<Vec<String>> {
    service(&state)?
        .tag_agent_profiles(&Caller::User, args.tag_id)
        .await
}
