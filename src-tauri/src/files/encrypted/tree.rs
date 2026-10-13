//! The view of an encrypted folder, built from its sidecars alone (format "Regeln für Leser und
//! Schreiber", research R7):
//!
//! - Of the versions of one `entry`, those that no other version names as `base` count. Two of
//!   them (the same `base`) are a conflict: both show, the one with the smaller `written` as
//!   "Name (Konflikt JJJJ-MM-TT hh-mm)". Different entries with one name in one parent are named the
//!   same way.
//! - An entry whose parent is missing (or that only reaches itself) shows in the virtual folder
//!   [`RECOVERED`] at the root.
//! - A sidecar that does not open shows at the root as a damaged entry named by its `sid`; a file
//!   whose content object is missing or has the wrong length, or failed a read, is damaged too.
//!
//! Pure: no I/O, so the rules are tested without a storage.

use std::collections::{HashMap, HashSet};

use super::format::content::ciphertext_len;
use super::format::sidecar::{FileFacts, Sidecar, SidecarKind};
use super::format::{encode_id, Id};
use crate::files::kind::mime_for;
use crate::files::{EncryptedInfo, EncryptedRole, EncryptedState, Entry, EntryKind};

/// The virtual folder of entries whose parent is gone.
pub const RECOVERED: &str = "Wiederhergestellt";

/// One version of an entry: a sidecar and its `sid`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    pub sid: Id,
    pub sidecar: Sidecar,
}

/// What a node of the tree stands for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    Root,
    Recovered,
    Version {
        version: Box<Version>,
        damaged: bool,
    },
    /// A sidecar that did not open.
    Broken {
        sid: Id,
    },
}

/// One node: its name as the window sees it and what it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub name: String,
    pub conflict: bool,
    pub item: Item,
}

/// What the tree is built from.
pub struct Inputs<'a> {
    pub fid: Id,
    pub versions: &'a [Version],
    pub broken: &'a [Id],
    /// The content objects of the folder by `oid` with their length; `None` when not listed.
    pub contents: Option<&'a HashMap<Id, u64>>,
    /// Sidecars whose content failed a read.
    pub damaged: &'a HashSet<Id>,
}

/// The tree of one encrypted folder; node 0 is its root.
#[derive(Debug, Clone)]
pub struct Tree {
    fid: Id,
    nodes: Vec<Node>,
    children: Vec<Vec<usize>>,
}

const ROOT: usize = 0;

impl Tree {
    pub fn build(inputs: &Inputs<'_>) -> Self {
        let mut nodes = vec![Node {
            name: "/".to_owned(),
            conflict: false,
            item: Item::Root,
        }];
        // The versions that count, newest first within each entry.
        let mut by_entry: HashMap<Id, Vec<&Version>> = HashMap::new();
        for version in inputs.versions {
            by_entry
                .entry(version.sidecar.entry)
                .or_default()
                .push(version);
        }
        let mut current: Vec<(&Version, bool)> = Vec::new();
        for versions in by_entry.values() {
            let based: HashSet<Id> = versions.iter().filter_map(|v| v.sidecar.base).collect();
            let mut heads: Vec<&Version> = versions
                .iter()
                .copied()
                .filter(|v| !based.contains(&v.sid))
                .collect();
            if heads.is_empty() {
                // Versions that only name each other: the newest one counts.
                heads = versions.clone();
                heads.sort_by(|a, b| newest_first(a, b));
                heads.truncate(1);
            }
            heads.sort_by(|a, b| newest_first(a, b));
            current.extend(heads.into_iter().enumerate().map(|(i, v)| (v, i > 0)));
        }
        current.sort_by_key(|(version, _)| version.sid);
        let mut folder_of: HashMap<Id, usize> = HashMap::new();
        let mut copies = Vec::new();
        for (version, copy) in current {
            let index = nodes.len();
            let damaged = is_damaged(version, inputs);
            if !copy && matches!(version.sidecar.kind, SidecarKind::Folder) {
                folder_of.insert(version.sidecar.entry, index);
            }
            copies.push(copy);
            nodes.push(Node {
                name: version.sidecar.name.clone(),
                conflict: false,
                item: Item::Version {
                    version: Box::new(version.clone()),
                    damaged,
                },
            });
        }
        let recovered = nodes.len();
        nodes.push(Node {
            name: RECOVERED.to_owned(),
            conflict: false,
            item: Item::Recovered,
        });
        let mut parent = vec![ROOT; nodes.len()];
        for (index, node) in nodes.iter().enumerate() {
            if let Item::Version { version, .. } = &node.item {
                let wanted = version.sidecar.parent;
                parent[index] = if wanted == inputs.fid {
                    ROOT
                } else {
                    folder_of.get(&wanted).copied().unwrap_or(recovered)
                };
            }
        }
        // Entries that only reach each other (moved into one another on two devices) are cut
        // loose, the one with the smallest sid first.
        for index in 1..recovered {
            if !reaches_root(&parent, index, recovered) {
                parent[index] = recovered;
            }
        }
        for sid in inputs.broken {
            parent.push(ROOT);
            nodes.push(Node {
                name: encode_id(sid),
                conflict: false,
                item: Item::Broken { sid: *sid },
            });
        }
        let mut children = vec![Vec::new(); nodes.len()];
        for (index, &up) in parent.iter().enumerate().skip(1) {
            if index != recovered {
                children[up].push(index);
            }
        }
        if !children[recovered].is_empty() {
            children[ROOT].push(recovered);
        }
        let mut tree = Self {
            fid: inputs.fid,
            nodes,
            children,
        };
        tree.name_children(&copies);
        tree
    }

    /// Gives every child of every folder a name of its own (format: conflict naming).
    fn name_children(&mut self, copies: &[bool]) {
        for index in 1..=copies.len() {
            if copies[index - 1] {
                let written = self.written(index);
                let node = &mut self.nodes[index];
                node.name = conflict_name(&node.name, written, is_file(&node.item), 0);
                node.conflict = true;
            }
        }
        for parent in 0..self.children.len() {
            let mut order = self.children[parent].clone();
            order.sort_by(|&a, &b| {
                self.written(b)
                    .cmp(&self.written(a))
                    .then_with(|| self.nodes[a].name.cmp(&self.nodes[b].name))
            });
            let mut taken = HashSet::new();
            for index in order {
                let written = self.written(index);
                let node = &mut self.nodes[index];
                let mut name = node.name.clone();
                let mut n = 0;
                while taken.contains(&name) {
                    name = conflict_name(&node.name, written, is_file(&node.item), n);
                    node.conflict = true;
                    n += 1;
                }
                taken.insert(name.clone());
                node.name = name;
            }
        }
    }

    fn written(&self, index: usize) -> i64 {
        match &self.nodes[index].item {
            Item::Version { version, .. } => i64::try_from(version.sidecar.written).unwrap_or(0),
            _ => 0,
        }
    }

    pub fn node(&self, index: usize) -> &Node {
        &self.nodes[index]
    }

    pub fn children(&self, index: usize) -> &[usize] {
        &self.children[index]
    }

    /// The node at `parts` below the root.
    pub fn find<S: AsRef<str>>(&self, parts: &[S]) -> Option<usize> {
        let mut at = ROOT;
        for part in parts {
            at = *self.children[at]
                .iter()
                .find(|&&child| self.nodes[child].name == part.as_ref())?;
        }
        Some(at)
    }

    /// The node that shows the version `sid`.
    pub fn find_sid(&self, sid: &Id) -> Option<usize> {
        self.nodes.iter().position(|node| match &node.item {
            Item::Version { version, .. } => version.sid == *sid,
            _ => false,
        })
    }

    /// Whether the node holds entries (the root, the virtual folder, a folder entry).
    pub fn is_dir(&self, index: usize) -> bool {
        !is_file(&self.nodes[index].item)
    }

    /// The entry id children of this node name as their parent; `None` for the virtual folder and
    /// for files.
    pub fn parent_id(&self, index: usize) -> Option<Id> {
        match &self.nodes[index].item {
            Item::Root => Some(self.fid),
            Item::Version { version, .. } if !is_file(&self.nodes[index].item) => {
                Some(version.sidecar.entry)
            }
            _ => None,
        }
    }

    /// The node as an entry of the window at `path`.
    pub fn entry(&self, index: usize, path: String) -> Entry {
        let node = &self.nodes[index];
        let (kind, size, modified, mime, damaged) = match &node.item {
            Item::Root | Item::Recovered => (EntryKind::Dir, None, None, None, false),
            Item::Broken { .. } => (EntryKind::File, None, None, None, true),
            Item::Version { version, damaged } => match &version.sidecar.kind {
                SidecarKind::Folder => (
                    EntryKind::Dir,
                    None,
                    Some(version.sidecar.written),
                    None,
                    *damaged,
                ),
                SidecarKind::File(facts) => (
                    EntryKind::File,
                    Some(facts.size),
                    Some(facts.modified),
                    facts
                        .content_type
                        .clone()
                        .or_else(|| mime_for(&node.name).map(str::to_owned)),
                    *damaged,
                ),
            },
        };
        Entry {
            hidden: node.name.starts_with('.'),
            name: node.name.clone(),
            path,
            kind,
            size,
            modified_ms: modified.and_then(|ms| i64::try_from(ms).ok()),
            mime,
            symlink: false,
            no_access: false,
            holzi_owned: false,
            encrypted: Some(EncryptedInfo {
                role: EncryptedRole::Inside,
                state: if damaged {
                    EncryptedState::Damaged
                } else {
                    EncryptedState::Readable
                },
                foreign: 0,
            }),
            conflict: node.conflict,
        }
    }

    /// Every node below `index` with the names of its place below it, parents before children.
    pub fn below(&self, index: usize) -> Vec<(usize, Vec<String>)> {
        let mut out = Vec::new();
        let mut stack: Vec<(usize, Vec<String>)> = self.children[index]
            .iter()
            .rev()
            .map(|&child| (child, vec![self.nodes[child].name.clone()]))
            .collect();
        while let Some((at, rel)) = stack.pop() {
            for &child in self.children[at].iter().rev() {
                let mut deeper = rel.clone();
                deeper.push(self.nodes[child].name.clone());
                stack.push((child, deeper));
            }
            out.push((at, rel));
        }
        out
    }
}

fn newest_first(a: &Version, b: &Version) -> std::cmp::Ordering {
    b.sidecar
        .written
        .cmp(&a.sidecar.written)
        .then_with(|| a.sid.cmp(&b.sid))
}

fn is_file(item: &Item) -> bool {
    match item {
        Item::Version { version, .. } => matches!(version.sidecar.kind, SidecarKind::File(_)),
        Item::Broken { .. } => true,
        Item::Root | Item::Recovered => false,
    }
}

/// The facts of a file version.
pub fn facts(version: &Version) -> Option<&FileFacts> {
    match &version.sidecar.kind {
        SidecarKind::File(facts) => Some(facts),
        SidecarKind::Folder => None,
    }
}

fn is_damaged(version: &Version, inputs: &Inputs<'_>) -> bool {
    if inputs.damaged.contains(&version.sid) {
        return true;
    }
    match (facts(version), inputs.contents) {
        (Some(facts), Some(contents)) => {
            contents.get(&facts.content) != Some(&ciphertext_len(facts.size))
        }
        _ => false,
    }
}

/// Whether `index` reaches the root through its parents (the virtual folder sits at the root).
fn reaches_root(parent: &[usize], index: usize, recovered: usize) -> bool {
    let mut at = index;
    for _ in 0..parent.len() {
        if at == ROOT || at == recovered {
            return true;
        }
        at = parent[at];
    }
    false
}

/// `name` as a conflict copy written at `written_ms` (UTC): "Name (Konflikt JJJJ-MM-TT hh-mm)",
/// before the extension of a file; `n` > 0 adds a number for copies of the same minute.
pub fn conflict_name(name: &str, written_ms: i64, file: bool, n: usize) -> String {
    let stamp = crate::passwords::clock::format_millis(written_ms);
    let label = format!(
        "Konflikt {} {}-{}{}",
        &stamp[..10],
        &stamp[11..13],
        &stamp[14..16],
        if n > 0 {
            format!(" {}", n + 1)
        } else {
            String::new()
        }
    );
    let (stem, ext) = match name.rfind('.') {
        Some(dot) if file && dot > 0 => name.split_at(dot),
        _ => (name, ""),
    };
    format!("{stem} ({label}){ext}")
}

#[cfg(test)]
#[path = "tree_tests.rs"]
mod tests;
