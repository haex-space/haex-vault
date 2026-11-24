//! What counts as foreign content in a conversation (spec 097 FR-019, research R6): what neither the
//! user nor haex-vault wrote. The list names what is own and treats everything else as foreign, so a
//! new tool asks once too often, never once too rarely.

use std::cell::Cell;

use serde_json::Value;

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
pub fn is_foreign(source: &str, scope: Option<&str>, tool: &str, contact_share: bool) -> bool {
    if source != ACTION_SOURCE {
        return true;
    }
    let Some(scope) = scope else {
        return !matches!(
            tool,
            crate::chat::tools::find_actions::FIND_ACTIONS_TOOL_NAME
                | crate::chat::tools::ask_user::ASK_USER_TOOL_NAME
        );
    };
    let own = OWN_SCOPES.contains(&scope)
        || OWN_SCOPE_PREFIXES
            .iter()
            .any(|prefix| scope.starts_with(prefix));
    !own || contact_share
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
