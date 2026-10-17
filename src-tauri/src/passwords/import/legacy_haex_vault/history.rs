//! The history of haex-vault 2.x (Legacy) entries (spec 037, research R7). haex-vault 2.x writes the JSON of a state
//! in three shapes (its editor, its KeePass import, its external API); holzi's [`SnapshotData`]
//! reads all of them, unknown fields such as `tags` are ignored. The attachments of a state come
//! from `snapshot_binaries`, because the KeePass import of haex-vault 2.x leaves the JSON list empty.

use std::collections::HashMap;

use haex_crdt::rusqlite::Connection;

use super::corrupt;
use super::read::{attachment, icon_name, Binaries};
use crate::error::Result;
use crate::passwords::import::{holzi_time, ImportAttachment, ImportState, Problem};
use crate::passwords::model::AttentionKind;
use crate::passwords::snapshots::SnapshotData;

struct Row {
    item_id: String,
    id: String,
    data: String,
    time: Option<String>,
}

/// The states of every entry, oldest first, keyed by the entry's id in haex-vault 2.x. A state that
/// cannot be read is left out and reported at its entry.
pub(super) fn read_history(
    conn: &Connection,
    binaries: &Binaries<'_>,
    problems: &mut HashMap<String, Vec<Problem>>,
) -> Result<HashMap<String, Vec<ImportState>>> {
    let mut statement = conn
        .prepare(
            "SELECT item_id, id, snapshot_data, COALESCE(modified_at, created_at) \
             FROM haex_passwords_item_snapshots ORDER BY rowid",
        )
        .map_err(corrupt)?;
    let rows: Vec<Row> = statement
        .query_map([], |r| {
            Ok(Row {
                item_id: r.get(0)?,
                id: r.get(1)?,
                data: r.get(2)?,
                time: r.get(3)?,
            })
        })
        .and_then(Iterator::collect)
        .map_err(corrupt)?;
    let files = state_files(conn)?;

    let mut states: HashMap<String, Vec<ImportState>> = HashMap::new();
    for row in rows {
        let item_problems = problems.entry(row.item_id.clone()).or_default();
        let time = holzi_time(row.time.as_deref());
        let Ok(mut data) = serde_json::from_str::<SnapshotData>(&row.data) else {
            item_problems.push(Problem::field(
                AttentionKind::HistoryUnreadable,
                time.as_deref().unwrap_or("?"),
            ));
            continue;
        };
        data.icon = data.icon.as_deref().and_then(|icon| {
            let mapped = icon_name(icon);
            if mapped.is_none() {
                item_problems.push(Problem::field(AttentionKind::IconNotMapped, "history icon"));
            }
            mapped
        });
        data.attachments.clear();
        let mut attachments: Vec<ImportAttachment> = Vec::new();
        for (hash, file_name) in files.get(&row.id).into_iter().flatten() {
            if let Some(file) = attachment(binaries, hash, file_name, item_problems)? {
                attachments.push(file);
            }
        }
        states.entry(row.item_id).or_default().push(ImportState {
            modified_at: time,
            data,
            attachments,
        });
    }
    for list in states.values_mut() {
        list.sort_by(|a, b| a.modified_at.cmp(&b.modified_at));
    }
    Ok(states)
}

/// The attachments of each state: snapshot id → (hash, file name), in the order written.
fn state_files(conn: &Connection) -> Result<HashMap<String, Vec<(String, String)>>> {
    let mut statement = conn
        .prepare(
            "SELECT snapshot_id, binary_hash, file_name FROM haex_passwords_snapshot_binaries \
             ORDER BY rowid",
        )
        .map_err(corrupt)?;
    let rows: Vec<(String, String, String)> = statement
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .and_then(Iterator::collect)
        .map_err(corrupt)?;
    let mut files: HashMap<String, Vec<(String, String)>> = HashMap::new();
    for (snapshot, hash, name) in rows {
        files.entry(snapshot).or_default().push((hash, name));
    }
    Ok(files)
}
