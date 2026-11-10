//! Reading and searching the transcripts of spoken conversations (spec 066 FR-048,
//! contracts/tauri-commands.md `assistant_transcript_list`, `assistant_transcript_search`).
//! Compiled in every build: transcripts sync to devices without the `voice` feature too.

use tauri::State;
use uuid::Uuid;

use crate::error::{HolziError, Result};
use crate::state::AppState;
use crate::storage::voice_transcript::{self, TranscriptEntry};
use crate::storage::voice_transcript_search::{self, SearchPage, SearchRequest};

/// The stored transcript of `thread_id`, ordered by `(seq, id)`.
#[tauri::command]
pub async fn assistant_transcript_list(
    state: State<'_, AppState>,
    thread_id: Uuid,
) -> Result<Vec<TranscriptEntry>> {
    let db = crate::state_utils::active_database(&state)?;
    db.read(move |r| voice_transcript::list_for_thread(r, thread_id))
        .await
}

/// The entries of every transcript that contain `query`, a page at a time (T127).
#[tauri::command]
pub async fn assistant_transcript_search(
    state: State<'_, AppState>,
    query: String,
    cursor: Option<String>,
    limit: Option<u32>,
) -> Result<SearchPage> {
    let request = SearchRequest::new(&query, cursor.as_deref(), limit)
        .map_err(|reason| HolziError::InvalidInput { reason })?;
    let db = crate::state_utils::active_database(&state)?;
    db.read(move |r| voice_transcript_search::search(r, &request))
        .await
}
