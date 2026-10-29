//! The plan of a run (research R3): what has to happen so both places match the rule, built from
//! both listings and the state of the last success without touching a place. Every decision of a
//! run is made here, so the table of the spec is the table of the tests.
//!
//! The steps come in the order they run: folders, moves, copies, then deletions and the rest, so
//! an abort never costs data before the copy is there.

use std::collections::{BTreeMap, BTreeSet};

use super::ignore::{Exclusions, IGNORE_FILE};
use super::model::{SyncRule, SyncRuleDeleteMode, SyncRuleDirection, SyncRuleSide};
use super::place::{split, Facts, Listing};
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
    /// Changed on both sides of a two-way rule (FR-012): the version of `keep` keeps the name, the
    /// other moves to `aside` on its side, and both sides get both files.
    Conflict {
        path: String,
        keep: SyncRuleSide,
        aside: String,
    },
    /// The exclusion files of a two-way rule differ (FR-039): both go aside as conflict copies on
    /// both sides, and the union of their patterns becomes the exclusion file of both.
    MergeIgnore { aside_a: String, aside_b: String },
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
            Self::Copy { .. } | Self::Conflict { .. } | Self::MergeIgnore { .. } => 2,
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
    /// The name a conflict copy of a path gets, free on both sides and not one of the names given
    /// (see [`super::conflict`]).
    pub conflict_name: &'a dyn Fn(&str, &[String]) -> String,
}

/// The plan for a rule: A → B (`BToA` is stored as `AToB`) or both ways.
pub fn build(inputs: &Inputs<'_>) -> Plan {
    let mut plan = Plan {
        ignored: ignored(inputs),
        ..Plan::default()
    };
    let mut planner = Planner::new(inputs, &mut plan);
    match inputs.rule.direction {
        SyncRuleDirection::Both => planner.two_way(),
        SyncRuleDirection::AToB | SyncRuleDirection::BToA => planner.one_way(),
    }
    plan.steps.sort_by_key(Step::rank);
    plan.hash_reads.sort();
    plan.hash_reads.dedup();
    plan
}

fn ignored(inputs: &Inputs<'_>) -> usize {
    excluded_count(inputs.exclusions, inputs.a, inputs.b)
}

/// How many files and folders of both listings `exclusions` leave out (FR-042).
pub fn excluded_count(exclusions: &Exclusions, a: &Listing, b: &Listing) -> usize {
    [a, b]
        .iter()
        .flat_map(|listing| {
            listing
                .files
                .keys()
                .map(|path| (path, false))
                .chain(listing.folders.iter().map(|path| (path, true)))
        })
        .filter(|(path, is_dir)| exclusions.excludes(path, *is_dir))
        .count()
}

fn same(facts: &Facts, seen: Seen) -> bool {
    facts.size == seen.size && facts.mtime_ms == seen.mtime_ms
}

/// What became of a file on one side since the last success (FR-008: only against the state of
/// the same side).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Was {
    Same,
    Changed,
    /// There, and the state does not know it on this side.
    New,
    /// Known on this side and gone.
    Gone,
    /// Neither there nor known.
    Never,
}

/// Plans a rule: A → B (FR-010, A is the source, B the target) or both ways (FR-011).
struct Planner<'i, 'p> {
    inputs: &'i Inputs<'i>,
    plan: &'p mut Plan,
    state: BTreeMap<&'i str, &'i StateRow>,
    /// Paths a move already decided.
    done: BTreeSet<String>,
}

impl<'i, 'p> Planner<'i, 'p> {
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

    fn listing(&self, side: SyncRuleSide) -> &'i Listing {
        match side {
            SyncRuleSide::A => self.inputs.a,
            SyncRuleSide::B => self.inputs.b,
        }
    }

    /// The hash of `path` on `side` when known: read in this run, or given by the place.
    fn hash(&self, side: SyncRuleSide, path: &str) -> Option<&str> {
        self.inputs
            .hashes
            .get(&(side, path.to_owned()))
            .map(String::as_str)
            .or_else(|| self.listing(side).files.get(path)?.hash.as_deref())
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

    /// Every path of both listings and the state that is not excluded.
    fn paths(&self) -> BTreeSet<&'i str> {
        let inputs = self.inputs;
        inputs
            .a
            .files
            .keys()
            .chain(inputs.b.files.keys())
            .map(String::as_str)
            .chain(self.state.keys().copied())
            .filter(|path| !self.excluded(path))
            .collect()
    }

    fn one_way(&mut self) {
        self.forget_excluded();
        self.folders();
        self.moves(SyncRuleSide::A);
        for path in self.paths() {
            if !self.done.contains(path) {
                self.file(path);
            }
        }
    }

    fn two_way(&mut self) {
        self.forget_excluded();
        self.moves(SyncRuleSide::A);
        self.moves(SyncRuleSide::B);
        for path in self.paths() {
            if !self.done.contains(path) {
                self.both_ways(path);
            }
        }
        self.parent_folders();
    }

    /// A file excluded since the last success stays as it is on both sides and drops from the state,
    /// so it is new to the rule once the exclusion goes (FR-040).
    fn forget_excluded(&mut self) {
        let excluded: Vec<String> = self
            .state
            .keys()
            .filter(|path| self.excluded(path))
            .map(|path| (*path).to_owned())
            .collect();
        for path in excluded {
            self.push(Step::Forget { path });
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

    /// Files that moved on `side` (FR-009): gone from their known path, new at a path with the same
    /// size and time, and the content confirmed by the hash the state holds. Without that hash, or
    /// with more than one match, they are a new and a deleted file.
    fn moves(&mut self, side: SyncRuleSide) {
        let (here, there) = (self.listing(side), self.listing(side.other()));
        let vanished: Vec<&StateRow> = self
            .state
            .values()
            .copied()
            .filter(|row| row.seen(side).is_some() && !here.files.contains_key(&row.path))
            .filter(|row| !self.excluded(&row.path) && !self.done.contains(&row.path))
            .collect();
        for row in vanished {
            let (Some(seen), Some(hash)) = (row.seen(side), row.hash.as_deref()) else {
                continue;
            };
            // The other side must still hold the old file as the rule left it, and not the new
            // path yet.
            let other_kept = match (there.files.get(&row.path), row.seen(side.other())) {
                (Some(facts), Some(seen_there)) => same(facts, seen_there),
                _ => false,
            };
            if !other_kept {
                continue;
            }
            let candidates: Vec<&String> = here
                .files
                .iter()
                .filter(|(path, facts)| {
                    same(facts, seen)
                        && !self.done.contains(path.as_str())
                        && !self.excluded(path)
                        && self
                            .state
                            .get(path.as_str())
                            .is_none_or(|known| known.seen(side).is_none())
                        && !there.files.contains_key(path.as_str())
                })
                .map(|(path, _)| path)
                .collect();
            let mut confirmed = Vec::new();
            for path in candidates {
                if let Some(found) = self.need_hash(side, path) {
                    if found == hash {
                        confirmed.push(path.clone());
                    }
                }
            }
            if let [to] = confirmed.as_slice() {
                self.done.insert(row.path.clone());
                self.done.insert(to.clone());
                self.push(Step::Move {
                    side: side.other(),
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
    /// other content makes a conflict copy.
    fn first_meeting(&mut self, path: &str) {
        let inputs = self.inputs;
        let (Some(source), Some(target)) = (inputs.a.files.get(path), inputs.b.files.get(path))
        else {
            return;
        };
        if source.size != target.size {
            self.conflict(path);
            return;
        }
        let a = self.need_hash(SyncRuleSide::A, path);
        let b = self.need_hash(SyncRuleSide::B, path);
        match (a, b) {
            (Some(a), Some(b)) if a == b => self.push(Step::Adopt {
                path: path.to_owned(),
                hash: a,
            }),
            (Some(_), Some(_)) => self.conflict(path),
            // The run reads the hashes and plans again.
            _ => {}
        }
    }

    /// Different versions on both sides: one-way, the target's goes aside; two-way, see
    /// [`Self::both_changed`].
    fn conflict(&mut self, path: &str) {
        if self.inputs.rule.direction == SyncRuleDirection::Both {
            self.both_changed(path);
        } else {
            self.aside_and_copy(path);
        }
    }

    /// The target's version moves to a conflict name; then the source's version is copied.
    fn aside_and_copy(&mut self, path: &str) {
        let aside = (self.inputs.conflict_name)(path, &[]);
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
        self.deleted(SyncRuleSide::B, path);
    }

    /// Deleted on the other side of `side`, unchanged on `side`: the delete mode decides (FR-019 to
    /// FR-021).
    fn deleted(&mut self, side: SyncRuleSide, path: &str) {
        let path = path.to_owned();
        self.push(match self.inputs.rule.delete_mode {
            SyncRuleDeleteMode::Mirror => Step::Delete { side, path },
            SyncRuleDeleteMode::Keep => Step::Keep { path },
            SyncRuleDeleteMode::Ask => Step::Pending { side, path },
        });
    }

    fn was(&self, side: SyncRuleSide, path: &str, row: Option<&StateRow>) -> Was {
        let known = row.and_then(|row| row.seen(side));
        match (self.listing(side).files.get(path), known) {
            (Some(facts), Some(seen)) if same(facts, seen) => Was::Same,
            (Some(_), Some(_)) => Was::Changed,
            (Some(_), None) => Was::New,
            (None, Some(_)) => Was::Gone,
            (None, None) => Was::Never,
        }
    }

    fn copy(&mut self, from: SyncRuleSide, path: &str, replaces: bool) {
        self.push(Step::Copy {
            from,
            path: path.to_owned(),
            replaces,
        });
    }

    /// One path of a two-way rule (FR-011 to FR-013).
    fn both_ways(&mut self, path: &str) {
        use SyncRuleSide::{A, B};
        use Was::{Changed, Gone, Never, New, Same};
        let row = self.state.get(path).copied();
        let (a, b) = (self.was(A, path, row), self.was(B, path, row));
        // "Keep" left it on one side on purpose; it is not copied back (FR-020).
        if row.is_some_and(|row| row.kept)
            && matches!((a, b), (Gone, Same | Changed) | (Same | Changed, Gone))
        {
            return;
        }
        match (a, b) {
            (Same, Same) => {}
            (Changed, Same) => self.copy(A, path, true),
            (Same, Changed) => self.copy(B, path, true),
            (Changed, Changed) => self.both_changed(path),
            // Only on one side, or deleted on one and changed on the other (FR-013).
            (Same | Changed | New, Never) | (Changed | New, Gone) => self.copy(A, path, false),
            (Never, Same | Changed | New) | (Gone, Changed | New) => self.copy(B, path, false),
            (Same, Gone) => self.deleted(A, path),
            (Gone, Same) => self.deleted(B, path),
            (Gone | Never, Gone | Never) => self.push(Step::Forget {
                path: path.to_owned(),
            }),
            // On both sides and new to the state on at least one.
            (New, Same | Changed | New) | (Same | Changed, New) => self.first_meeting(path),
        }
    }

    /// Changed on both sides of a two-way rule (FR-012): the newer version keeps the name; the only
    /// comparison of times across sides. The exclusion file merges (FR-039).
    fn both_changed(&mut self, path: &str) {
        let conflict_name = self.inputs.conflict_name;
        if path == IGNORE_FILE {
            let aside_a = conflict_name(path, &[]);
            let aside_b = conflict_name(path, std::slice::from_ref(&aside_a));
            self.push(Step::MergeIgnore { aside_a, aside_b });
            return;
        }
        let (Some(a), Some(b)) = (self.inputs.a.files.get(path), self.inputs.b.files.get(path))
        else {
            return;
        };
        let keep = if b.mtime_ms > a.mtime_ms {
            SyncRuleSide::B
        } else {
            SyncRuleSide::A
        };
        self.push(Step::Conflict {
            path: path.to_owned(),
            keep,
            aside: conflict_name(path, &[]),
        });
    }

    /// The folders a two-way copy goes into, where its side lacks them. A folder is not part of the
    /// state, so an empty one is not copied: a folder deleted on one side would come back.
    fn parent_folders(&mut self) {
        let mut wanted = BTreeSet::new();
        for step in &self.plan.steps {
            if let Step::Copy { from, path, .. } = step {
                let (folder, _) = split(path);
                let to = from.other();
                if !folder.is_empty() && !self.listing(to).folders.contains(folder) {
                    wanted.insert((to, folder.to_owned()));
                }
            }
        }
        for (side, path) in wanted {
            self.push(Step::Folder { side, path });
        }
    }
}

#[cfg(test)]
#[path = "plan_tests.rs"]
mod tests;
