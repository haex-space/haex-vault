//! The answers of holzi's own reading tools beyond mail, contacts, passwords and the file search
//! (spec 087 research R12, FR-016): the conversations, a file's details, the open window, the
//! installed models and this device, and empty sync rules, shares and agent tabs. An unknown id or
//! path is an error, as the real tool answers it, so that a model told "nothing here" can look
//! elsewhere.

use serde::Deserialize;
use serde_json::{json, Value};

use super::world::{error, file_json, ok, str_arg, Answer, FixtureWorld};
use crate::chat::eval::embedded_tools;
use crate::chat::tools::action_tool::ActionEffect;

/// A message as `chat_messages_list` shows it to an agent: no text.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationMessage {
    pub id: String,
    pub role: String,
    pub created_at: i64,
    pub status: Option<String>,
}

/// One of the user's own conversations with holzi. Conversations with contacts look absent to an
/// agent without spec 072's grant (FR-070), so the world holds none.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Conversation {
    pub id: String,
    pub title: String,
    pub created_at: i64,
    pub messages: Vec<ConversationMessage>,
}

/// The one tab open in the world: the chat the request is typed in.
const CHAT_TAB: &str = "tab-chat";

impl FixtureWorld {
    /// `None` for a tool this module does not answer.
    pub(super) fn answer_app(&self, tool: &str, input: &Value) -> Option<Answer> {
        let answer = match tool {
            "chat_conversations_list" => ok(
                json!({ "conversations": self.conversations.iter().map(|c| json!({
                "id": c.id, "title": c.title, "createdAt": c.created_at,
            })).collect::<Vec<_>>() }),
            ),
            "chat_messages_list" => match self
                .conversations
                .iter()
                .find(|c| str_arg(input, "threadId") == Some(c.id.as_str()))
            {
                Some(c) => ok(json!({ "messages": c.messages.iter().map(|m| json!({
                    "id": m.id, "role": m.role, "createdAt": m.created_at, "status": m.status,
                })).collect::<Vec<_>>() })),
                None => error("no conversation with this id"),
            },
            "files_sources" => ok(json!({
                "device": { "name": self.settings["deviceName"], "roots": ["/"] },
                "storages": [],
            })),
            "files_stat" => self.file_stat(input),
            "files_read" => match self.file_at(input) {
                Some(_) => error("no text could be read from this file"),
                None => error("no file at this path"),
            },
            "files_shares_list" | "files_shares_ofContact" => ok(json!({ "shares": [] })),
            "files_shares_of" => ok(json!({})),
            "syncRules_list" => ok(json!({ "rules": [] })),
            "syncRules_status" => error("no sync rule with this id"),
            "settings_models_list" => ok(json!({ "models": self.models })),
            "settings_models_checkUpdates" => ok(json!({ "updates": [] })),
            "settings_devices_list" => ok(json!({ "devices": [{
                "name": self.settings["deviceName"], "thisDevice": true, "role": "main", "online": true,
            }] })),
            "settings_devices_identity" => ok(json!({ "npub": "npub1fixture", "hex": "00" })),
            "settings_appearance_export" => {
                ok(json!({ "colorScheme": self.settings["colorScheme"] }))
            }
            "wm_state_get" => ok(json!({ "workspaces": [{
                "id": "ws-1", "active": true,
                "windows": [{
                    "id": "win-chat", "app": "system.chat", "title": "Chat", "focused": true,
                    "minimized": false, "maximized": false,
                    "tabs": [{ "id": CHAT_TAB, "location": "/", "title": "Chat", "attention": false }],
                }],
            }] })),
            "wm_tab_history" => match str_arg(input, "tabId") {
                Some(CHAT_TAB) => {
                    ok(json!({ "entries": [{ "location": "/", "title": "Chat" }], "index": 0 }))
                }
                _ => error("no tab with this id"),
            },
            "wm_actions_list" => ok(json!({ "actions": embedded_tools().iter().map(|t| json!({
                "id": t.action_id, "description": t.description, "effect": effect_name(t.effect),
            })).collect::<Vec<_>>() })),
            "wm_app_open" => match self.app_named(str_arg(input, "appId").unwrap_or_default()) {
                Some(_) => ok(json!({ "ok": true })),
                None => error("no app with this name; wm_apps_list lists the apps"),
            },
            "browser_tabs_list" => ok(json!({ "tabs": [] })),
            "browser_page_read"
            | "browser_page_screenshot"
            | "browser_scroll"
            | "browser_wait"
            | "browser_takeover_request" => error("no agent tab with this name"),
            _ => return None,
        };
        Some(answer)
    }

    /// The app `name` opens, as the app's own matching decides it in short (`src/lib/wm/appMatch.ts`):
    /// its id, its title or the word of a holzi id ("passwords"), ignoring case, or a name that holds
    /// one of those ("haex-chat"). Where the app would offer a choice, the world has none to offer.
    fn app_named(&self, name: &str) -> Option<&super::world::App> {
        let query = name.trim().to_lowercase();
        if query.is_empty() {
            return None;
        }
        let names = |app: &super::world::App| {
            let mut names = vec![app.id.to_lowercase(), app.title.to_lowercase()];
            if let Some(rest) = app.id.strip_prefix("system.") {
                names.push(rest.to_lowercase());
            }
            names
        };
        self.apps
            .iter()
            .find(|app| names(app).contains(&query))
            .or_else(|| {
                self.apps
                    .iter()
                    .find(|app| names(app).iter().any(|n| query.contains(n.as_str())))
            })
    }

    fn file_at(&self, input: &Value) -> Option<&super::world::FileEntry> {
        let path = str_arg(input, "path")?;
        self.files.iter().find(|f| f.path == path)
    }

    fn file_stat(&self, input: &Value) -> Answer {
        // The world has this device only (`files_sources` lists no storage).
        if str_arg(input, "source").is_some_and(|source| source != "device") {
            return error("no storage with this name or id");
        }
        if let Some(file) = self.file_at(input) {
            return ok(file_json(file));
        }
        let path = str_arg(input, "path")
            .unwrap_or_default()
            .trim_end_matches('/');
        if path.is_empty() {
            return ok(json!({ "name": "/", "path": "/", "kind": "dir" }));
        }
        let below = format!("{path}/");
        if self.files.iter().any(|f| f.path.starts_with(&below)) {
            let name = path.rsplit('/').next().unwrap_or(path);
            return ok(json!({ "name": name, "path": path, "kind": "dir" }));
        }
        error("no file or folder at this path")
    }
}

fn effect_name(effect: ActionEffect) -> &'static str {
    match effect {
        ActionEffect::Read => "read",
        ActionEffect::Write => "write",
        ActionEffect::Destructive => "destructive",
    }
}
