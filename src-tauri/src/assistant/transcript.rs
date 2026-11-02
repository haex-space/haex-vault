//! Reading the transcript of a spoken conversation (spec 066 FR-048, contracts/tauri-commands.md
//! `assistant_transcript_list`). Compiled in every build: transcripts sync to devices without the
//! `voice` feature too.

use tauri::State;
use uuid::Uuid;

use crate::error::Result;
use crate::state::AppState;
use crate::storage::voice_transcript::{self, TranscriptEntry};

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
