//! What counts as foreign content in a conversation (spec 097 FR-019, research R6): what neither the
//! user nor haex-vault wrote. The list names what is own and treats everything else as foreign, so a
//! new tool asks once too often, never once too rarely.

use std::cell::Cell;
use std::collections::HashMap;
use std::sync::Mutex;

use serde_json::Value;
use uuid::Uuid;

use crate::chat::tools::ask_user::ASK_USER_TOOL_NAME;
use crate::chat::tools::find_actions::FIND_ACTIONS_TOOL_NAME;
use crate::chat::tools::ToolRegistry;
use crate::storage::chat_messages::{ChatMessage, MessageRole};
use crate::storage::chat_threads;
use crate::vault_gate::VaultDb;

tokio::task_local! {
    pub(crate) static CONTACT_SHARE: Cell<bool>;
}

#[cfg(test)]
#[path = "foreign_tests.rs"]
mod tests;

/// The source of haex-vault's own actions (`chat_messages.tool_source`).
const ACTION_SOURCE: &str = "action";

/// Scopes of haex-vault's own actions whose results are the user's or haex-vault's own data.
const OWN_SCOPES: [&str; 5] = [
    "passwords.read",
    "files.read",
    "files.write",
    "files.syncRules",
    "chat.write",
];

/// Scope families that are own as a whole.
const OWN_SCOPE_PREFIXES: [&str; 4] = ["wm.", "settings.", "contacts.", "memory."];

/// Whether a call of a tool from `source` in `scope` brought foreign content into the
/// conversation. Every tool of an extension, a connected server or the command line is foreign, and
/// so are the scopes `browser`, `chat.read`, `files.shares` and any this build does not know.
pub fn is_foreign(
    source: &str,
    scope: Option<&str>,
    tool: &str,
    contact_share: bool,
    result: &str,
) -> bool {
    if source != ACTION_SOURCE {
        return true;
    }
    let Some(scope) = scope else {
        return match tool {
            ASK_USER_TOOL_NAME => false,
            FIND_ACTIONS_TOOL_NAME => search_result_has_extension(result),
            _ => true,
        };
    };
    let own = OWN_SCOPES.contains(&scope)
        || OWN_SCOPE_PREFIXES
            .iter()
            .any(|prefix| scope.starts_with(prefix));
    !own || contact_share
}

/// The search can include descriptions and schemas supplied by extensions.
fn search_result_has_extension(result: &str) -> bool {
    let Ok(result) = serde_json::from_str::<Value>(result) else {
        return false;
    };
    result["actions"].as_array().is_some_and(|actions| {
        actions.iter().any(|action| {
            action["tool"]
                .as_str()
                .and_then(crate::chat::tools::domains::extension_prefix)
                .is_some()
        })
    })
}

/// Older conversations can already have tool results written before they had a foreign mark.
pub(crate) async fn recover_mark(
    db: &VaultDb,
    registry: &Mutex<ToolRegistry>,
    thread_id: Uuid,
    history: &[ChatMessage],
) -> crate::error::Result<()> {
    if db
        .read(move |q| chat_threads::memory_foreign_at(q, thread_id))
        .await?
        .is_some()
    {
        return Ok(());
    }
    let foreign = {
        let results: HashMap<_, _> = history
            .iter()
            .filter(|row| row.role == MessageRole::ToolResult && row.tool_is_error != Some(true))
            .filter_map(|row| Some((row.parent_id?, row.content.as_str())))
            .collect();
        let registry = registry.lock().unwrap_or_else(|e| e.into_inner());
        history
            .iter()
            .filter(|row| row.role == MessageRole::ToolCall)
            .any(|row| {
                let Some(result) = results.get(&row.id) else {
                    return false;
                };
                let name = row.tool_name.as_deref().unwrap_or_default();
                let scope = registry
                    .get(name)
                    .and_then(|tool| tool.action_definition().map(|def| def.scope.clone()));
                let contact_share = scope
                    .as_deref()
                    .is_some_and(|scope| scope.starts_with("files."))
                    && row
                        .tool_input
                        .as_deref()
                        .and_then(|input| serde_json::from_str(input).ok())
                        .as_ref()
                        .is_some_and(names_remote_storage);
                is_foreign(
                    row.tool_source.as_deref().unwrap_or("unknown"),
                    scope.as_deref(),
                    name,
                    contact_share,
                    result,
                )
            })
    };
    if foreign {
        let now = crate::passwords::clock::unix_millis(std::time::SystemTime::now());
        db.write(move |tx| chat_threads::mark_foreign_once(tx, thread_id, now))
            .await?;
    }
    Ok(())
}

/// Legacy tool rows contain the requested name but omit which storage it resolved to.
pub(crate) fn names_remote_storage(input: &Value) -> bool {
    match input {
        Value::Object(map) => map.iter().any(|(key, value)| {
            (matches!(key.as_str(), "source" | "toSource")
                && value.as_str().is_some_and(|id| id.starts_with("storage:")))
                || names_remote_storage(value)
        }),
        Value::Array(items) => items.iter().any(names_remote_storage),
        _ => false,
    }
}
