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
        source: SourceRef::Device,
        path: path.to_owned(),
    };
    SyncRule {
        id: "r".to_owned(),
        name: "r".to_owned(),
        place_a: place("/a"),
        place_b: place("/b"),
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
    let (a, b) = (MemPlace::new(), MemPlace::new());
    a.put("neu.txt", b"neu", 1);
    a.put("x.tmp", b"t", 1);
    a.put(".haex-ignore", b"*.tmp\n", 1);
    // Same size on both sides, never met: the run would compare their content.
    a.put("beide.txt", b"aaaa", 1);
    b.put("beide.txt", b"bbbb", 2);
    let plan = preview(
        &rule(SyncRuleDirection::Both),
        &a,
        &b,
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
    assert_eq!(writes(&a) + writes(&b), 0, "nothing changed (FR-007)");
}

#[tokio::test]
async fn the_preview_names_conflict_copies_and_deviations() {
    let (a, b) = (MemPlace::new(), MemPlace::new());
    a.put("x.txt", b"lang", 1);
    b.put("x.txt", b"kurz!!", 2);
    b.put("nur-b.txt", b"b", 2);
    let plan = preview(
        &rule(SyncRuleDirection::AToB),
        &a,
        &b,
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
    assert_eq!(conflict.side, Some(SyncRuleSide::B));
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
    let (a, b) = (MemPlace::new(), MemPlace::new());
    b.set_unreachable(true);
    let error = preview(
        &rule(SyncRuleDirection::AToB),
        &a,
        &b,
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
    let (a, b) = (MemPlace::new(), MemPlace::new());
    a.put(".haex-ignore", b"*.tmp\n[z-a\n", 1);
    a.put("x.tmp", b"t", 1);
    b.put(".haex-ignore", b"*.bak\n", 1);
    b.put("y.bak", b"b", 1);
    b.put("z.txt", b"z", 1);
    let shown = ignore_get(&a, &b, None).await.unwrap();
    assert_eq!(shown.content, "*.tmp\n[z-a\n");
    assert!(shown.writable);
    assert_eq!(shown.fixed.len(), FIXED.len());
    assert_eq!(shown.ignored_now, 2, "the union of both sides");
    assert_eq!(shown.line_errors.len(), 1);
    assert_eq!(shown.line_errors[0].line, 2);
    let read_only = ignore_get(&MemPlace::new(), &b, Some("read only".to_owned()))
        .await
        .unwrap();
    assert_eq!(read_only.content, "");
    assert!(!read_only.writable);
    assert_eq!(read_only.reason.as_deref(), Some("read only"));
}

#[tokio::test]
async fn the_editor_writes_place_a_within_the_limits() {
    let a = MemPlace::new();
    ignore_set(&a, "*.tmp\n").await.unwrap();
    assert_eq!(a.content(".haex-ignore").as_deref(), Some(&b"*.tmp\n"[..]));
    let too_long = "x\n".repeat(ignore::MAX_LINES + 1);
    let error = ignore_set(&a, &too_long).await.unwrap_err();
    assert_eq!(error.code, FilesErrorCode::TooLarge);
    assert_eq!(a.content(".haex-ignore").as_deref(), Some(&b"*.tmp\n"[..]));
}
