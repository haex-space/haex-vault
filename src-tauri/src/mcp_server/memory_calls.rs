//! The memory in an Abo-Agent's calls (spec 097 FR-018, FR-019, FR-021, research R7, T071): a write
//! of the memory runs by the same rule as the built-in agent's, judged by the mark of the
//! delegate's chat thread after the reader of its output has caught up; a call of a tool that
//! brings foreign content marks that thread; and an approved write is bound to the page the
//! question showed. An agent outside haex-vault never gets here: its memory right is off
//! (`registry::load_rights`).

use std::cell::Cell;

use rmcp::model::CallToolResult;
use serde_json::{json, Value};
use uuid::Uuid;

use super::agent_log::Outcome;
use super::callee::Callee;
use super::grants::MEMORY_WRITE_SCOPE;
use super::results::error_result;
use super::server::Core;
use crate::agent_memory::foreign::{is_foreign, CONTACT_SHARE};
use crate::agent_memory::store::{self, Page};
use crate::agent_memory::{gate, watch};
use crate::chat::tools::permission::{self, Decision};
use crate::storage::chat_threads;

/// Whether `callee` writes the memory.
pub(super) fn writes_memory(callee: Callee<'_>) -> bool {
    matches!(callee, Callee::Action(def) if def.scope == MEMORY_WRITE_SCOPE)
}

/// The verdict on a write of the memory by a delegate in `thread`: the chat's mode and the mark of
/// the thread, as for the built-in agent. A delegate without a thread, a reader that does not catch
/// up, or a mark that cannot be read counts as foreign, so the write asks.
pub(super) async fn decide(core: &Core, thread: Option<Uuid>) -> Decision {
    let mode = permission::read_from_vault(&core.db).await;
    gate::decide(mode, foreign(core, thread).await).into()
}

async fn foreign(core: &Core, thread: Option<Uuid>) -> bool {
    let Some(thread) = thread else {
        return true;
    };
    if watch::retry_unrecorded(&core.db, thread).await {
        return true;
    }
    if !watch::fence(thread, watch::FENCE_WAIT).await {
        return true;
    }
    match core
        .db
        .read(move |q| chat_threads::memory_foreign_at(q, thread))
        .await
    {
        Ok(at) => at.is_some(),
        Err(error) => {
            log::warn!("mcp server: the mark of foreign content could not be read: {error}");
            true
        }
    }
}

/// The page a write names as it is stored now, for the question and to bind the answer to it
/// (FR-021): `Ok(None)` for a page that does not exist yet.
pub(super) async fn snapshot(
    core: &Core,
    input: &Value,
) -> Result<Option<Page>, Box<CallToolResult>> {
    let Some(title) = input["title"].as_str().map(str::to_owned) else {
        return Err(Box::new(error_result(
            "invalid_input",
            "The write names no title. Nothing was written.",
            Some(("field", json!("title"))),
        )));
    };
    core.db
        .read(move |q| Ok(store::by_title(q, &title)?.map(|found| found.page)))
        .await
        .map_err(|error| {
            log::warn!("mcp server: the memory page could not be read: {error}");
            Box::new(error_result(
                "unavailable",
                "The page could not be read for approval. Nothing was written.",
                None,
            ))
        })
}

/// The stored page as the question shows it: `null` for a new page.
pub(super) fn shown(page: &Option<Page>) -> Value {
    page.as_ref().map_or(
        Value::Null,
        |page| json!({ "description": page.description, "body": page.body }),
    )
}

/// Runs `work` bound to the page the user approved, if any: a write finds another page stored by
/// then and is refused (`page_changed`).
pub(super) async fn bound<F: std::future::Future>(
    approved: Option<Option<Page>>,
    work: F,
) -> F::Output {
    match approved {
        Some(page) => gate::APPROVED_PAGE.scope(page, work).await,
        None => work.await,
    }
}

/// Runs a call and tells whether a file action reached a share of a contact on the way.
pub(super) async fn tracing_shares<F: std::future::Future>(work: F) -> (F::Output, bool) {
    CONTACT_SHARE
        .scope(Cell::new(false), async {
            let output = work.await;
            (output, CONTACT_SHARE.with(Cell::get))
        })
        .await
}

/// Marks the delegate's thread after a successful call that brought foreign content.
pub(super) async fn mark_after(
    core: &Core,
    thread: Option<Uuid>,
    callee: Callee<'_>,
    tool: &str,
    outcome: Outcome,
    contact_share: bool,
) {
    let Some(thread) = thread else {
        return;
    };
    if outcome != Outcome::Ok {
        return;
    }
    let (source, scope) = match callee {
        Callee::Action(def) => ("action", Some(def.scope.as_str())),
        Callee::Extension(_) => ("haextension", None),
        Callee::Command(_) => ("cli", None),
    };
    if !is_foreign(source, scope, tool, contact_share, "") {
        return;
    }
    let _ = watch::persist_foreign_mark(&core.db, thread).await;
}
