//! The plan of a one-way rule A → B (spec 061 Story 1, FR-009, FR-010, FR-012a, FR-013, FR-019 to
//! FR-021) and of a two-way rule (Story 2, FR-011 to FR-013, FR-039): one test per row of the
//! table, plus moves, first meetings and the order of the steps.

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

    fn both(mut self) -> Self {
        self.rule.direction = SyncRuleDirection::Both;
        self.rule.delete_mode = SyncRuleDeleteMode::Mirror;
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
            conflict_name: &|path, taken| match taken.len() {
                0 => format!("{path}.conflict"),
                n => format!("{path}.conflict{}", n + 1),
            },
        })
    }
}

fn copy(path: &str, replaces: bool) -> Step {
    copy_from(SyncRuleSide::A, path, replaces)
}

fn copy_from(from: SyncRuleSide, path: &str, replaces: bool) -> Step {
    Step::Copy {
        from,
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
fn two_way_new_files_go_to_the_other_side() {
    let plan = Case::new(listing(&[("a", 1, 1)]), listing(&[("b", 1, 1)]), vec![])
        .both()
        .plan();
    assert_eq!(
        plan.steps,
        [
            copy_from(SyncRuleSide::A, "a", false),
            copy_from(SyncRuleSide::B, "b", false)
        ]
    );
}

#[test]
fn two_way_the_changed_side_wins() {
    let state = vec![
        row("x", seen(1, 1), seen(1, 9)),
        row("y", seen(1, 1), seen(1, 9)),
    ];
    let plan = Case::new(
        listing(&[("x", 2, 2), ("y", 1, 1)]),
        listing(&[("x", 1, 9), ("y", 3, 10)]),
        state,
    )
    .both()
    .plan();
    assert_eq!(
        plan.steps,
        [
            copy_from(SyncRuleSide::A, "x", true),
            copy_from(SyncRuleSide::B, "y", true)
        ]
    );
}

#[test]
fn two_way_both_changed_the_newer_keeps_the_name() {
    let state = vec![row("x", seen(1, 1), seen(1, 9))];
    // The times of both sides are compared only to choose the newer one (FR-012).
    let b_newer = Case::new(
        listing(&[("x", 2, 50)]),
        listing(&[("x", 3, 60)]),
        state.clone(),
    )
    .both()
    .plan();
    assert_eq!(
        b_newer.steps,
        [Step::Conflict {
            path: "x".to_owned(),
            keep: SyncRuleSide::B,
            aside: "x.conflict".to_owned(),
        }]
    );
    let a_newer = Case::new(listing(&[("x", 2, 70)]), listing(&[("x", 3, 60)]), state)
        .both()
        .plan();
    assert!(matches!(
        a_newer.steps[0],
        Step::Conflict {
            keep: SyncRuleSide::A,
            ..
        }
    ));
}

#[test]
fn two_way_deleted_against_changed_copies_the_change_back() {
    let state = vec![row("x", seen(1, 1), seen(1, 9))];
    let plan = Case::new(listing(&[]), listing(&[("x", 2, 10)]), state.clone())
        .both()
        .plan();
    assert_eq!(plan.steps, [copy_from(SyncRuleSide::B, "x", false)]);
    let other_way = Case::new(listing(&[("x", 2, 2)]), listing(&[]), state)
        .both()
        .plan();
    assert_eq!(other_way.steps, [copy_from(SyncRuleSide::A, "x", false)]);
}

#[test]
fn two_way_a_deletion_follows_the_delete_mode_on_the_other_side() {
    let state = vec![row("x", seen(1, 1), seen(1, 9))];
    let deleted_on_b = Case::new(listing(&[("x", 1, 1)]), listing(&[]), state.clone()).both();
    assert_eq!(
        deleted_on_b.plan().steps,
        [Step::Delete {
            side: SyncRuleSide::A,
            path: "x".to_owned()
        }]
    );
    assert_eq!(
        deleted_on_b.mode(SyncRuleDeleteMode::Ask).plan().steps,
        [Step::Pending {
            side: SyncRuleSide::A,
            path: "x".to_owned()
        }]
    );
    let kept = Case::new(listing(&[]), listing(&[("x", 1, 9)]), state)
        .both()
        .mode(SyncRuleDeleteMode::Keep);
    assert_eq!(
        kept.plan().steps,
        [Step::Keep {
            path: "x".to_owned()
        }]
    );
}

#[test]
fn two_way_a_kept_file_is_not_copied_back() {
    let mut known = row("x", seen(1, 1), seen(1, 9));
    known.kept = true;
    let plan = Case::new(listing(&[]), listing(&[("x", 2, 10)]), vec![known])
        .both()
        .plan();
    assert!(plan.steps.is_empty(), "{plan:?}");
}

#[test]
fn two_way_gone_on_both_sides_the_state_goes() {
    let plan = Case::new(
        listing(&[]),
        listing(&[]),
        vec![row("x", seen(1, 1), seen(1, 9))],
    )
    .both()
    .plan();
    assert_eq!(
        plan.steps,
        [Step::Forget {
            path: "x".to_owned()
        }]
    );
}

#[test]
fn two_way_a_move_on_either_side_moves_on_the_other() {
    let mut known = row("old", seen(7, 3), seen(7, 9));
    known.hash = Some("h7".to_owned());
    let moved_on_b = Case::new(
        listing(&[("old", 7, 3)]),
        listing(&[("sub/new", 7, 9)]),
        vec![known.clone()],
    )
    .both();
    assert_eq!(
        moved_on_b.plan().hash_reads,
        [(SyncRuleSide::B, "sub/new".to_owned())]
    );
    assert_eq!(
        moved_on_b
            .hash(SyncRuleSide::B, "sub/new", "h7")
            .plan()
            .steps,
        [Step::Move {
            side: SyncRuleSide::A,
            from: "old".to_owned(),
            to: "sub/new".to_owned(),
        }]
    );
    let moved_on_a = Case::new(
        listing(&[("new", 7, 3)]),
        listing(&[("old", 7, 9)]),
        vec![known],
    )
    .both()
    .hash(SyncRuleSide::A, "new", "h7")
    .plan();
    assert_eq!(
        moved_on_a.steps,
        [Step::Move {
            side: SyncRuleSide::B,
            from: "old".to_owned(),
            to: "new".to_owned(),
        }]
    );
}

#[test]
fn two_way_two_move_candidates_are_told_apart_by_the_hash() {
    let mut known = row("old", seen(7, 3), seen(7, 9));
    known.hash = Some("h7".to_owned());
    let case = Case::new(
        listing(&[("one", 7, 3), ("two", 7, 3)]),
        listing(&[("old", 7, 9)]),
        vec![known],
    )
    .both();
    assert_eq!(
        case.plan().hash_reads,
        [
            (SyncRuleSide::A, "one".to_owned()),
            (SyncRuleSide::A, "two".to_owned())
        ]
    );
    let plan = case
        .hash(SyncRuleSide::A, "one", "other")
        .hash(SyncRuleSide::A, "two", "h7")
        .plan();
    assert_eq!(
        plan.steps,
        [
            Step::Move {
                side: SyncRuleSide::B,
                from: "old".to_owned(),
                to: "two".to_owned(),
            },
            copy_from(SyncRuleSide::A, "one", false),
        ]
    );
}

#[test]
fn two_way_first_meeting_adopts_equal_and_conflicts_on_other_content() {
    let case = Case::new(
        listing(&[("same", 4, 1), ("other", 4, 1), ("size", 4, 1)]),
        listing(&[("same", 4, 9), ("other", 4, 9), ("size", 5, 9)]),
        vec![],
    )
    .both();
    let first = case.plan();
    assert_eq!(first.hash_reads.len(), 4, "same and other, both sides");
    let plan = case
        .hash(SyncRuleSide::A, "same", "h")
        .hash(SyncRuleSide::B, "same", "h")
        .hash(SyncRuleSide::A, "other", "h1")
        .hash(SyncRuleSide::B, "other", "h2")
        .plan();
    assert_eq!(
        plan.steps,
        [
            Step::Conflict {
                path: "other".to_owned(),
                keep: SyncRuleSide::B,
                aside: "other.conflict".to_owned(),
            },
            Step::Conflict {
                path: "size".to_owned(),
                keep: SyncRuleSide::B,
                aside: "size.conflict".to_owned(),
            },
            Step::Adopt {
                path: "same".to_owned(),
                hash: "h".to_owned()
            },
        ]
    );
}

#[test]
fn two_way_differing_exclusion_files_merge() {
    let plan = Case::new(
        listing(&[(IGNORE_FILE, 4, 1)]),
        listing(&[(IGNORE_FILE, 6, 9)]),
        vec![],
    )
    .both()
    .plan();
    assert_eq!(
        plan.steps,
        [Step::MergeIgnore {
            aside_a: ".haex-ignore.conflict".to_owned(),
            aside_b: ".haex-ignore.conflict2".to_owned(),
        }]
    );
}

#[test]
fn two_way_copies_create_their_folders_but_empty_folders_stay() {
    let mut b = listing(&[("d/e/x", 1, 1)]);
    b.folders
        .extend(["d".to_owned(), "d/e".to_owned(), "empty".to_owned()]);
    let plan = Case::new(listing(&[]), b, vec![]).both().plan();
    assert_eq!(
        plan.steps,
        [
            Step::Folder {
                side: SyncRuleSide::A,
                path: "d/e".to_owned()
            },
            copy_from(SyncRuleSide::B, "d/e/x", false),
        ]
    );
}
