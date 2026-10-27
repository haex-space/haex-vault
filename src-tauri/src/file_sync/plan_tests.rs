//! The plan of a one-way rule A → B (spec 061 Story 1, FR-009, FR-010, FR-012a, FR-013, FR-019 to
//! FR-021): one test per row of the table, plus moves, first meetings and the order of the steps.

use super::*;
use crate::file_sync::model::{SyncRuleDeleteMode, SyncRuleDirection, SyncRulePlace};
use crate::files::SourceRef;

fn rule(delete_mode: SyncRuleDeleteMode) -> SyncRule {
    let place = |path: &str| SyncRulePlace {
        source: SourceRef::Device,
        path: path.to_owned(),
    };
    SyncRule {
        id: "r".to_owned(),
        name: "r".to_owned(),
        place_a: place("/a"),
        place_b: place("/b"),
        direction: SyncRuleDirection::AToB,
        delete_mode,
        interval_secs: 900,
        enabled: true,
        allow_decrypted: false,
        created_at: String::new(),
        updated_at: String::new(),
        last_run_at: None,
        last_outcome: None,
        last_error: None,
    }
}

fn facts(size: u64, mtime_ms: i64) -> Facts {
    Facts {
        size,
        mtime_ms,
        hash: None,
    }
}

fn listing(files: &[(&str, u64, i64)]) -> Listing {
    let mut listing = Listing::default();
    for (path, size, mtime) in files {
        listing
            .files
            .insert((*path).to_owned(), facts(*size, *mtime));
    }
    listing
}

fn seen(size: u64, mtime_ms: i64) -> Option<Seen> {
    Some(Seen { size, mtime_ms })
}

fn row(path: &str, a: Option<Seen>, b: Option<Seen>) -> StateRow {
    StateRow {
        path: path.to_owned(),
        a,
        b,
        kept: false,
        hash: None,
    }
}

struct Case {
    rule: SyncRule,
    a: Listing,
    b: Listing,
    state: Vec<StateRow>,
    hashes: Hashes,
    ignore: Option<&'static str>,
}

impl Case {
    fn new(a: Listing, b: Listing, state: Vec<StateRow>) -> Self {
        Self {
            rule: rule(SyncRuleDeleteMode::Keep),
            a,
            b,
            state,
            hashes: Hashes::new(),
            ignore: None,
        }
    }

    fn mode(mut self, mode: SyncRuleDeleteMode) -> Self {
        self.rule.delete_mode = mode;
        self
    }

    fn hash(mut self, side: SyncRuleSide, path: &str, hash: &str) -> Self {
        self.hashes.insert((side, path.to_owned()), hash.to_owned());
        self
    }

    fn plan(&self) -> Plan {
        let exclusions = Exclusions::new(self.ignore, None).unwrap();
        build(&Inputs {
            rule: &self.rule,
            a: &self.a,
            b: &self.b,
            state: &self.state,
            exclusions: &exclusions,
            hashes: &self.hashes,
            conflict_name: &|_, path| format!("{path}.conflict"),
        })
    }
}

fn copy(path: &str, replaces: bool) -> Step {
    Step::Copy {
        from: SyncRuleSide::A,
        path: path.to_owned(),
        replaces,
    }
}

fn report(path: &str, what: Deviation) -> Step {
    Step::Report {
        path: path.to_owned(),
        what,
    }
}

#[test]
fn a_new_file_is_copied() {
    let plan = Case::new(listing(&[("x", 1, 1)]), listing(&[]), vec![]).plan();
    assert_eq!(plan.steps, [copy("x", false)]);
}

#[test]
fn a_changed_file_replaces_the_unchanged_target() {
    let plan = Case::new(
        listing(&[("x", 2, 5)]),
        listing(&[("x", 1, 9)]),
        vec![row("x", seen(1, 1), seen(1, 9))],
    )
    .plan();
    assert_eq!(plan.steps, [copy("x", true)]);
}

#[test]
fn an_unchanged_file_does_nothing() {
    let plan = Case::new(
        listing(&[("x", 1, 1)]),
        listing(&[("x", 1, 9)]),
        vec![row("x", seen(1, 1), seen(1, 9))],
    )
    .plan();
    assert!(plan.steps.is_empty());
}

#[test]
fn a_change_at_the_target_is_reported_never_overwritten() {
    let plan = Case::new(
        listing(&[("x", 1, 1)]),
        listing(&[("x", 3, 20)]),
        vec![row("x", seen(1, 1), seen(1, 9))],
    )
    .plan();
    assert_eq!(plan.steps, [report("x", Deviation::Changed)]);
    let deleted = Case::new(
        listing(&[("x", 1, 1)]),
        listing(&[]),
        vec![row("x", seen(1, 1), seen(1, 9))],
    )
    .plan();
    assert_eq!(deleted.steps, [report("x", Deviation::Deleted)]);
}

#[test]
fn a_file_only_at_the_target_is_reported_once() {
    let first = Case::new(listing(&[]), listing(&[("mine", 1, 1)]), vec![]).plan();
    assert_eq!(first.steps, [report("mine", Deviation::Added)]);
    let later = Case::new(
        listing(&[]),
        listing(&[("mine", 1, 1)]),
        vec![row("mine", None, seen(1, 1))],
    )
    .plan();
    assert!(later.steps.is_empty(), "known since the last run");
}

#[test]
fn changed_on_both_sides_the_target_version_goes_aside() {
    let plan = Case::new(
        listing(&[("x", 2, 5)]),
        listing(&[("x", 3, 20)]),
        vec![row("x", seen(1, 1), seen(1, 9))],
    )
    .plan();
    assert_eq!(
        plan.steps,
        [
            Step::Aside {
                side: SyncRuleSide::B,
                path: "x".to_owned(),
                to: "x.conflict".to_owned(),
            },
            copy("x", false),
        ]
    );
}

#[test]
fn deleted_at_the_source_follows_the_delete_mode() {
    let case = |mode| {
        Case::new(
            listing(&[]),
            listing(&[("x", 1, 9)]),
            vec![row("x", seen(1, 1), seen(1, 9))],
        )
        .mode(mode)
        .plan()
        .steps
    };
    assert_eq!(
        case(SyncRuleDeleteMode::Keep),
        [Step::Keep {
            path: "x".to_owned()
        }]
    );
    assert_eq!(
        case(SyncRuleDeleteMode::Mirror),
        [Step::Delete {
            side: SyncRuleSide::B,
            path: "x".to_owned()
        }]
    );
    assert_eq!(
        case(SyncRuleDeleteMode::Ask),
        [Step::Pending {
            side: SyncRuleSide::B,
            path: "x".to_owned()
        }]
    );
}

#[test]
fn a_kept_file_is_not_copied_back_or_deleted() {
    let mut kept = row("x", seen(1, 1), seen(1, 9));
    kept.kept = true;
    let plan = Case::new(listing(&[]), listing(&[("x", 1, 9)]), vec![kept])
        .mode(SyncRuleDeleteMode::Mirror)
        .plan();
    assert!(plan.steps.is_empty());
}

#[test]
fn deleted_at_the_source_and_changed_at_the_target_stays() {
    let plan = Case::new(
        listing(&[]),
        listing(&[("x", 5, 30)]),
        vec![row("x", seen(1, 1), seen(1, 9))],
    )
    .mode(SyncRuleDeleteMode::Mirror)
    .plan();
    assert_eq!(plan.steps, [report("x", Deviation::Changed)]);
}

#[test]
fn gone_on_both_sides_the_state_goes() {
    let plan = Case::new(
        listing(&[]),
        listing(&[]),
        vec![row("x", seen(1, 1), seen(1, 9))],
    )
    .plan();
    assert_eq!(
        plan.steps,
        [Step::Forget {
            path: "x".to_owned()
        }]
    );
}

#[test]
fn a_first_meeting_adopts_equal_files_and_asks_for_hashes_before() {
    let case = Case::new(listing(&[("x", 4, 1)]), listing(&[("x", 4, 9)]), vec![]);
    let first = case.plan();
    assert!(first.steps.is_empty());
    assert_eq!(
        first.hash_reads,
        [
            (SyncRuleSide::A, "x".to_owned()),
            (SyncRuleSide::B, "x".to_owned())
        ]
    );
    let equal = case
        .hash(SyncRuleSide::A, "x", "h1")
        .hash(SyncRuleSide::B, "x", "h1")
        .plan();
    assert_eq!(
        equal.steps,
        [Step::Adopt {
            path: "x".to_owned(),
            hash: "h1".to_owned()
        }]
    );
    assert!(equal.hash_reads.is_empty());
}

#[test]
fn a_first_meeting_with_other_content_makes_a_conflict_copy() {
    let differs = Case::new(listing(&[("x", 4, 1)]), listing(&[("x", 4, 9)]), vec![])
        .hash(SyncRuleSide::A, "x", "h1")
        .hash(SyncRuleSide::B, "x", "h2")
        .plan();
    assert_eq!(differs.steps.len(), 2, "aside and copy");
    let other_size = Case::new(listing(&[("x", 4, 1)]), listing(&[("x", 5, 9)]), vec![]).plan();
    assert!(other_size.hash_reads.is_empty(), "the size already differs");
    assert_eq!(other_size.steps[1], copy("x", false));
}

#[test]
fn a_move_with_confirmed_content_moves_at_the_target() {
    let mut known = row("old", seen(7, 3), seen(7, 9));
    known.hash = Some("h7".to_owned());
    let case = Case::new(
        listing(&[("new", 7, 3)]),
        listing(&[("old", 7, 9)]),
        vec![known],
    );
    assert_eq!(
        case.plan().hash_reads,
        [(SyncRuleSide::A, "new".to_owned())]
    );
    let moved = case.hash(SyncRuleSide::A, "new", "h7").plan();
    assert_eq!(
        moved.steps,
        [Step::Move {
            side: SyncRuleSide::B,
            from: "old".to_owned(),
            to: "new".to_owned(),
        }]
    );
}

#[test]
fn without_a_known_hash_a_move_is_a_new_file_and_a_deletion() {
    let case = Case::new(
        listing(&[("new", 7, 3)]),
        listing(&[("old", 7, 9)]),
        vec![row("old", seen(7, 3), seen(7, 9))],
    );
    let plan = case.plan();
    assert!(plan.hash_reads.is_empty());
    assert_eq!(
        plan.steps,
        [
            copy("new", false),
            Step::Keep {
                path: "old".to_owned()
            }
        ]
    );
}

#[test]
fn a_move_whose_content_differs_is_no_move() {
    let mut known = row("old", seen(7, 3), seen(7, 9));
    known.hash = Some("h7".to_owned());
    let plan = Case::new(
        listing(&[("new", 7, 3)]),
        listing(&[("old", 7, 9)]),
        vec![known],
    )
    .hash(SyncRuleSide::A, "new", "other")
    .plan();
    assert_eq!(plan.steps[0], copy("new", false));
}

#[test]
fn excluded_paths_are_counted_and_left_out() {
    let mut case = Case::new(
        listing(&[("a.tmp", 1, 1), ("b.txt", 1, 1)]),
        listing(&[("c.tmp", 1, 1)]),
        vec![],
    );
    case.ignore = Some("*.tmp\n");
    let plan = case.plan();
    assert_eq!(plan.ignored, 2);
    assert_eq!(plan.steps, [copy("b.txt", false)]);
}

#[test]
fn folders_come_first_and_deletions_last() {
    let mut a = listing(&[("d/x", 1, 1)]);
    a.folders.insert("d".to_owned());
    a.folders.insert("empty".to_owned());
    let plan = Case::new(
        a,
        listing(&[("gone", 1, 9)]),
        vec![row("gone", seen(1, 1), seen(1, 9))],
    )
    .mode(SyncRuleDeleteMode::Mirror)
    .plan();
    assert_eq!(
        plan.steps,
        [
            Step::Folder {
                side: SyncRuleSide::B,
                path: "d".to_owned()
            },
            Step::Folder {
                side: SyncRuleSide::B,
                path: "empty".to_owned()
            },
            copy("d/x", false),
            Step::Delete {
                side: SyncRuleSide::B,
                path: "gone".to_owned()
            },
        ]
    );
}

#[test]
fn a_two_way_rule_plans_nothing_yet() {
    let mut case = Case::new(listing(&[("x", 1, 1)]), listing(&[]), vec![]);
    case.rule.direction = SyncRuleDirection::Both;
    assert!(case.plan().steps.is_empty());
}
