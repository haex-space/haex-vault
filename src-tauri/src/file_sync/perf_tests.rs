//! SC-007 (T068): a run over 10 000 unchanged files of this device ends in under five seconds.
//! Never in CI; run it with
//!
//! ```text
//! cargo test --lib -- --ignored file_sync::run::tests::perf --nocapture
//! ```
//!
//! It prints the time; research.md records it.

use std::time::{Duration, Instant};

use super::*;

const FILES: u64 = 10_000;
const LIMIT: Duration = Duration::from_secs(5);

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "measurement, see the module doc"]
async fn a_run_over_ten_thousand_unchanged_files_ends_in_five_seconds() {
    let w = world(SyncRuleDeleteMode::Keep);
    // 100 folders of 100 files, as a photo folder is split by month or event.
    for i in 0..FILES {
        let folder = w.a.join(format!("d{:03}", i / 100));
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join(format!("f{i:05}.txt")), i.to_string()).unwrap();
    }
    let first = w.run().await;
    assert_eq!(first.outcome, SyncRuleOutcome::Done, "{:?}", first.error);
    assert_eq!(first.counts.copied, FILES);

    let started = Instant::now();
    let second = w.run().await;
    let took = started.elapsed();
    println!("sync rule run over {FILES} unchanged files: {took:?}");
    assert_eq!(second.outcome, SyncRuleOutcome::Done, "{:?}", second.error);
    assert_eq!(second.counts.copied, 0);
    assert!(
        second.items.is_empty(),
        "{} file actions",
        second.items.len()
    );
    assert!(took < LIMIT, "took {took:?}, SC-007 allows {LIMIT:?}");
}
