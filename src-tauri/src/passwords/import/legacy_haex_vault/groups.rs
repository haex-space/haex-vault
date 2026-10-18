//! Folders and tags of haex-vault 2.x (Legacy) (spec 037, `contracts/haex-vault-mapping.md` §Ordner, §Tags).
//! The folder rows can hold a parent that does not exist or a cycle (haex-vault 2.x checks neither);
//! such a folder goes to the top and is reported. The row `trash` is the recycle bin, and anything
//! whose chain reaches it is in the trash.

use std::collections::{BTreeMap, HashMap};

use haex_crdt::rusqlite::Connection;

use super::read::{icon_ref, Binaries};
use super::{corrupt, TRASH_ID};
use crate::error::Result;
use crate::passwords::ids::fold;
use crate::passwords::import::{ImportGroup, Problem};
use crate::passwords::model::AttentionKind;

/// The folders, parents before children, with each folder's effective parent.
pub(super) struct Folders {
    pub groups: Vec<ImportGroup>,
    parents: HashMap<String, Option<String>>,
}

impl Folders {
    pub fn exists(&self, id: &str) -> bool {
        self.parents.contains_key(id)
    }

    /// Whether the folder or one of its ancestors is the trash.
    pub fn in_trash(&self, id: &str) -> bool {
        let mut current = Some(id.to_string());
        let mut steps = 0;
        while let Some(id) = current {
            if id == TRASH_ID {
                return true;
            }
            steps += 1;
            if steps > self.parents.len() {
                return false;
            }
            current = self.parents.get(&id).cloned().flatten();
        }
        false
    }
}

struct Row {
    name: String,
    description: Option<String>,
    icon: Option<String>,
    sort_order: Option<i64>,
    color: Option<String>,
    parent: Option<String>,
}

pub(super) fn read_folders(
    conn: &Connection,
    binaries: &Binaries<'_>,
    problems: &mut Vec<Problem>,
) -> Result<Folders> {
    let mut statement = conn
        .prepare(
            "SELECT id, name, description, icon, sort_order, color, parent_id \
             FROM haex_passwords_groups ORDER BY id",
        )
        .map_err(corrupt)?;
    let rows: BTreeMap<String, Row> = statement
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                Row {
                    name: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                    description: r.get(2)?,
                    icon: r.get(3)?,
                    sort_order: r.get(4)?,
                    color: r.get(5)?,
                    parent: r.get(6)?,
                },
            ))
        })
        .and_then(Iterator::collect)
        .map_err(corrupt)?;

    let mut parents: HashMap<String, Option<String>> = rows
        .iter()
        .map(|(id, row)| {
            let parent = if id == TRASH_ID {
                None
            } else {
                row.parent.clone()
            };
            (id.clone(), parent)
        })
        .collect();
    // Break missing parents and cycles, in the order of the ids so the result is stable.
    for (id, row) in &rows {
        if reaches_itself_or_nothing(&parents, id) {
            parents.insert(id.clone(), None);
            problems.push(Problem::field(AttentionKind::GroupReparented, &row.name));
        }
    }

    let mut groups = Vec::with_capacity(rows.len());
    let mut emitted: HashMap<String, bool> = HashMap::new();
    for id in rows.keys() {
        emit(id, &rows, &parents, &mut emitted, &mut |id, row| {
            let mut icon_problem = Vec::new();
            let icon = if id == TRASH_ID {
                None
            } else {
                icon_ref(binaries, row.icon.as_deref(), &mut icon_problem)?
            };
            for problem in icon_problem {
                problems.push(Problem::field(
                    problem.kind,
                    &format!("folder {}", row.name),
                ));
            }
            groups.push(ImportGroup {
                reference: id.to_string(),
                parent_ref: parents.get(id).cloned().flatten(),
                name: row.name.clone(),
                description: row.description.clone(),
                icon,
                is_recycle_bin: id == TRASH_ID,
                previous_parent_ref: None,
                color: row.color.clone(),
                sort_order: row.sort_order,
            });
            Ok(())
        })?;
    }
    Ok(Folders { groups, parents })
}

/// True when following the parents of `id` meets a missing folder or `id` again.
fn reaches_itself_or_nothing(parents: &HashMap<String, Option<String>>, id: &str) -> bool {
    let mut current = parents.get(id).cloned().flatten();
    let mut steps = 0;
    while let Some(parent) = current {
        if parent == id || !parents.contains_key(&parent) || steps > parents.len() {
            return true;
        }
        steps += 1;
        current = parents.get(&parent).cloned().flatten();
    }
    false
}

/// Emits a folder after its parent (the parents are acyclic by now).
fn emit(
    id: &str,
    rows: &BTreeMap<String, Row>,
    parents: &HashMap<String, Option<String>>,
    emitted: &mut HashMap<String, bool>,
    push: &mut dyn FnMut(&str, &Row) -> Result<()>,
) -> Result<()> {
    if emitted.contains_key(id) {
        return Ok(());
    }
    emitted.insert(id.to_string(), true);
    if let Some(parent) = parents.get(id).cloned().flatten() {
        emit(&parent, rows, parents, emitted, push)?;
    }
    if let Some(row) = rows.get(id) {
        push(id, row)?;
    }
    Ok(())
}

/// The tags of the source: names by id, and the colour of each name. Names that only differ in
/// the way holzi compares them become one tag (holzi's rule), which the report names.
pub(super) struct Tags {
    pub names: HashMap<String, String>,
    pub colors: Vec<(String, String)>,
}

pub(super) fn read_tags(conn: &Connection, problems: &mut Vec<Problem>) -> Result<Tags> {
    let mut statement = conn
        .prepare("SELECT id, name, color FROM haex_passwords_tags ORDER BY rowid")
        .map_err(corrupt)?;
    let rows: Vec<(String, String, Option<String>)> = statement
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .and_then(Iterator::collect)
        .map_err(corrupt)?;
    let mut by_fold: BTreeMap<String, Vec<(String, Option<String>)>> = BTreeMap::new();
    let mut names = HashMap::new();
    for (id, name, color) in rows {
        by_fold
            .entry(fold(&name))
            .or_default()
            .push((name.clone(), color));
        names.insert(id, name);
    }
    let mut colors = Vec::new();
    for spellings in by_fold.values() {
        if spellings.len() > 1 {
            let list: Vec<&str> = spellings.iter().map(|(n, _)| n.as_str()).collect();
            problems.push(Problem::field(AttentionKind::TagMerged, &list.join(", ")));
        }
        if let Some(color) = spellings.iter().find_map(|(_, c)| c.clone()) {
            colors.push((spellings[0].0.clone(), color));
        }
    }
    Ok(Tags { names, colors })
}
