//! Copying entries and folders (spec 036, FR-015, FR-022, research R7, `passwords_copy`): a deep
//! copy in the caller's one transaction, so several hundred entries are all copied or none. A copy
//! is a new entry with a new id: every field by value (custom fields, tags, TOTP, aliases, expiry,
//! icon, colour), the attachments linked to the same binaries (nothing is stored twice), and on
//! request the history and the user name or password as a placeholder on the original. A passkey is
//! never copied; on request the copy shows it by a link (research R6). Folders are copied with
//! every subfolder and entry.

use haex_crdt::rusqlite::params;
use haex_crdt::CrdtTransaction;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use super::groups::is_in_trash;
use super::items::item_state;
use super::kind::EntryType;
use super::model::{Target, TargetKind};
use super::references::{build_token, RefKind};
use super::{clock, groups, passkey_links, snapshots, tags, TRASH_GROUP_ID};
use crate::error::{HolziError, Result};
use crate::storage::query::Query;

/// The title of a copy: the exact title for a single entry, or a suffix for several targets or a
/// folder (the window passes the localized default, " – Kopie" or " – Copy").
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub enum CopyTitle {
    Exact(String),
    Suffix(String),
}

#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct CopyOptions {
    pub title: CopyTitle,
    #[serde(default)]
    pub history: bool,
    #[serde(default)]
    pub username_as_reference: bool,
    #[serde(default)]
    pub password_as_reference: bool,
    #[serde(default)]
    #[ts(optional)]
    pub passkeys_as_links: Option<bool>,
}

/// What a copy made. No ids: the window reloads.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct CopyReport {
    pub items_created: u32,
    pub groups_created: u32,
    pub passkey_links: u32,
    /// Targets that were deleted meanwhile (or lie in the trash) and were skipped.
    pub skipped_missing: u32,
}

fn invalid(reason: &str) -> HolziError {
    HolziError::InvalidInput {
        reason: reason.to_string(),
    }
}

/// The columns of an entry that a copy takes over.
struct Columns {
    title: Option<String>,
    username: Option<String>,
    password: Option<String>,
    note: Option<String>,
    icon: Option<String>,
    color: Option<String>,
    url: Option<String>,
    otp_secret: Option<String>,
    otp_digits: Option<i64>,
    otp_period: Option<i64>,
    otp_algorithm: Option<String>,
    expires_at: Option<String>,
    autofill_aliases: Option<String>,
    /// The holzi function the entry belongs to (Z14); a copy keeps it, so the copy of a storage's
    /// credentials is as hidden from extensions as the original.
    owner: Option<String>,
    /// `login` or `pass` (spec 092); a copy has the type of its original.
    kind: String,
}

fn columns(q: &mut impl Query, item_id: &str) -> Result<Option<Columns>> {
    q.query_row(
        "SELECT title, username, password, note, icon, color, url, otp_secret, otp_digits, \
                otp_period, otp_algorithm, expires_at, autofill_aliases, owner, kind \
         FROM haex_passwords_item_details WHERE id = ?1",
        params![item_id],
        |r| {
            Ok(Columns {
                title: r.get(0)?,
                username: r.get(1)?,
                password: r.get(2)?,
                note: r.get(3)?,
                icon: r.get(4)?,
                color: r.get(5)?,
                url: r.get(6)?,
                otp_secret: r.get(7)?,
                otp_digits: r.get(8)?,
                otp_period: r.get(9)?,
                otp_algorithm: r.get(10)?,
                expires_at: r.get(11)?,
                autofill_aliases: r.get(12)?,
                owner: r.get(13)?,
                kind: r.get(14)?,
            })
        },
    )
    .map_err(Into::into)
}

/// The value of a copy for a field that may point at the original: the placeholder when asked and
/// the original has a value, else the value itself.
fn value_or_reference(
    source: &str,
    value: Option<String>,
    as_reference: bool,
    kind: RefKind,
) -> Option<String> {
    match value {
        Some(text) if as_reference && !text.is_empty() => build_token(source, &kind).or(Some(text)),
        other => other,
    }
}

/// The typed fields and the codes of a pass (spec 092): the copy has the same start, kind and
/// parent, new code rows, and no `source_id`, so it is not taken for the imported file itself.
fn copy_pass(tx: &mut CrdtTransaction<'_>, source: &str, id: &str) -> Result<()> {
    tx.execute(
        "INSERT INTO haex_passwords_pass_details \
         (item_id, pass_kind, start_date, start_at, start_zone, parent_id) \
         SELECT ?2, pass_kind, start_date, start_at, start_zone, parent_id \
         FROM haex_passwords_pass_details WHERE item_id = ?1",
        params![source, id],
    )?;
    let codes = tx.query_map(
        "SELECT format, value, label, position, flag FROM haex_passwords_pass_barcodes \
         WHERE item_id = ?1 ORDER BY position, id",
        params![source],
        |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, Option<String>>(4)?,
            ))
        },
    )?;
    for (format, value, label, position, flag) in codes {
        tx.execute(
            "INSERT INTO haex_passwords_pass_barcodes \
             (id, item_id, format, value, label, position, flag) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![Uuid::new_v4().to_string(), id, format, value, label, position, flag],
        )?;
    }
    Ok(())
}

/// Copies one entry; `None` when it is gone, else the number of passkey links the copy got.
fn copy_item(
    tx: &mut CrdtTransaction<'_>,
    source: &str,
    into: Option<&str>,
    title: Option<String>,
    options: &CopyOptions,
) -> Result<Option<u32>> {
    let Some(stored) = columns(tx, source)? else {
        return Ok(None);
    };
    let id = Uuid::new_v4().to_string();
    let now = clock::now();
    let username = value_or_reference(
        source,
        stored.username,
        options.username_as_reference,
        RefKind::Username,
    );
    let password = value_or_reference(
        source,
        stored.password,
        options.password_as_reference,
        RefKind::Password,
    );
    tx.execute(
        "INSERT INTO haex_passwords_item_details \
         (id, title, username, password, note, icon, color, url, otp_secret, otp_digits, \
          otp_period, otp_algorithm, expires_at, autofill_aliases, created_at, updated_at, owner, \
          kind) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?15, ?16, ?17)",
        params![
            id,
            title.or(stored.title),
            username,
            password,
            stored.note,
            stored.icon,
            stored.color,
            stored.url,
            stored.otp_secret,
            stored.otp_digits,
            stored.otp_period,
            stored.otp_algorithm,
            stored.expires_at,
            stored.autofill_aliases,
            now,
            stored.owner,
            stored.kind,
        ],
    )?;
    if stored.kind == super::kind::EntryType::PASS {
        copy_pass(tx, source, &id)?;
    }
    let names = tags::names_of_item(tx, source)?;
    tags::set_item_tags(tx, &id, &names)?;
    let fields = tx.query_map(
        "SELECT key, value FROM haex_passwords_item_key_values WHERE item_id = ?1 ORDER BY rowid",
        params![source],
        |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?)),
    )?;
    for (key, value) in fields {
        tx.execute(
            "INSERT INTO haex_passwords_item_key_values (id, item_id, key, value, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![Uuid::new_v4().to_string(), id, key, value, now],
        )?;
    }
    if let Some(group) = into {
        tx.execute(
            "INSERT INTO haex_passwords_group_items (item_id, group_id) VALUES (?1, ?2)",
            params![id, group],
        )?;
    }
    let attachments = tx.query_map(
        "SELECT binary_hash, file_name FROM haex_passwords_item_binaries \
         WHERE item_id = ?1 ORDER BY rowid",
        params![source],
        |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
    )?;
    for (hash, file_name) in attachments {
        tx.execute(
            "INSERT INTO haex_passwords_item_binaries (id, item_id, binary_hash, file_name) \
             VALUES (?1, ?2, ?3, ?4)",
            params![Uuid::new_v4().to_string(), id, hash, file_name],
        )?;
    }
    if options.history {
        copy_history(tx, source, &id)?;
    }
    // A passkey is never copied by value (that would clone an authenticator, FR-046): with
    // "Passkeys per Verweis" the copy shows each passkey of the original, its own and the linked
    // ones, by a link (research R6).
    let mut links = 0;
    if options.passkeys_as_links == Some(true) {
        for passkey in passkey_links::shown_passkey_ids(tx, source)? {
            passkey_links::link(tx, &id, &passkey)?;
            links += 1;
        }
    }
    // The copy's own state on top (with "Verlauf übernehmen" off, its first state).
    snapshots::take_snapshot(tx, &id)?;
    Ok(Some(links))
}

/// The history states of `source` as states of `target`, with new ids and their attachments.
fn copy_history(tx: &mut CrdtTransaction<'_>, source: &str, target: &str) -> Result<()> {
    let states = tx.query_map(
        "SELECT id, snapshot_data, created_at, modified_at FROM haex_passwords_item_snapshots \
         WHERE item_id = ?1 ORDER BY rowid",
        params![source],
        |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<String>>(3)?,
            ))
        },
    )?;
    for (old_id, data, created_at, modified_at) in states {
        let new_id = Uuid::new_v4().to_string();
        tx.execute(
            "INSERT INTO haex_passwords_item_snapshots \
             (id, item_id, snapshot_data, created_at, modified_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![new_id, target, data, created_at, modified_at],
        )?;
        let files = tx.query_map(
            "SELECT binary_hash, file_name FROM haex_passwords_snapshot_binaries \
             WHERE snapshot_id = ?1",
            params![old_id],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?)),
        )?;
        for (hash, file_name) in files {
            tx.execute(
                "INSERT INTO haex_passwords_snapshot_binaries \
                 (id, snapshot_id, binary_hash, file_name) VALUES (?1, ?2, ?3, ?4)",
                params![Uuid::new_v4().to_string(), new_id, hash, file_name],
            )?;
        }
    }
    Ok(())
}

/// A folder of the source subtree as the copy needs it.
struct FolderRow {
    id: String,
    name: Option<String>,
    description: Option<String>,
    icon: Option<String>,
    color: Option<String>,
    parent_id: Option<String>,
}

/// The folder and every folder below it, parents before children. Read before anything is
/// written, so a folder copied into its own subtree does not copy its copy. The recursion runs on
/// the id alone, so `UNION` ends it even on a parent loop (see `groups.rs`); the order is made here.
fn subtree(q: &mut impl Query, root: &str) -> Result<Vec<FolderRow>> {
    let rows = q.query_map(
        "WITH RECURSIVE below(id) AS ( \
           SELECT ?1 \
           UNION \
           SELECT g.id FROM haex_passwords_groups g JOIN below b ON g.parent_id = b.id) \
         SELECT g.id, g.name, g.description, g.icon, g.color, g.parent_id \
         FROM below b JOIN haex_passwords_groups g ON g.id = b.id ORDER BY g.rowid",
        params![root],
        |r| {
            Ok(FolderRow {
                id: r.get(0)?,
                name: r.get(1)?,
                description: r.get(2)?,
                icon: r.get(3)?,
                color: r.get(4)?,
                parent_id: r.get(5)?,
            })
        },
    )?;
    Ok(parents_first(rows, root))
}

/// `rows` from `root` down, each folder after its parent (breadth first, rowid order among
/// siblings); a folder reached twice through a loop comes once.
fn parents_first(mut rows: Vec<FolderRow>, root: &str) -> Vec<FolderRow> {
    let mut ordered: Vec<FolderRow> = Vec::with_capacity(rows.len());
    if let Some(position) = rows.iter().position(|row| row.id == root) {
        ordered.push(rows.remove(position));
    }
    let mut next = 0;
    while next < ordered.len() {
        let parent = ordered[next].id.clone();
        let mut index = 0;
        while index < rows.len() {
            if rows[index].parent_id.as_deref() == Some(parent.as_str()) {
                ordered.push(rows.remove(index));
            } else {
                index += 1;
            }
        }
        next += 1;
    }
    ordered
}

fn items_of(q: &mut impl Query, group_id: &str) -> Result<Vec<String>> {
    q.query_map(
        "SELECT item_id FROM haex_passwords_group_items WHERE group_id = ?1 ORDER BY rowid",
        params![group_id],
        |r| r.get::<_, String>(0),
    )
    .map_err(Into::into)
}

fn copy_group(
    tx: &mut CrdtTransaction<'_>,
    root: &str,
    into: Option<&str>,
    suffix: &str,
    options: &CopyOptions,
    report: &mut CopyReport,
) -> Result<()> {
    let folders = subtree(tx, root)?;
    let mut items: Vec<(String, Vec<String>)> = Vec::with_capacity(folders.len());
    for folder in &folders {
        items.push((folder.id.clone(), items_of(tx, &folder.id)?));
    }
    let mut new_ids: Vec<(String, String)> = Vec::with_capacity(folders.len());
    for folder in &folders {
        let parent = if folder.id == root {
            into.map(str::to_string)
        } else {
            folder.parent_id.as_ref().and_then(|old| {
                new_ids
                    .iter()
                    .find(|(id, _)| id == old)
                    .map(|(_, new)| new.clone())
            })
        };
        // Names as they are: a folder without a name (import, another device) stays so, and only
        // the copied folder itself gets the suffix.
        let name = if folder.id == root {
            Some(format!(
                "{}{suffix}",
                folder.name.as_deref().unwrap_or_default()
            ))
        } else {
            folder.name.clone()
        };
        let created = groups::insert_group_row(
            tx,
            name.as_deref(),
            folder.description.as_deref(),
            folder.icon.as_deref(),
            folder.color.as_deref(),
            parent.as_deref(),
        )?;
        report.groups_created += 1;
        new_ids.push((folder.id.clone(), created));
    }
    for (old_folder, entries) in items {
        let new_folder = new_ids
            .iter()
            .find(|(id, _)| *id == old_folder)
            .map(|(_, new)| new.clone());
        for entry in entries {
            if let Some(links) = copy_item(tx, &entry, new_folder.as_deref(), None, options)? {
                report.items_created += 1;
                report.passkey_links += links;
            }
        }
    }
    Ok(())
}

fn group_exists_outside_trash(q: &mut impl Query, id: &str) -> Result<bool> {
    let exists = q
        .query_row(
            "SELECT COUNT(*) FROM haex_passwords_groups WHERE id = ?1",
            params![id],
            |r| r.get::<_, i64>(0),
        )?
        .unwrap_or(0)
        > 0;
    Ok(exists && id != TRASH_GROUP_ID && !is_in_trash(q, id)?)
}

/// Copies `targets` into `into` (`None` is the top level) in the caller's transaction.
pub fn copy(
    tx: &mut CrdtTransaction<'_>,
    targets: &[Target],
    into: Option<&str>,
    options: &CopyOptions,
) -> Result<CopyReport> {
    if let CopyTitle::Exact(_) = options.title {
        let single_entry = targets.len() == 1 && targets[0].kind == TargetKind::Item;
        if !single_entry {
            return Err(invalid("options.title"));
        }
    }
    if let Some(group) = into {
        let exists = tx
            .query_row(
                "SELECT COUNT(*) FROM haex_passwords_groups WHERE id = ?1",
                params![group],
                |r| r.get::<_, i64>(0),
            )?
            .unwrap_or(0)
            > 0;
        if !exists {
            return Err(HolziError::PasswordsNotFound);
        }
        if group == TRASH_GROUP_ID || is_in_trash(tx, group)? {
            return Err(HolziError::PasswordsIntoTrash);
        }
    }
    let mut report = CopyReport::default();
    for target in targets {
        match target.kind {
            TargetKind::Item => {
                // A deleted entry, or one in the trash, is skipped and counted.
                let live = item_state(tx, &target.id)?.is_some_and(|s| !s.in_trash);
                if !live {
                    report.skipped_missing += 1;
                    continue;
                }
                // The two apps' sides hold for a copy as for a move (spec 092, Phase W).
                if let Some(stored) = columns(tx, &target.id)? {
                    groups::check_place(tx, &EntryType::parse(&stored.kind), into)?;
                }
                let title = match &options.title {
                    CopyTitle::Exact(title) => Some(title.clone()),
                    CopyTitle::Suffix(suffix) => {
                        let base = columns(tx, &target.id)?.and_then(|c| c.title);
                        let titled = format!("{}{suffix}", base.unwrap_or_default());
                        Some(titled).filter(|t| !t.trim().is_empty())
                    }
                };
                if let Some(links) = copy_item(tx, &target.id, into, title, options)? {
                    report.items_created += 1;
                    report.passkey_links += links;
                }
            }
            TargetKind::Group => {
                if !group_exists_outside_trash(tx, &target.id)? {
                    report.skipped_missing += 1;
                    continue;
                }
                groups::check_folder_place(tx, &target.id, into)?;
                let suffix = match &options.title {
                    CopyTitle::Suffix(suffix) => suffix.as_str(),
                    CopyTitle::Exact(_) => "",
                };
                copy_group(tx, &target.id, into, suffix, options, &mut report)?;
            }
        }
    }
    Ok(report)
}
