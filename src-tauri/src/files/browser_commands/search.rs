//! Search by name (spec 044 US4).

use tauri::ipc::Channel;
use tauri::{AppHandle, State};

use super::{device_path, storage_of, window_storage};
use crate::files::access::Want;
use crate::files::search::{
    FilesSearchEvent, SearchFilters, SearchLimits, SearchManager, SearchOptions,
};
use crate::files::state::FilesState;
use crate::files::{FilesError, SourceRef};
use crate::state::AppState;

/// Searches the folder `path` and below by name (FR-027 to FR-030); hits come through `channel`.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn files_search_start(
    app: AppHandle,
    state: State<'_, AppState>,
    files: State<'_, FilesState>,
    searches: State<'_, SearchManager>,
    source: SourceRef,
    path: String,
    query: String,
    filters: SearchFilters,
    show_hidden: bool,
    channel: Channel<FilesSearchEvent>,
) -> Result<String, FilesError> {
    let options = SearchOptions {
        filters,
        show_hidden,
        own: files.own.clone(),
        hide_own: false,
        limits: SearchLimits::USER,
    };
    if let Some(id) = storage_of(&source) {
        let storage = window_storage(&app, &state, &files, id).await?;
        return searches.start_storage(state.gate(), storage, path, query, options, channel);
    }
    let root = device_path(&files, &source, &path, Want::Read)?;
    searches.start(state.gate(), root, query, options, channel)
}

/// Ends a search; the window calls it when the query changes or the tab goes away (FR-028).
#[tauri::command]
pub fn files_search_cancel(searches: State<'_, SearchManager>, search_id: String) {
    searches.cancel(&search_id);
}
