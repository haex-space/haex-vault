//! What a turn gets of the memory (spec 097 FR-010, FR-011, FR-015, FR-017, research R5): the
//! conversation's table of contents, taken once when its first turn starts, and the block built from
//! it for the turn's model. The tools of the memory are offered by the rights alone
//! (`profile_gate::retain_permitted`); this decides only the text of the system prompt.

use haex_crdt::CrdtTransaction;
use uuid::Uuid;

use super::store;
use super::toc::{block, gives_start_toc, render};
use crate::mcp_server::grants::MemoryLevel;
use crate::storage::chat_threads::{memory_toc, take_memory_toc};

#[cfg(test)]
#[path = "turn_tests.rs"]
mod tests;

/// Takes the conversation's table of contents unless it has one, and gives the block for a model
/// with `level`, the context `context_tokens` and the subject key `model`: `None` for a model that
/// may not read or gets no block at the start (FR-027). The snapshot is taken whatever the level,
/// so a model that may read later in the conversation gets the one from its start.
pub fn prepare(
    tx: &mut CrdtTransaction<'_>,
    thread: Uuid,
    level: MemoryLevel,
    context_tokens: Option<usize>,
    model: Option<&str>,
) -> haex_crdt::Result<Option<String>> {
    let snapshot = match memory_toc(tx, thread)? {
        Some(snapshot) => snapshot,
        None => {
            let rendered = render(&store::list(tx)?);
            take_memory_toc(tx, thread, &rendered)?.unwrap_or(rendered)
        }
    };
    if level.is_off() || model.is_some_and(|model| !gives_start_toc(model)) {
        return Ok(None);
    }
    Ok(Some(block(
        &snapshot,
        context_tokens,
        level == MemoryLevel::ReadWrite,
    )))
}

/// The memory block of the turn (spec 097 research R5): the conversation's table of contents for a
/// model whose profile lets it read the memory. An Abo-Agent gets it with spec 097 PR C. A failure
/// leaves the block out rather than the turn.
pub async fn block_for_session(
    db: &crate::vault_gate::VaultDb,
    session: &crate::chat::session::ActiveSession,
    thread_id: Uuid,
) -> Option<String> {
    if session.provider_kind == crate::storage::providers::ProviderKind::CliDelegate {
        return None;
    }
    let level = crate::chat::tools::profile_gate::rights_of_session(db, session)
        .await
        .memory;
    let context = session.adapter.context_limit().or_else(|| {
        session
            .context_window
            .and_then(|window| usize::try_from(window).ok())
    });
    let model = crate::agent_profiles::local::subject_of_session(session).map(|s| s.key());
    match db
        .write(move |tx| prepare(tx, thread_id, level, context, model.as_deref()))
        .await
    {
        Ok(block) => block,
        Err(error) => {
            log::warn!("chat: the memory block was left out: {error}");
            None
        }
    }
}
