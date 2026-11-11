//! The fixture world the everyday measurement answers tool calls from (spec 078 FR-005, research R4):
//! a fixed clock, a small mailbox, contacts, password entries without secrets, files, apps and
//! holzi's conversations (`world_app.rs` answers holzi's other reading tools). It
//! answers in the shape of the real tools, limited to the fields the set needs; no real data is read
//! and no action runs.

use serde::Deserialize;
use serde_json::{json, Value};

use super::world_app::Conversation;
use crate::local_time::{Civil, Clock};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorldClock {
    /// `YYYY-MM-DD`
    pub date: String,
    /// `HH:MM`
    pub time: String,
    pub utc_offset_minutes: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Mailbox {
    pub id: String,
    pub role: String,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Account {
    pub id: String,
    pub email: String,
    pub name: String,
    pub mailboxes: Vec<Mailbox>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub id: String,
    pub account: String,
    pub mailbox: String,
    pub from: String,
    pub to: String,
    pub subject: String,
    /// RFC 3339 with offset
    pub date: String,
    pub unread: bool,
    pub flagged: bool,
    pub has_attachment: bool,
    pub body: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Rule {
    pub id: String,
    pub account: String,
    pub name: String,
    pub conditions: Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Contact {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub organization: Option<String>,
    pub phones: Vec<Value>,
    pub emails: Vec<String>,
    #[serde(default)]
    pub addresses: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PasswordEntry {
    pub id: String,
    pub title: String,
    pub tags: Vec<String>,
    pub folder: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FileEntry {
    pub path: String,
    pub name: String,
    pub size: u64,
    pub modified: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct App {
    pub id: String,
    pub title: String,
}

/// The whole fixture world (data-model.md).
#[derive(Debug, Clone, Deserialize)]
pub struct FixtureWorld {
    pub clock: WorldClock,
    pub accounts: Vec<Account>,
    pub messages: Vec<Message>,
    pub rules: Vec<Rule>,
    pub contacts: Vec<Contact>,
    pub passwords: Vec<PasswordEntry>,
    pub files: Vec<FileEntry>,
    pub apps: Vec<App>,
    /// What `settings_get` returns.
    pub settings: Value,
    /// The user's own conversations with holzi (spec 087 R12).
    pub conversations: Vec<Conversation>,
    /// What `settings_models_list` returns.
    pub models: Vec<Value>,
}

/// What a tool answered: its content and whether it is an error.
#[derive(Debug, Clone, PartialEq)]
pub struct Answer {
    pub content: String,
    pub is_error: bool,
}

pub(super) fn ok(value: Value) -> Answer {
    Answer {
        content: value.to_string(),
        is_error: false,
    }
}

pub(super) fn error(message: impl Into<String>) -> Answer {
    Answer {
        content: json!({ "error": message.into() }).to_string(),
        is_error: true,
    }
}

fn has(haystack: &str, needle: &str) -> bool {
    haystack.to_lowercase().contains(&needle.to_lowercase())
}

pub(super) fn str_arg<'a>(input: &'a Value, key: &str) -> Option<&'a str> {
    input.get(key).and_then(Value::as_str)
}

fn bool_arg(input: &Value, key: &str) -> Option<bool> {
    input.get(key).and_then(Value::as_bool)
}

fn ids_arg(input: &Value, key: &str) -> Vec<String> {
    input
        .get(key)
        .and_then(Value::as_array)
        .map(|ids| {
            ids.iter()
                .filter_map(|id| id.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

impl WorldClock {
    fn civil(&self) -> Civil {
        let number = |s: &str| s.parse::<u32>().expect("a number in world.json's clock");
        let date: Vec<&str> = self.date.split('-').collect();
        let time: Vec<&str> = self.time.split(':').collect();
        Civil::new(
            date[0].parse().expect("a year in world.json's clock"),
            number(date[1]),
            number(date[2]),
            number(time[0]),
            number(time[1]),
        )
    }
}

impl FixtureWorld {
    /// The fixed clock the measurement passes to the model.
    pub fn clock(&self) -> Clock {
        Clock {
            now: self.clock.civil(),
            utc_offset_minutes: self.clock.utc_offset_minutes,
        }
    }

    /// The day `offset` days from the clock, as `YYYY-MM-DD`.
    pub fn day(&self, offset: i64) -> String {
        let day = Civil::from_days(self.clock.civil().days() + offset, 0, 0);
        format!("{:04}-{:02}-{:02}", day.year, day.month, day.day)
    }

    fn mailbox(&self, account: &str, id: &str) -> Option<&Mailbox> {
        self.accounts
            .iter()
            .find(|a| a.id == account)
            .and_then(|a| a.mailboxes.iter().find(|m| m.id == id))
    }

    fn message(&self, id: &str) -> Option<&Message> {
        self.messages.iter().find(|m| m.id == id)
    }

    /// Answers one tool call (contracts/everyday-format.md "Tool results").
    pub fn answer(&self, tool: &str, input: &Value) -> Answer {
        match tool {
            "x_mail_find_messages" => self.find_messages(input),
            "x_mail_get_message" => self.get_message(input),
            "x_mail_list_accounts" => ok(json!({ "accounts": self.accounts.iter().map(|a| json!({
                "id": a.id, "email": a.email, "name": a.name,
            })).collect::<Vec<_>>() })),
            "x_mail_list_mailboxes" => self.list_mailboxes(input),
            "x_mail_list_rules" => ok(json!({ "rules": self.rules.iter().map(|r| json!({
                "id": r.id, "account": r.account, "name": r.name, "conditions": r.conditions,
            })).collect::<Vec<_>>() })),
            "x_mail_create_draft" => self.create_draft(input),
            "x_mail_move_messages" => self.each_message(input, "ids", "moved"),
            "x_mail_mark_messages" => self.each_message(input, "ids", "marked"),
            "x_mail_delete_messages" => {
                let ids: Vec<String> = input
                    .get("messages")
                    .and_then(Value::as_array)
                    .map(|ms| {
                        ms.iter()
                            .filter_map(|m| m.get("id").and_then(Value::as_str).map(str::to_owned))
                            .collect()
                    })
                    .unwrap_or_default();
                self.known_ids(&ids, "deleted")
            }
            "x_mail_send_mail" => ok(json!({ "sent": true })),
            "x_mail_remove_rule" => {
                let found = self.rules.iter().any(|r| {
                    str_arg(input, "id") == Some(r.id.as_str())
                        || str_arg(input, "name").is_some_and(|n| has(&r.name, n))
                });
                if found {
                    ok(json!({ "removed": true }))
                } else {
                    error("no rule with this id or name")
                }
            }
            "contacts_search" => self.contacts_search(input),
            "contacts_get" => match self
                .contacts
                .iter()
                .find(|c| str_arg(input, "id") == Some(c.id.as_str()))
            {
                Some(c) => ok(contact_json(c)),
                None => error("no contact with this id"),
            },
            "passwords_items_search" => {
                let query = str_arg(input, "query").unwrap_or_default();
                let words: Vec<&str> = query.split_whitespace().collect();
                let items: Vec<Value> = self
                    .passwords
                    .iter()
                    .filter(|p| {
                        let text = format!("{} {} {}", p.title, p.tags.join(" "), p.folder);
                        words.iter().all(|w| has(&text, w))
                    })
                    .map(|p| json!({ "id": p.id, "title": p.title, "tags": p.tags, "folder": p.folder, "hasTotp": false }))
                    .collect();
                ok(json!({ "items": items }))
            }
            "files_search" => {
                // Every word, in any order: the real search matches names loosely too.
                let query = str_arg(input, "query").unwrap_or_default();
                let words: Vec<&str> = query.split_whitespace().collect();
                let hits: Vec<Value> = self
                    .files
                    .iter()
                    .filter(|f| words.iter().all(|w| has(&f.path, w)))
                    .map(file_json)
                    .collect();
                ok(json!({ "hits": hits }))
            }
            "files_list" => {
                let path = str_arg(input, "path").unwrap_or("/");
                let entries: Vec<Value> = self
                    .files
                    .iter()
                    .filter(|f| f.path.starts_with(path))
                    .map(file_json)
                    .collect();
                ok(json!({ "entries": entries, "truncated": false }))
            }
            "wm_apps_list" => ok(json!({ "apps": self.apps.iter().map(|a| json!({
                "id": a.id, "title": a.title,
            })).collect::<Vec<_>>() })),
            "settings_get" => ok(self.settings.clone()),
            other => self
                .answer_app(other, input)
                .unwrap_or_else(|| ok(json!({ "ok": true }))),
        }
    }

    fn find_messages(&self, input: &Value) -> Answer {
        let mailbox = str_arg(input, "mailbox").unwrap_or("inbox");
        let since = str_arg(input, "since");
        let before = str_arg(input, "before");
        // One page as the manifest says: 25 when omitted, at most 50, after `offset` hits.
        let limit = input
            .get("limit")
            .and_then(Value::as_u64)
            .unwrap_or(25)
            .min(50) as usize;
        let offset = input.get("offset").and_then(Value::as_u64).unwrap_or(0) as usize;
        let mut hits: Vec<&Message> = self
            .messages
            .iter()
            .filter(|m| str_arg(input, "account").is_none_or(|a| a == m.account))
            .filter(|m| {
                self.mailbox(&m.account, &m.mailbox).is_some_and(|b| {
                    b.role == mailbox || b.name.to_lowercase() == mailbox.to_lowercase()
                })
            })
            .filter(|m| str_arg(input, "from").is_none_or(|f| has(&m.from, f)))
            .filter(|m| str_arg(input, "to").is_none_or(|t| has(&m.to, t)))
            .filter(|m| str_arg(input, "subject").is_none_or(|s| has(&m.subject, s)))
            .filter(|m| {
                // Like IMAP's TEXT search: the headers and the body.
                str_arg(input, "text").is_none_or(|t| {
                    has(&m.from, t) || has(&m.to, t) || has(&m.subject, t) || has(&m.body, t)
                })
            })
            .filter(|m| since.is_none_or(|d| m.date[..10] >= *d))
            .filter(|m| before.is_none_or(|d| m.date[..10] < *d))
            .filter(|m| bool_arg(input, "unread").is_none_or(|u| m.unread == u))
            .filter(|m| bool_arg(input, "flagged").is_none_or(|f| m.flagged == f))
            .filter(|m| bool_arg(input, "hasAttachment").is_none_or(|a| m.has_attachment == a))
            .collect();
        hits.sort_by(|a, b| b.date.cmp(&a.date));
        let total = hits.len();
        let messages: Vec<Value> = hits
            .into_iter()
            .skip(offset)
            .take(limit)
            .map(|m| {
                json!({
                    "id": m.id, "account": m.account, "from": m.from, "subject": m.subject,
                    "date": m.date, "unread": m.unread, "flagged": m.flagged,
                })
            })
            .collect();
        let has_more = offset.saturating_add(messages.len()) < total;
        ok(json!({ "messages": messages, "total": total, "hasMore": has_more }))
    }

    fn get_message(&self, input: &Value) -> Answer {
        match str_arg(input, "id").and_then(|id| self.message(id)) {
            Some(m) => ok(json!({
                "id": m.id, "account": m.account, "from": m.from, "to": [m.to], "subject": m.subject,
                "date": m.date, "unread": m.unread, "flagged": m.flagged, "text": m.body,
                "truncated": false, "attachments": [],
            })),
            None => error("no message with this id"),
        }
    }

    fn list_mailboxes(&self, input: &Value) -> Answer {
        let accounts: Vec<Value> = self
            .accounts
            .iter()
            .filter(|a| str_arg(input, "account").is_none_or(|id| id == a.id))
            .map(|a| {
                let mailboxes: Vec<Value> = a
                    .mailboxes
                    .iter()
                    .map(|b| {
                        let inside = self
                            .messages
                            .iter()
                            .filter(|m| m.account == a.id && m.mailbox == b.id);
                        let unread = inside.clone().filter(|m| m.unread).count();
                        json!({ "name": b.name, "role": b.role, "unread": unread, "total": inside.count() })
                    })
                    .collect();
                json!({ "account": a.id, "email": a.email, "mailboxes": mailboxes })
            })
            .collect();
        ok(json!({ "accounts": accounts }))
    }

    fn create_draft(&self, input: &Value) -> Answer {
        let account_known =
            str_arg(input, "account").is_some_and(|id| self.accounts.iter().any(|a| a.id == id));
        if !account_known {
            return error("no account with this id");
        }
        if str_arg(input, "replyTo").is_some_and(|id| self.message(id).is_none()) {
            return error("no message with this id");
        }
        ok(json!({ "draftId": "draft-1" }))
    }

    fn each_message(&self, input: &Value, key: &str, done: &str) -> Answer {
        self.known_ids(&ids_arg(input, key), done)
    }

    fn known_ids(&self, ids: &[String], done: &str) -> Answer {
        if ids.is_empty() {
            return error("no message ids");
        }
        let unknown: Vec<&String> = ids.iter().filter(|id| self.message(id).is_none()).collect();
        if unknown.is_empty() {
            ok(json!({ done: ids }))
        } else {
            error(format!("no message with the ids {unknown:?}"))
        }
    }

    fn contacts_search(&self, input: &Value) -> Answer {
        let query = str_arg(input, "query").unwrap_or_default();
        let contacts: Vec<Value> = self
            .contacts
            .iter()
            .filter(|c| {
                has(&c.name, query)
                    || c.organization.as_deref().is_some_and(|o| has(o, query))
                    || c.emails.iter().any(|e| has(e, query))
            })
            .map(contact_json)
            .collect();
        ok(json!({ "contacts": contacts }))
    }
}

fn contact_json(c: &Contact) -> Value {
    json!({
        "id": c.id, "name": c.name, "organization": c.organization, "phones": c.phones,
        "emails": c.emails, "addresses": c.addresses,
    })
}

pub(super) fn file_json(f: &FileEntry) -> Value {
    json!({ "name": f.name, "path": f.path, "kind": "file", "size": f.size, "modified": f.modified })
}
