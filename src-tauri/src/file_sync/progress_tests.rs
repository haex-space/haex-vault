//! The reports of a run: the phase, the files and bytes counted up, a file's bytes thinned out, and
//! a reporter nobody hears.

use std::sync::{Arc, Mutex};

use super::*;

fn collecting() -> (Reporter, Arc<Mutex<Vec<SyncRuleProgress>>>) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&seen);
    let reporter = Reporter::new("r1", move |progress| {
        sink.lock().unwrap().push(progress.clone());
    });
    (reporter, seen)
}

#[test]
fn a_run_goes_from_scanning_to_copying_and_counts_up() {
    let (reporter, seen) = collecting();
    reporter.scanning();
    reporter.planned(2, 300);
    reporter.file_done(100);
    reporter.file_done(200);
    let seen = seen.lock().unwrap();
    assert_eq!(seen[0].phase, SyncRulePhase::Scanning);
    assert_eq!(seen[1].phase, SyncRulePhase::Copying);
    assert_eq!((seen[1].files_done, seen[1].files_total), (0, 2));
    assert_eq!((seen[2].files_done, seen[2].bytes_done), (1, 100));
    let last = seen.last().unwrap();
    assert_eq!(
        (last.files_done, last.bytes_done, last.bytes_total),
        (2, 300, 300)
    );
    assert!(seen.iter().all(|progress| progress.rule_id == "r1"));
}

#[test]
fn bytes_of_a_file_are_added_to_the_finished_ones_and_thinned_out() {
    let (reporter, seen) = collecting();
    reporter.planned(2, 300);
    reporter.file_done(100);
    reporter.bytes(50);
    // Within the pause after the first report: counted, not reported.
    reporter.bytes(120);
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 3);
    assert_eq!(seen[2].bytes_done, 150);
}

#[test]
fn bytes_never_exceed_the_total() {
    let (reporter, seen) = collecting();
    reporter.planned(1, 10);
    reporter.bytes(99);
    assert_eq!(seen.lock().unwrap().last().unwrap().bytes_done, 10);
}

#[test]
fn a_failed_copy_discards_its_partial_bytes() {
    let (reporter, seen) = collecting();
    reporter.planned(2, 300);
    reporter.file_done(100);
    reporter.bytes(50);
    reporter.copy_failed();
    let last = seen.lock().unwrap().last().unwrap().clone();
    assert_eq!((last.files_done, last.bytes_done), (1, 100));
}

#[test]
fn a_silent_reporter_takes_reports() {
    let reporter = Reporter::silent();
    reporter.scanning();
    reporter.planned(1, 1);
    reporter.bytes(1);
    reporter.file_done(1);
}
