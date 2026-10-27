//! The plan of a run (research R3): what has to happen so both places match the rule, built from
//! both listings and the state of the last success without touching a place. Every decision of a
//! run is made here, so the table of the spec is the table of the tests.
//!
//! The steps come in the order they run: folders, moves, copies, then deletions and the rest, so
//! an abort never costs data before the copy is there.

use std::collections::{BTreeMap, BTreeSet};

use super::ignore::Exclusions;
use super::model::{SyncRule, SyncRuleDeleteMode, SyncRuleDirection, SyncRuleSide};
use super::place::{Facts, Listing};
use super::store::{Seen, StateRow};

/// Hashes a run already knows, by side and path (research R5).
pub type Hashes = BTreeMap<(SyncRuleSide, String), String>;

/// Why the target of a one-way rule differs from the state (FR-010).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Deviation {
    /// Changed at the target since the last success.
    Changed,
    /// Deleted at the target since the last success.
    Deleted,
    /// At the target only; unknown to the rule until now.
    Added,
}

/// One step of a run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Creates the folder `path` on `side`.
    Folder { side: SyncRuleSide, path: String },
    /// Moves a file on `side` from `from` to `to`, as it moved on the other side (FR-009).
    Move {
        side: SyncRuleSide,
        from: String,
        to: String,
    },
    /// Moves the version of `path` on `side` to the conflict name `to` (FR-012); the other version
    /// is copied to `path` after.
    Aside {
        side: SyncRuleSide,
        path: String,
        to: String,
    },
    /// Copies `path` from `from` to the other side; `replaces` when a file is there.
    Copy {
        from: SyncRuleSide,
        path: String,
        replaces: bool,
    },
    /// Equal on both sides; only the state is written (FR-012a).
    Adopt { path: String, hash: String },
    /// Deleted on the other side; mirrored into a trash (FR-019).
    Delete { side: SyncRuleSide, path: String },
    /// Deleted on the other side; waits for the user (FR-021).
    Pending { side: SyncRuleSide, path: String },
    /// Deleted on the other side; stays on purpose (FR-020).
    Keep { path: String },
    /// The target of a one-way rule differs; it is left as it is (FR-010).
    Report { path: String, what: Deviation },
    /// Gone on both sides; the state of it goes.
    Forget { path: String },
}

impl Step {
    /// Where the step runs in the order of a plan.
    fn rank(&self) -> u8 {
        match self {
            Self::Folder { .. } => 0,
            Self::Move { .. } | Self::Aside { .. } => 1,
            Self::Copy { .. } => 2,
            _ => 3,
        }
    }
}

/// Everything a run would do.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    pub steps: Vec<Step>,
    /// Files whose hash decides a step; the run reads them and builds the plan again.
    pub hash_reads: Vec<(SyncRuleSide, String)>,
    /// Files and folders the exclusions leave out (FR-042).
    pub ignored: usize,
}

/// What a plan is built from.
pub struct Inputs<'a> {
    pub rule: &'a SyncRule,
    pub a: &'a Listing,
    pub b: &'a Listing,
    pub state: &'a [StateRow],
    pub exclusions: &'a Exclusions,
    pub hashes: &'a Hashes,
    /// The name a conflict copy of a path gets on a side (see [`super::conflict`]).
    pub conflict_name: &'a dyn Fn(SyncRuleSide, &str) -> String,
}

/// The plan for a rule. Only one-way rules (`AToB`) are planned so far; a rule `Both` plans
/// nothing and the run refuses it before it gets here.
pub fn build(inputs: &Inputs<'_>) -> Plan {
    let mut plan = Plan {
        ignored: ignored(inputs),
        ..Plan::default()
    };
    if inputs.rule.direction == SyncRuleDirection::AToB {
        OneWay::new(inputs, &mut plan).build();
    }
    plan.steps.sort_by_key(Step::rank);
    plan.hash_reads.sort();
    plan.hash_reads.dedup();
    plan
}

fn ignored(inputs: &Inputs<'_>) -> usize {
    [inputs.a, inputs.b]
        .iter()
        .flat_map(|listing| {
            listing
                .files
                .keys()
                .map(|path| (path, false))
                .chain(listing.folders.iter().map(|path| (path, true)))
        })
        .filter(|(path, is_dir)| inputs.exclusions.excludes(path, *is_dir))
        .count()
}

fn same(facts: &Facts, seen: Seen) -> bool {
    facts.size == seen.size && facts.mtime_ms == seen.mtime_ms
}

/// The plan of a rule A → B (FR-010): A is the source, B the target.
struct OneWay<'i, 'p> {
    inputs: &'i Inputs<'i>,
    plan: &'p mut Plan,
    state: BTreeMap<&'i str, &'i StateRow>,
    /// Paths a move already decided.
    done: BTreeSet<String>,
}

impl<'i, 'p> OneWay<'i, 'p> {
    fn new(inputs: &'i Inputs<'i>, plan: &'p mut Plan) -> Self {
        let state = inputs
            .state
            .iter()
            .map(|row| (row.path.as_str(), row))
            .collect();
        Self {
            inputs,
            plan,
            state,
            done: BTreeSet::new(),
        }
    }

    fn excluded(&self, path: &str) -> bool {
        self.inputs.exclusions.excludes(path, false)
    }

    /// The hash of `path` on `side` when known: read in this run, or given by the place.
    fn hash(&self, side: SyncRuleSide, path: &str) -> Option<&str> {
        let listing = match side {
            SyncRuleSide::A => self.inputs.a,
            SyncRuleSide::B => self.inputs.b,
        };
        self.inputs
            .hashes
            .get(&(side, path.to_owned()))
            .map(String::as_str)
            .or_else(|| listing.files.get(path)?.hash.as_deref())
    }

    /// The hash of `path` on `side`, or a note that the run has to read it.
    fn need_hash(&mut self, side: SyncRuleSide, path: &str) -> Option<String> {
        let hash = self.hash(side, path).map(str::to_owned);
        if hash.is_none() {
            self.plan.hash_reads.push((side, path.to_owned()));
        }
        hash
    }

    fn push(&mut self, step: Step) {
        self.plan.steps.push(step);
    }

    fn build(&mut self) {
        self.folders();
        self.moves();
        let inputs = self.inputs;
        let paths: BTreeSet<&str> = inputs
            .a
            .files
            .keys()
            .chain(inputs.b.files.keys())
            .map(String::as_str)
            .chain(self.state.keys().copied())
            .collect();
        for path in paths {
            if self.excluded(path) || self.done.contains(path) {
                continue;
            }
            self.file(path);
        }
    }

    /// Folders of A that B lacks, also empty ones (research R3).
    fn folders(&mut self) {
        let inputs = self.inputs;
        for folder in &inputs.a.folders {
            if !inputs.exclusions.excludes(folder, true) && !inputs.b.folders.contains(folder) {
                self.push(Step::Folder {
                    side: SyncRuleSide::B,
                    path: folder.clone(),
                });
            }
        }
    }

    /// Files that moved on A (FR-009): gone from their known path, new at a path with the same size
    /// and time, and the content confirmed by the hash the state holds. Without that hash, or with
    /// more than one match, they are a new and a deleted file.
    fn moves(&mut self) {
        let inputs = self.inputs;
        let vanished: Vec<&StateRow> = self
            .state
            .values()
            .copied()
            .filter(|row| row.a.is_some() && !inputs.a.files.contains_key(&row.path))
            .filter(|row| !self.excluded(&row.path))
            .collect();
        for row in vanished {
            let (Some(seen), Some(hash)) = (row.a, row.hash.as_deref()) else {
                continue;
            };
            // B must still hold the old file as the rule left it, and not the new path yet.
            let target_kept = match (inputs.b.files.get(&row.path), row.b) {
                (Some(facts), Some(seen_b)) => same(facts, seen_b),
                _ => false,
            };
            if !target_kept {
                continue;
            }
            let candidates: Vec<&String> = inputs
                .a
                .files
                .iter()
                .filter(|(path, facts)| {
                    same(facts, seen)
                        && !self.done.contains(path.as_str())
                        && !self.excluded(path)
                        && self
                            .state
                            .get(path.as_str())
                            .is_none_or(|known| known.a.is_none())
                        && !inputs.b.files.contains_key(path.as_str())
                })
                .map(|(path, _)| path)
                .collect();
            let mut confirmed = Vec::new();
            for path in candidates {
                if let Some(found) = self.need_hash(SyncRuleSide::A, path) {
                    if found == hash {
                        confirmed.push(path.clone());
                    }
                }
            }
            if let [to] = confirmed.as_slice() {
                self.done.insert(row.path.clone());
                self.done.insert(to.clone());
                self.push(Step::Move {
                    side: SyncRuleSide::B,
                    from: row.path.clone(),
                    to: to.clone(),
                });
            }
        }
    }

    fn file(&mut self, path: &str) {
        let inputs = self.inputs;
        let source = inputs.a.files.get(path);
        let target = inputs.b.files.get(path);
        let row = self.state.get(path).copied();
        let known_a = row.and_then(|row| row.a);
        let known_b = row.and_then(|row| row.b);
        let target_same = match (target, known_b) {
            (Some(facts), Some(seen)) => same(facts, seen),
            _ => false,
        };
        match (source, known_a) {
            // New at A, or only known at B so far.
            (Some(_), None) => match target {
                None => self.push(Step::Copy {
                    from: SyncRuleSide::A,
                    path: path.to_owned(),
                    replaces: false,
                }),
                Some(_) => self.first_meeting(path),
            },
            (Some(facts), Some(seen)) => {
                if same(facts, seen) {
                    match target {
                        None if known_b.is_some() => self.push(Step::Report {
                            path: path.to_owned(),
                            what: Deviation::Deleted,
                        }),
                        None => self.push(Step::Copy {
                            from: SyncRuleSide::A,
                            path: path.to_owned(),
                            replaces: false,
                        }),
                        Some(_) if !target_same => self.push(Step::Report {
                            path: path.to_owned(),
                            what: Deviation::Changed,
                        }),
                        Some(_) => {}
                    }
                } else {
                    match target {
                        None => self.push(Step::Copy {
                            from: SyncRuleSide::A,
                            path: path.to_owned(),
                            replaces: false,
                        }),
                        Some(_) if target_same => self.push(Step::Copy {
                            from: SyncRuleSide::A,
                            path: path.to_owned(),
                            replaces: true,
                        }),
                        // Changed on both: the target's version goes aside, nothing is lost.
                        Some(_) => self.aside_and_copy(path),
                    }
                }
            }
            // Deleted at A since the last success.
            (None, Some(_)) => match target {
                None => self.push(Step::Forget {
                    path: path.to_owned(),
                }),
                Some(_) if row.is_some_and(|row| row.kept) => {}
                Some(_) if target_same => self.deleted_at_source(path),
                // The target changed meanwhile; it stays (FR-013).
                Some(_) => self.push(Step::Report {
                    path: path.to_owned(),
                    what: Deviation::Changed,
                }),
            },
            (None, None) => match target {
                Some(_) if !target_same => self.push(Step::Report {
                    path: path.to_owned(),
                    what: Deviation::Added,
                }),
                Some(_) => {}
                None => self.push(Step::Forget {
                    path: path.to_owned(),
                }),
            },
        }
    }

    /// A file on both sides that the rule has not met before (FR-012a): equal content is adopted,
    /// other content makes a conflict copy at the target.
    fn first_meeting(&mut self, path: &str) {
        let inputs = self.inputs;
        let (Some(source), Some(target)) = (inputs.a.files.get(path), inputs.b.files.get(path))
        else {
            return;
        };
        if source.size != target.size {
            self.aside_and_copy(path);
            return;
        }
        let a = self.need_hash(SyncRuleSide::A, path);
        let b = self.need_hash(SyncRuleSide::B, path);
        match (a, b) {
            (Some(a), Some(b)) if a == b => self.push(Step::Adopt {
                path: path.to_owned(),
                hash: a,
            }),
            (Some(_), Some(_)) => self.aside_and_copy(path),
            // The run reads the hashes and plans again.
            _ => {}
        }
    }

    /// The target's version moves to a conflict name; then the source's version is copied.
    fn aside_and_copy(&mut self, path: &str) {
        let aside = (self.inputs.conflict_name)(SyncRuleSide::B, path);
        self.push(Step::Aside {
            side: SyncRuleSide::B,
            path: path.to_owned(),
            to: aside,
        });
        self.push(Step::Copy {
            from: SyncRuleSide::A,
            path: path.to_owned(),
            replaces: false,
        });
    }

    fn deleted_at_source(&mut self, path: &str) {
        let path = path.to_owned();
        self.push(match self.inputs.rule.delete_mode {
            SyncRuleDeleteMode::Mirror => Step::Delete {
                side: SyncRuleSide::B,
                path,
            },
            SyncRuleDeleteMode::Keep => Step::Keep { path },
            SyncRuleDeleteMode::Ask => Step::Pending {
                side: SyncRuleSide::B,
                path,
            },
        });
    }
}

#[cfg(test)]
#[path = "plan_tests.rs"]
mod tests;
