//! Scanning both places: listings and exclusions, an unreachable place waits, an exclusion file over
//! its limits fails the run closed (FR-022, FR-038).

use super::*;
use crate::file_sync::test_support::MemPlace;

#[tokio::test]
async fn both_listings_and_both_exclusion_files_are_read() {
    let (a, b) = (MemPlace::new(), MemPlace::new());
    a.put("x.tmp", b"x", 1);
    a.put(IGNORE_FILE, b"*.tmp\n", 1);
    b.put("y.log", b"y", 1);
    b.put(IGNORE_FILE, b"*.log\n", 1);
    let scanned = scan(&a, &b, &CancellationToken::new()).await.unwrap();
    assert!(scanned.a.files.contains_key("x.tmp"));
    assert!(scanned.exclusions.excludes("x.tmp", false));
    assert!(scanned.exclusions.excludes("y.log", false));
}

#[tokio::test]
async fn an_unreachable_place_waits() {
    let (a, b) = (MemPlace::new(), MemPlace::new());
    b.set_unreachable(true);
    let error = scan(&a, &b, &CancellationToken::new())
        .await
        .err()
        .expect("refused");
    assert!(matches!(
        error,
        ScanError::Unreachable {
            side: SyncRuleSide::B,
            ..
        }
    ));
}

#[tokio::test]
async fn an_exclusion_file_over_its_limits_fails_the_run() {
    let (a, b) = (MemPlace::new(), MemPlace::new());
    a.put(IGNORE_FILE, &vec![b'x'; MAX_BYTES + 1], 1);
    let error = scan(&a, &b, &CancellationToken::new())
        .await
        .err()
        .expect("refused");
    assert!(matches!(
        error,
        ScanError::Failed {
            side: SyncRuleSide::A,
            ..
        }
    ));
}
