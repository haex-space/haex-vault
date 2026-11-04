//! Scanning both places: listings and exclusions, an unreachable place waits, an exclusion file over
//! its limits fails the run closed (FR-022, FR-038).

use super::*;
use crate::file_sync::test_support::MemPlace;

#[tokio::test]
async fn both_listings_and_both_exclusion_files_are_read() {
    let (source, target) = (MemPlace::new(), MemPlace::new());
    source.put("x.tmp", b"x", 1);
    source.put(IGNORE_FILE, b"*.tmp\n", 1);
    target.put("y.log", b"y", 1);
    target.put(IGNORE_FILE, b"*.log\n", 1);
    let scanned = scan(&source, &target, &CancellationToken::new())
        .await
        .unwrap();
    assert!(scanned.source.files.contains_key("x.tmp"));
    assert!(scanned.exclusions.excludes("x.tmp", false));
    assert!(scanned.exclusions.excludes("y.log", false));
}

#[tokio::test]
async fn an_unreachable_place_waits() {
    let (source, target) = (MemPlace::new(), MemPlace::new());
    target.set_unreachable(true);
    let error = scan(&source, &target, &CancellationToken::new())
        .await
        .err()
        .expect("refused");
    assert!(matches!(
        error,
        ScanError::Unreachable {
            side: SyncRuleSide::Target,
            ..
        }
    ));
}

#[tokio::test]
async fn an_exclusion_file_over_its_limits_fails_the_run() {
    let (source, target) = (MemPlace::new(), MemPlace::new());
    source.put(IGNORE_FILE, &vec![b'x'; MAX_BYTES + 1], 1);
    let error = scan(&source, &target, &CancellationToken::new())
        .await
        .err()
        .expect("refused");
    assert!(matches!(
        error,
        ScanError::Failed {
            side: SyncRuleSide::Source,
            ..
        }
    ));
}
