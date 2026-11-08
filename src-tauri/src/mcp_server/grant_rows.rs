//! The rows that [`Rights`] are stored as, in `haex_agent_profile_grants` (spec 083; spec 021
//! stored the same rows per agent in `haex_mcp_agent_grants`, dropped by migration 0056):
//! `grant_group`, `grant_key`, `level` and `scope_tags`, for every group in one place. A row this
//! build cannot read counts as no right, so a newer build's rows never widen an older one's rights
//! (FR-008).
//!
//! - `operate`: one row per scope of "holzi bedienen", keyed by the scope.
//! - `browser` (spec 065): a switch row with an empty key, a row `user-browser` and one row per
//!   website; a website whose credential fill is remembered carries the tags `["fill"]`; a rule
//!   for the requests a page sends to a website is a row `request:<website>` at `allow` or `deny`
//!   (an older build cannot read the key as a website and skips it); the level for every other
//!   website is a row `*` at `read` or `operate` (FR-043; no row is none, and an older build skips
//!   it too).
//! - `passwords`: one row keyed by the tag new entries get (empty for none); its tags are a JSON
//!   array of tag names, none for every entry.
//! - `extension`: one row per extension, keyed by its id, and the row `*` for "Alle Erweiterungen".
//! - `commands`: one row with an empty key when it is on.
//! - `delegate` (spec 083 FR-030): a row `autonomy` at `standard`, `ungated` or `gated_permissive`
//!   (without it the group is off, so every call asks) and a row `deny:<category>` at `on` for each
//!   category the permissive mode refuses; an older build skips the group.
//! - `contacts` (spec 083 FR-032): the row `*` for all contacts and one row `group:<id>` per group of
//!   the address book, at `read`, `readWrite` or `denied`; a level this build does not know reads as
//!   `denied`, so a newer build's rows never widen anything.
//! - `files` (spec 083 FR-033): the rows `device`, `shares` and `sync` at `on` ("Dateien des Geräts",
//!   "Dateifreigaben verwalten", "Sync-Regeln verwalten"), a row `storage:<id>` per storage and a
//!   row `folder:<storage id>/<folder id>` per encrypted folder at `read`, `readWrite` or `denied`
//!   (unknown reads as `denied`); a folder row for cloud models too carries the tags `["cloud"]`.

use uuid::Uuid;

use super::grants::{
    AccessLevel, BrowserRights, ContactsRights, DelegateRights, FilesRights, FolderRight,
    OperateLevel, PasswordLevel, PasswordRights, RequestRule, Rights, SiteGrant, SiteLevel,
    OPERATE_SCOPES,
};
use crate::adapters::cli_delegate::autonomy::{AutonomyMode, DenyCategory};
use crate::agent_browser::sites::Site;

const GROUP_OPERATE: &str = "operate";
const GROUP_BROWSER: &str = "browser";
const GROUP_PASSWORDS: &str = "passwords";
const GROUP_EXTENSION: &str = "extension";
const GROUP_COMMANDS: &str = "commands";
const GROUP_DELEGATE: &str = "delegate";
const GROUP_CONTACTS: &str = "contacts";
const GROUP_FILES: &str = "files";
const FILES_DEVICE: &str = "device";
const FILES_SHARES: &str = "shares";
const FILES_SYNC: &str = "sync";
const STORAGE_PREFIX: &str = "storage:";
const FOLDER_PREFIX: &str = "folder:";
/// The scope tags of an encrypted folder cloud models reach too.
const CLOUD_TAGS: &str = r#"["cloud"]"#;
/// The key of the row for all contacts in the group `contacts`.
const ALL_CONTACTS: &str = "*";
/// The start of the key of a contact group in the group `contacts`.
const CONTACT_GROUP_PREFIX: &str = "group:";
/// The key of the row with the autonomy mode of the group `delegate`.
const AUTONOMY: &str = "autonomy";
/// The start of the key of a deny category of the group `delegate`.
const DENY_PREFIX: &str = "deny:";
/// The key of the row "Alle Erweiterungen" (spec 083 plan Q3); an older build cannot read it as an
/// extension id and skips it.
const ALL_EXTENSIONS: &str = "*";

/// The level of a row that switches something on.
const ON: &str = "on";
/// The key of the row that turns the group `browser` on.
const BROWSER_ON: &str = "";
/// The key of the row that allows the user's own browser.
const USER_BROWSER: &str = "user-browser";
/// The key of the row with the level for all websites not listed (FR-043).
const ALL_SITES: &str = "*";
/// The start of the key of a rule for page requests (FR-044).
const REQUEST_PREFIX: &str = "request:";
/// The scope tags of a website whose credential fill is remembered.
const FILL_TAGS: &str = r#"["fill"]"#;

/// One rights row, without its ids.
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
    let all_extensions = rights
        .all_extensions
        .then(|| GrantRow::new(GROUP_EXTENSION, ALL_EXTENSIONS, ON));
    let extensions = all_extensions.into_iter().chain(
        rights
            .extensions
            .iter()
            .map(|id| GrantRow::new(GROUP_EXTENSION, id.to_string(), ON)),
    );
    let passwords = rights.passwords.iter().map(password_row);
    let commands = rights
        .commands
        .then(|| GrantRow::new(GROUP_COMMANDS, "", ON));
    let delegate = rights.delegate.iter().flat_map(delegate_rows);
    let contacts = contact_rows(&rights.contacts);
    let files = file_rows(&rights.files);
    operate
        .chain(browser)
        .chain(extensions)
        .chain(passwords)
        .chain(commands)
        .chain(delegate)
        .chain(contacts)
        .chain(files)
        .collect()
}

/// The rights `rows` stand for.
pub fn rights_of(rows: impl IntoIterator<Item = GrantRow>) -> Rights {
    let mut rights = Rights::default();
    let mut browser = Vec::new();
    let mut delegate = Vec::new();
    for row in rows {
        match row.group.as_str() {
            // Only the scopes this build knows count: a key it does not know (a newer build's, or one
            // that is no right at all) never widens anything (FR-012).
            GROUP_OPERATE if OPERATE_SCOPES.contains(&row.key.as_str()) => {
                if let Some(level) = OperateLevel::parse(&row.level) {
                    rights.operate.insert(row.key, level);
                }
            }
            GROUP_BROWSER => browser.push(row),
            GROUP_DELEGATE => delegate.push(row),
            GROUP_CONTACTS => contacts_entry(&mut rights.contacts, row),
            GROUP_FILES => files_entry(&mut rights.files, row),
            GROUP_PASSWORDS => rights.passwords = passwords_of(&row),
            GROUP_EXTENSION if row.level == ON => {
                if row.key == ALL_EXTENSIONS {
                    rights.all_extensions = true;
                } else if let Ok(id) = Uuid::parse_str(&row.key) {
                    rights.extensions.insert(id);
                }
            }
            // The commands row has the empty key; any other key is not this right.
            GROUP_COMMANDS if row.key.is_empty() => rights.commands = row.level == ON,
            _ => {}
        }
    }
    rights.browser = browser_of(&browser);
    rights.delegate = delegate_of(&delegate);
    rights
}

fn contact_rows(contacts: &ContactsRights) -> Vec<GrantRow> {
    contacts
        .all
        .iter()
        .map(|level| GrantRow::new(GROUP_CONTACTS, ALL_CONTACTS, level.as_str()))
        .chain(contacts.groups.iter().map(|(id, level)| {
            GrantRow::new(
                GROUP_CONTACTS,
                format!("{CONTACT_GROUP_PREFIX}{id}"),
                level.as_str(),
            )
        }))
        .collect()
}

/// One row of the group `contacts`; a key that is neither `*` nor a group is no right.
fn contacts_entry(contacts: &mut ContactsRights, row: GrantRow) {
    let level = AccessLevel::parse(&row.level);
    if row.key == ALL_CONTACTS {
        contacts.all = Some(level);
    } else if let Some(id) = row
        .key
        .strip_prefix(CONTACT_GROUP_PREFIX)
        .filter(|id| !id.is_empty())
    {
        contacts.groups.insert(id.to_owned(), level);
    }
}

fn file_rows(files: &FilesRights) -> Vec<GrantRow> {
    let switch = |on: bool, key: &str| on.then(|| GrantRow::new(GROUP_FILES, key, ON));
    [
        switch(files.device, FILES_DEVICE),
        switch(files.file_shares, FILES_SHARES),
        switch(files.sync_rules, FILES_SYNC),
    ]
    .into_iter()
    .flatten()
    .chain(files.storages.iter().map(|(id, level)| {
        GrantRow::new(GROUP_FILES, format!("{STORAGE_PREFIX}{id}"), level.as_str())
    }))
    .chain(files.folders.iter().map(|(target, right)| {
        GrantRow::new(
            GROUP_FILES,
            format!("{FOLDER_PREFIX}{target}"),
            right.level.as_str(),
        )
        .tagged(right.cloud.then(|| CLOUD_TAGS.to_owned()))
    }))
    .collect()
}

/// One row of the group `files`; a key this build does not know is no right.
fn files_entry(files: &mut FilesRights, row: GrantRow) {
    match row.key.as_str() {
        FILES_DEVICE => files.device = row.level == ON,
        FILES_SHARES => files.file_shares = row.level == ON,
        FILES_SYNC => files.sync_rules = row.level == ON,
        key => {
            let level = AccessLevel::parse(&row.level);
            if let Some(id) = key.strip_prefix(STORAGE_PREFIX).filter(|id| !id.is_empty()) {
                files.storages.insert(id.to_owned(), level);
            } else if let Some(target) = key
                .strip_prefix(FOLDER_PREFIX)
                .filter(|target| target.contains('/'))
            {
                files.folders.insert(
                    target.to_owned(),
                    FolderRight {
                        level,
                        cloud: row.tags.as_deref() == Some(CLOUD_TAGS),
                    },
                );
            }
        }
    }
}

fn delegate_rows(delegate: &DelegateRights) -> Vec<GrantRow> {
    let mut rows = vec![GrantRow::new(
        GROUP_DELEGATE,
        AUTONOMY,
        delegate.autonomy.as_str(),
    )];
    rows.extend(delegate.deny.iter().map(|category| {
        GrantRow::new(
            GROUP_DELEGATE,
            format!("{DENY_PREFIX}{}", category.as_str()),
            ON,
        )
    }));
    rows
}

/// The group `delegate`; off without a readable autonomy row. A deny category this build does not
/// know makes the whole group unreadable: skipping it would refuse less than the user set, so the
/// group is off and every call asks.
fn delegate_of(rows: &[GrantRow]) -> Option<DelegateRights> {
    let autonomy = rows
        .iter()
        .find(|row| row.key == AUTONOMY)
        .and_then(|row| AutonomyMode::parse(&row.level))?;
    let mut deny = std::collections::BTreeSet::new();
    for row in rows.iter().filter(|row| row.level == ON) {
        if let Some(label) = row.key.strip_prefix(DENY_PREFIX) {
            deny.insert(DenyCategory::parse(label)?);
        }
    }
    Some(DelegateRights { autonomy, deny })
}

fn browser_rows(browser: &BrowserRights) -> Vec<GrantRow> {
    let mut rows = vec![GrantRow::new(GROUP_BROWSER, BROWSER_ON, ON)];
    if browser.user_browser {
        rows.push(GrantRow::new(GROUP_BROWSER, USER_BROWSER, ON));
    }
    if let Some(level) = browser.all {
        rows.push(GrantRow::new(GROUP_BROWSER, ALL_SITES, level.as_str()));
    }
    rows.extend(browser.sites.iter().map(|(site, grant)| {
        let tags = grant.fill_remembered.then(|| FILL_TAGS.to_owned());
        GrantRow::new(GROUP_BROWSER, site.to_string(), grant.level.as_str()).tagged(tags)
    }));
    rows.extend(browser.requests.iter().map(|(site, rule)| {
        GrantRow::new(
            GROUP_BROWSER,
            format!("{REQUEST_PREFIX}{site}"),
            rule.as_str(),
        )
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
        if row.key == ALL_SITES {
            // A blocked level for all websites is no right (FR-047): it reads as nothing.
            browser.all = SiteLevel::parse(&row.level).filter(|level| level.opens());
            continue;
        }
        if let Some(site) = row.key.strip_prefix(REQUEST_PREFIX) {
            if let (Ok(site), Some(rule)) = (Site::parse(site), RequestRule::parse(&row.level)) {
                browser.requests.insert(site, rule);
            }
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
    let default_tag = passwords.default_tag.as_deref().unwrap_or("");
    GrantRow::new(GROUP_PASSWORDS, default_tag, passwords.level.as_str()).tagged(tags)
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
        default_tag: Some(row.key.clone()).filter(|tag| !tag.is_empty()),
    })
}
