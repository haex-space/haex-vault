//! What an open encrypted folder holds (research R7): every version that opened, what failed, and
//! the tree built from them. Writes of this device update it at once and mark the tree stale; the
//! next [`super::OpenFolder::view`] builds it again, once for a whole transfer.

use std::collections::{HashMap, HashSet};

use super::format::Id;
use super::journal;
use super::load::LoadedFolder;
use super::tree::{self, Inputs, Tree, Version};

/// What a folder holds: every version that opened, what failed, and the tree built from them.
pub struct FolderView {
    pub(super) versions: HashMap<Id, Version>,
    pub(super) broken: Vec<Id>,
    pub(super) contents: HashMap<Id, u64>,
    pub(super) damaged: HashSet<Id>,
    pub tree: Tree,
    /// Files in the folder that are not part of the format (research R19).
    pub foreign: u32,
    pub(super) journal: journal::Journal,
    /// `tree` lags behind `versions` until the next [`OpenFolder::view`].
    pub(super) stale: bool,
}

impl FolderView {
    pub(super) fn new(fid: Id, loaded: LoadedFolder) -> Self {
        let mut view = Self {
            versions: loaded
                .versions
                .into_iter()
                .map(|row| {
                    (
                        row.sid,
                        Version {
                            sid: row.sid,
                            sidecar: row.sidecar,
                        },
                    )
                })
                .collect(),
            broken: loaded.broken,
            contents: loaded.contents,
            damaged: HashSet::new(),
            foreign: loaded.foreign,
            journal: journal::Journal::default(),
            stale: false,
            tree: Tree::build(&Inputs {
                fid,
                versions: &[],
                broken: &[],
                contents: None,
                damaged: &HashSet::new(),
            }),
        };
        view.rebuild(fid);
        view
    }

    /// Adds a version this device wrote (its content object, if any, of `content_len` bytes).
    /// The tree follows at the next [`OpenFolder::view`].
    pub fn add(&mut self, version: Version, content: Option<(Id, u64)>) {
        // The only version of a new entry hangs into the tree at once; anything else builds it.
        let new_entry = !self
            .versions
            .values()
            .any(|shown| shown.sidecar.entry == version.sidecar.entry);
        let appended = new_entry && !self.stale && self.tree.append(&version);
        let change = journal::Change::Add(Box::new(version), content);
        self.apply(&change);
        self.journal.record(change);
        self.stale |= !appended;
    }

    /// `change` without rebuilding the tree.
    pub(super) fn apply(&mut self, change: &journal::Change) {
        match change {
            journal::Change::Add(version, content) => {
                if let Some((oid, len)) = content {
                    self.contents.insert(*oid, *len);
                }
                self.versions.insert(version.sid, (**version).clone());
            }
            journal::Change::Remove(sids) => {
                for sid in sids {
                    if let Some(version) = self.versions.remove(sid) {
                        if let Some(facts) = tree::facts(&version) {
                            if !self.references(&facts.content, &[]) {
                                self.contents.remove(&facts.content);
                            }
                        }
                    }
                }
            }
        }
    }

    /// Drops versions this device deleted; their content objects go with them unless another
    /// version still points at one (a rename keeps the content).
    pub fn remove(&mut self, sids: &[Id]) {
        let change = journal::Change::Remove(sids.to_vec());
        self.apply(&change);
        self.journal.record(change);
        self.stale = true;
    }

    /// Whether a version of this view other than `except` points at the content object `oid`.
    pub fn references(&self, oid: &Id, except: &[Id]) -> bool {
        self.versions.values().any(|version| {
            !except.contains(&version.sid)
                && tree::facts(version).is_some_and(|facts| facts.content == *oid)
        })
    }

    /// The version `sid`, whether it shows or not.
    pub fn version(&self, sid: &Id) -> Option<&Version> {
        self.versions.get(sid)
    }

    /// The versions of `entry` another version of it names as `base`: replaced, never shown.
    pub fn superseded(&self, entry: &Id) -> Vec<Version> {
        let of_entry: Vec<&Version> = self
            .versions
            .values()
            .filter(|version| version.sidecar.entry == *entry)
            .collect();
        of_entry
            .iter()
            .filter(|version| {
                of_entry
                    .iter()
                    .any(|other| other.sidecar.base == Some(version.sid))
            })
            .map(|version| (*version).clone())
            .collect()
    }

    /// `gone` and the versions that would show again without them: a replaced version whose
    /// every successor is in the set (format "Ansicht": a version nobody names counts).
    pub fn with_replaced(&self, gone: &[Id]) -> Vec<Version> {
        let mut out: HashSet<Id> = gone.iter().copied().collect();
        let entries: HashSet<Id> = gone
            .iter()
            .filter_map(|sid| self.versions.get(sid))
            .map(|version| version.sidecar.entry)
            .collect();
        let candidates: Vec<&Version> = self
            .versions
            .values()
            .filter(|version| entries.contains(&version.sidecar.entry))
            .collect();
        loop {
            let mut grown = false;
            for version in &candidates {
                if out.contains(&version.sid) {
                    continue;
                }
                let mut successors = candidates
                    .iter()
                    .filter(|other| other.sidecar.base == Some(version.sid))
                    .peekable();
                if successors.peek().is_some() && successors.all(|other| out.contains(&other.sid)) {
                    out.insert(version.sid);
                    grown = true;
                }
            }
            if !grown {
                break;
            }
        }
        let mut versions: Vec<Version> = out
            .iter()
            .filter_map(|sid| self.versions.get(sid).cloned())
            .collect();
        // Older versions first: what stays after a failure never shows an older one again.
        versions.sort_by_key(|version| (version.sidecar.revision, version.sid));
        versions
    }

    pub(super) fn rebuild(&mut self, fid: Id) {
        self.stale = false;
        let mut versions: Vec<Version> = self.versions.values().cloned().collect();
        versions.sort_by_key(|version| version.sid);
        self.tree = Tree::build(&Inputs {
            fid,
            versions: &versions,
            broken: &self.broken,
            contents: Some(&self.contents),
            damaged: &self.damaged,
        });
    }
}
