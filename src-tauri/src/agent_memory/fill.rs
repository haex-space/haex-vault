//! Pages for the hand test of a large memory (spec 097 quickstart step 12, T063): a debug build
//! writes `count` generated pages, so a tester sees the cut table of contents and the search
//! without writing hundreds by hand. A release build refuses the command.

use haex_crdt::CrdtTransaction;
use tauri::{AppHandle, Runtime, State};

use super::limits::MAX_PAGES;
use super::store::{self, MemoryError, NewPage};
use super::Author;
use crate::error::{HaexError, Result};
use crate::state::AppState;

/// Writes up to `count` pages titled "Testseite 0001" onwards, skipping titles that exist, and
/// stops at the most pages a memory holds. Returns how many it wrote.
pub fn fill_in(
    tx: &mut CrdtTransaction<'_>,
    author: &Author,
    now: i64,
    count: usize,
) -> std::result::Result<usize, MemoryError> {
    let mut written = 0;
    let mut number = 0;
    while written < count && store::count(tx)? < MAX_PAGES {
        number += 1;
        let title = format!("Testseite {number:04}");
        if store::by_title(tx, &title)?.is_some() {
            continue;
        }
        let body = format!(
            "Eine erzeugte Seite für den Test eines großen Gedächtnisses, Nummer {number}. \
             Sie verweist auf [[Testseite {next:04}]].",
            next = number + 1
        );
        let new = NewPage {
            title: &title,
            description: "Erzeugt für den Test vieler Seiten",
            body: &body,
        };
        store::add_page(tx, author, now + number as i64, &new)?;
        written += 1;
    }
    Ok(written)
}

/// Writes `count` generated pages as the user. Debug builds only.
#[tauri::command]
pub async fn agent_memory_debug_fill<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    count: usize,
) -> Result<usize> {
    if !cfg!(debug_assertions) {
        return Err(HaexError::InvalidInput {
            reason: "generated pages exist only in debug builds".to_string(),
        });
    }
    let db = state.database()?;
    let author = Author::user(crate::storage::wm_session_commands::current_device_uuid(
        &app, &db,
    )?);
    let now = crate::passwords::clock::unix_millis(std::time::SystemTime::now());
    let written = db
        .write(move |tx| store::settle(fill_in(tx, &author, now, count)))
        .await?;
    written.map_err(|error| HaexError::InvalidInput {
        reason: error.reason(),
    })
}

#[cfg(test)]
#[path = "fill_tests.rs"]
mod tests;
