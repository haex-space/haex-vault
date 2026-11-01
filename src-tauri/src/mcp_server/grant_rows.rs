//! The rows of `haex_mcp_agent_grants` that [`Rights`] are stored as (spec 021 data-model.md):
//! `grant_group`, `grant_key`, `level` and `scope_tags`, for every group in one place. A row this
//! build cannot read counts as no right, so a newer build's rows never widen an older one's rights
//! (FR-008).
//!
//! - `operate`: one row per scope of "holzi bedienen", keyed by the scope.
//! - `browser` (spec 065): a switch row with an empty key, a row `user-browser` and one row per
//!   website; a website whose credential fill is remembered carries the tags `["fill"]`.
//! - `passwords`: one row with an empty key; its tags are a JSON array of tag names, none for every
//!   entry.
//! - `extension`: one row per extension, keyed by its id.
//! - `commands`: one row with an empty key when it is on.

use uuid::Uuid;

use super::grants::{
    BrowserRights, OperateLevel, PasswordLevel, PasswordRights, Rights, SiteGrant, SiteLevel,
};
use crate::agent_browser::sites::Site;

const GROUP_OPERATE: &str = "operate";
const GROUP_BROWSER: &str = "browser";
const GROUP_PASSWORDS: &str = "passwords";
const GROUP_EXTENSION: &str = "extension";
const GROUP_COMMANDS: &str = "commands";

/// The level of a row that switches something on.
const ON: &str = "on";
/// The key of the row that turns the group `browser` on.
const BROWSER_ON: &str = "";
/// The key of the row that allows the user's own browser.
const USER_BROWSER: &str = "user-browser";
/// The scope tags of a website whose credential fill is remembered.
const FILL_TAGS: &str = r#"["fill"]"#;

/// One row of `haex_mcp_agent_grants`, without its ids.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrantRow {
    pub group: String,
    pub key: String,
    pub level: String,
    pub tags: Option<String>,
}

impl GrantRow {
    fn new(group: &str, key: impl Into<String>, level: impl Into<String>) -> Self {
        Self {
            group: group.to_owned(),
            key: key.into(),
            level: level.into(),
            tags: None,
        }
    }

    fn tagged(mut self, tags: Option<String>) -> Self {
        self.tags = tags;
        self
    }
}

/// The rows `rights` are stored as.
pub fn rows_of(rights: &Rights) -> Vec<GrantRow> {
    let operate = rights
        .operate
        .iter()
        .map(|(scope, level)| GrantRow::new(GROUP_OPERATE, scope.clone(), level.as_str()));
    let browser = rights.browser.iter().flat_map(browser_rows);
    let extensions = rights
        .extensions
        .iter()
        .map(|id| GrantRow::new(GROUP_EXTENSION, id.to_string(), ON));
    let passwords = rights.passwords.iter().map(password_row);
    let commands = rights
        .commands
        .then(|| GrantRow::new(GROUP_COMMANDS, "", ON));
    operate
        .chain(browser)
        .chain(extensions)
        .chain(passwords)
        .chain(commands)
        .collect()
}

/// The rights `rows` stand for.
pub fn rights_of(rows: impl IntoIterator<Item = GrantRow>) -> Rights {
    let mut rights = Rights::default();
    let mut browser = Vec::new();
    for row in rows {
        match row.group.as_str() {
            GROUP_OPERATE => {
                if let Some(level) = OperateLevel::parse(&row.level) {
                    rights.operate.insert(row.key, level);
                }
            }
            GROUP_BROWSER => browser.push(row),
            GROUP_PASSWORDS => rights.passwords = passwords_of(&row),
            GROUP_EXTENSION if row.level == ON => {
                if let Ok(id) = Uuid::parse_str(&row.key) {
                    rights.extensions.insert(id);
                }
            }
            GROUP_COMMANDS => rights.commands = row.level == ON,
            _ => {}
        }
    }
    rights.browser = browser_of(&browser);
    rights
}

fn browser_rows(browser: &BrowserRights) -> Vec<GrantRow> {
    let mut rows = vec![GrantRow::new(GROUP_BROWSER, BROWSER_ON, ON)];
    if browser.user_browser {
        rows.push(GrantRow::new(GROUP_BROWSER, USER_BROWSER, ON));
    }
    rows.extend(browser.sites.iter().map(|(site, grant)| {
        let tags = grant.fill_remembered.then(|| FILL_TAGS.to_owned());
        GrantRow::new(GROUP_BROWSER, site.to_string(), grant.level.as_str()).tagged(tags)
    }));
    rows
}

/// The group `browser`; off without the row that turns it on.
fn browser_of(rows: &[GrantRow]) -> Option<BrowserRights> {
    rows.iter()
        .find(|row| row.key == BROWSER_ON && row.level == ON)?;
    let mut browser = BrowserRights::default();
    for row in rows {
        if row.key == BROWSER_ON {
            continue;
        }
        if row.key == USER_BROWSER {
            browser.user_browser = row.level == ON;
            continue;
        }
        let (Ok(site), Some(level)) = (Site::parse(&row.key), SiteLevel::parse(&row.level)) else {
            continue;
        };
        let fill_remembered = level == SiteLevel::Operate && row.tags.as_deref() == Some(FILL_TAGS);
        browser.sites.insert(
            site,
            SiteGrant {
                level,
                fill_remembered,
            },
        );
    }
    Some(browser)
}

fn password_row(passwords: &PasswordRights) -> GrantRow {
    let tags = passwords
        .tags
        .as_ref()
        .map(|tags| serde_json::Value::from(tags.clone()).to_string());
    GrantRow::new(GROUP_PASSWORDS, "", passwords.level.as_str()).tagged(tags)
}

/// The group `passwords`; unreadable tags must not widen it to every entry.
fn passwords_of(row: &GrantRow) -> Option<PasswordRights> {
    let tags = match row.tags.as_deref().map(serde_json::from_str::<Vec<String>>) {
        None => None,
        Some(Ok(tags)) => Some(tags),
        Some(Err(_)) => return None,
    };
    Some(PasswordRights {
        level: PasswordLevel::parse(&row.level)?,
        tags,
    })
}
