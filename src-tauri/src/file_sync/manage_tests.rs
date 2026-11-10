//! The preview changes nothing and names hash reads and excluded files (FR-007, FR-012a, FR-042);
//! the exclusion editor shows the file of the source, the fixed exclusions, the excluded count and bad
//! lines, and writes only files within the limits (FR-041).

use super::*;
use crate::file_sync::model::{
    SyncRuleDeleteMode, SyncRuleDirection, SyncRuleOutcome, SyncRulePlace,
};
use crate::file_sync::test_support::MemPlace;
use crate::files::SourceRef;

fn rule(direction: SyncRuleDirection) -> SyncRule {
    let place = |path: &str| SyncRulePlace {
        location: SourceRef::Device,
        path: path.to_owned(),
    };
    SyncRule {
        id: "r".to_owned(),
        name: "r".to_owned(),
        source: place("/a"),
        target: place("/b"),
        source_kind: crate::file_sync::model::SyncRuleSourceKind::Folder,
        direction,
        delete_mode: SyncRuleDeleteMode::Keep,
        interval_secs: 900,
        enabled: true,
        allow_decrypted: false,
        created_at: String::new(),
        updated_at: String::new(),
        last_run_at: None,
        last_outcome: None::<SyncRuleOutcome>,
        last_error: None,
    }
}

fn writes(place: &MemPlace) -> usize {
    ["create_folder", "rename", "delete", "trash", "write_text"]
        .iter()
        .map(|name| place.calls(name))
        .sum()
}

#[tokio::test]
async fn the_preview_shows_the_steps_and_changes_nothing() {
    let (source, target) = (MemPlace::new(), MemPlace::new());
    source.put("neu.txt", b"neu", 1);
    source.put("x.tmp", b"t", 1);
    source.put(".haex-ignore", b"*.tmp\n", 1);
    // Same size on both sides, never met: the run would compare their content.
    source.put("beide.txt", b"aaaa", 1);
    target.put("beide.txt", b"bbbb", 2);
    let plan = preview(
        &rule(SyncRuleDirection::Both),
        &source,
        &target,
        &[],
        "Laptop",
        "2026-10-29",
    )
    .await
    .unwrap();
    let copies: Vec<&str> = plan
        .steps
        .iter()
        .filter(|step| step.action == SyncRulePlanAction::Copy)
        .map(|step| step.path.as_str())
        .collect();
    assert_eq!(copies, [".haex-ignore", "neu.txt"]);
    assert_eq!(plan.hash_reads, 2, "both sides of beide.txt");
    assert_eq!(plan.ignored, 1);
    assert_eq!(
        writes(&source) + writes(&target),
        0,
        "nothing changed (FR-007)"
    );
}

#[tokio::test]
async fn the_preview_names_conflict_copies_and_deviations() {
    let (source, target) = (MemPlace::new(), MemPlace::new());
    source.put("x.txt", b"lang", 1);
    target.put("x.txt", b"kurz!!", 2);
    target.put("nur-b.txt", b"b", 2);
    let plan = preview(
        &rule(SyncRuleDirection::SourceToTarget),
        &source,
        &target,
        &[],
        "Laptop",
        "2026-10-29",
    )
    .await
    .unwrap();
    let conflict = plan
        .steps
        .iter()
        .find(|step| step.action == SyncRulePlanAction::ConflictCopy)
        .unwrap();
    assert_eq!(conflict.side, Some(SyncRuleSide::Target));
    assert_eq!(
        conflict.detail.as_deref(),
        Some("x (Konflikt Laptop 2026-10-29).txt")
    );
    assert!(plan
        .steps
        .iter()
        .any(|step| step.action == SyncRulePlanAction::Deviation
            && step.path == "nur-b.txt"
            && step.detail.as_deref() == Some("added")));
}

#[tokio::test]
async fn an_unreachable_place_has_no_preview() {
    let (source, target) = (MemPlace::new(), MemPlace::new());
    target.set_unreachable(true);
    let error = preview(
        &rule(SyncRuleDirection::SourceToTarget),
        &source,
        &target,
        &[],
        "Laptop",
        "2026-10-29",
    )
    .await
    .unwrap_err();
    assert!(matches!(error, ScanError::Unreachable { .. }));
}

#[tokio::test]
async fn the_editor_shows_the_file_of_a_the_fixed_exclusions_and_what_they_leave_out() {
    let (source, target) = (MemPlace::new(), MemPlace::new());
    source.put(".haex-ignore", b"*.tmp\n[z-a\n", 1);
    source.put("x.tmp", b"t", 1);
    target.put(".haex-ignore", b"*.bak\n", 1);
    target.put("y.bak", b"b", 1);
    target.put("z.txt", b"z", 1);
    let shown = ignore_get(&source, &target, None).await.unwrap();
    assert_eq!(shown.content, "*.tmp\n[z-a\n");
    assert!(shown.writable);
    assert_eq!(shown.fixed.len(), FIXED.len());
    assert_eq!(shown.ignored_now, 2, "the union of both sides");
    assert_eq!(shown.line_errors.len(), 1);
    assert_eq!(shown.line_errors[0].line, 2);
    let read_only = ignore_get(&MemPlace::new(), &target, Some("read only".to_owned()))
        .await
        .unwrap();
    assert_eq!(read_only.content, "");
    assert!(!read_only.writable);
    assert_eq!(read_only.reason.as_deref(), Some("read only"));
}

#[tokio::test]
async fn the_editor_writes_the_source_within_the_limits() {
    let source = MemPlace::new();
    ignore_set(&source, "*.tmp\n").await.unwrap();
    assert_eq!(
        source.content(".haex-ignore").as_deref(),
        Some(&b"*.tmp\n"[..])
    );
    let too_long = "x\n".repeat(ignore::MAX_LINES + 1);
    let error = ignore_set(&source, &too_long).await.unwrap_err();
    assert_eq!(error.code, FilesErrorCode::TooLarge);
    assert_eq!(
        source.content(".haex-ignore").as_deref(),
        Some(&b"*.tmp\n"[..])
    );
}

// ---- file rules (spec 074) ----

use crate::file_sync::model::SyncRuleSourceKind;
use crate::file_sync::place_file::OneFilePlace;

fn file_rule() -> SyncRule {
    let mut rule = rule(SyncRuleDirection::SourceToTarget);
    rule.source_kind = SyncRuleSourceKind::File;
    rule.source.path = "/a/f.txt".to_owned();
    rule
}

#[tokio::test]
async fn the_preview_of_a_file_rule_shows_its_one_step() {
    let (source, target) = (MemPlace::new(), MemPlace::new());
    source.put("f.txt", b"abc", 1);
    source.put("andere.txt", b"x", 1);
    let source = OneFilePlace::new(Box::new(source), "f.txt");
    let target = OneFilePlace::new(Box::new(target), "f.txt");
    let plan = preview(&file_rule(), &source, &target, &[], "Laptop", "2026-11-09")
        .await
        .unwrap();
    assert_eq!(plan.steps.len(), 1, "{:?}", plan.steps);
    assert_eq!(plan.steps[0].action, SyncRulePlanAction::Copy);
    assert_eq!(plan.steps[0].path, "f.txt");
    assert_eq!(plan.ignored, 0, "no exclusions for a file rule");
}

#[tokio::test]
async fn the_preview_of_a_file_rule_without_its_file_is_an_invalid_source_and_never_a_deletion() {
    let target = MemPlace::new();
    target.put("f.txt", b"abc", 1);
    let source = OneFilePlace::new(Box::new(MemPlace::new()), "f.txt");
    let target = OneFilePlace::new(Box::new(target), "f.txt");
    let error = preview(&file_rule(), &source, &target, &[], "Laptop", "2026-11-09")
        .await
        .unwrap_err();
    assert_eq!(
        error,
        ScanError::InvalidSource {
            reason: "noFile".to_owned()
        }
    );
}
