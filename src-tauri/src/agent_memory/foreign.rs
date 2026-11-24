//! What counts as foreign content in a conversation (spec 097 FR-019, research R6): what neither the
//! user nor haex-vault wrote. The list names what is own and treats everything else as foreign, so a
//! new tool asks once too often, never once too rarely. A call that failed returned nothing.

use serde_json::Value;

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

/// The start of the storage id of a share a contact gave the user: its files are foreign.
const RECEIVED_SHARE_PREFIX: &str = "share:";

/// Whether a call of a tool from `source` in `scope` with `input` brought foreign content into the
/// conversation. Every tool of an extension, a connected server or the command line is foreign, and
/// so are the scopes `browser`, `chat.read`, `files.shares` and any this build does not know.
pub fn is_foreign(source: &str, scope: Option<&str>, input: &Value) -> bool {
    if source != ACTION_SOURCE {
        return true;
    }
    let Some(scope) = scope else {
        return true;
    };
    let own = OWN_SCOPES.contains(&scope)
        || OWN_SCOPE_PREFIXES
            .iter()
            .any(|prefix| scope.starts_with(prefix));
    !own || (scope.starts_with("files.") && names_received_share(input))
}

/// Whether `input` names a storage that is a share received from a contact, anywhere in it.
fn names_received_share(input: &Value) -> bool {
    match input {
        Value::Object(map) => map.iter().any(|(key, value)| {
            (key == "storageId"
                && value
                    .as_str()
                    .is_some_and(|id| id.starts_with(RECEIVED_SHARE_PREFIX)))
                || names_received_share(value)
        }),
        Value::Array(items) => items.iter().any(names_received_share),
        _ => false,
    }
}
