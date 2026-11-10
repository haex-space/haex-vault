//! Only folders of this device are watched.

use super::*;

#[test]
fn a_storage_place_is_not_watched() {
    let place = SyncRulePlace {
        location: SourceRef::Storage {
            storage_id: "s3".to_owned(),
        },
        path: "/x".to_owned(),
    };
    assert!(watch_place(&place, Arc::new(|| {})).is_none());
}

#[test]
fn a_device_folder_is_watched() {
    let dir = tempfile::tempdir().unwrap();
    let place = SyncRulePlace {
        location: SourceRef::Device,
        path: dir.path().to_string_lossy().into_owned(),
    };
    assert!(watch_place(&place, Arc::new(|| {})).is_some());
}

// ---- one file (spec 074 research R7) ----

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// Long enough for the debounce and a slow CI machine.
const WAIT: Duration = Duration::from_secs(10);

fn counted_watch(
    folder: &std::path::Path,
    name: &str,
) -> (Arc<AtomicUsize>, Box<dyn std::any::Any + Send>) {
    let count = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&count);
    let watch = watch_file(
        &SourceRef::Device,
        &folder.to_string_lossy(),
        name,
        Arc::new(move || {
            seen.fetch_add(1, Ordering::SeqCst);
        }),
    )
    .expect("a file of this device is watched");
    (count, Box::new(watch))
}

fn wait_for(count: &AtomicUsize, at_least: usize) -> bool {
    let started = Instant::now();
    while started.elapsed() < WAIT {
        if count.load(Ordering::SeqCst) >= at_least {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

#[test]
fn a_file_of_a_storage_is_not_watched() {
    let storage = SourceRef::Storage {
        storage_id: "s3".to_owned(),
    };
    assert!(watch_file(&storage, "/x", "f.txt", Arc::new(|| {})).is_none());
}

#[test]
fn a_write_to_the_file_wakes_the_rule() {
    let dir = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(dir.path()).unwrap();
    std::fs::write(root.join("Haushalt.ods"), "eins").unwrap();
    let (count, _watch) = counted_watch(&root, "Haushalt.ods");
    std::fs::write(root.join("Haushalt.ods"), "zwei").unwrap();
    assert!(wait_for(&count, 1), "the change of the file was not heard");
}

#[test]
fn replacing_the_file_by_renaming_a_new_one_over_it_wakes_the_rule() {
    let dir = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(dir.path()).unwrap();
    std::fs::write(root.join("Haushalt.ods"), "eins").unwrap();
    let (count, _watch) = counted_watch(&root, "Haushalt.ods");
    std::fs::write(root.join("Haushalt.ods.tmp"), "neu").unwrap();
    std::fs::rename(root.join("Haushalt.ods.tmp"), root.join("Haushalt.ods")).unwrap();
    assert!(wait_for(&count, 1), "the replacement was not heard");
}

#[test]
fn a_write_to_a_sibling_does_not_wake_the_rule() {
    let dir = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(dir.path()).unwrap();
    std::fs::write(root.join("Haushalt.ods"), "eins").unwrap();
    let (count, _watch) = counted_watch(&root, "Haushalt.ods");
    std::fs::write(root.join("anderes.txt"), "x").unwrap();
    std::fs::write(root.join("anderes.txt"), "xy").unwrap();
    // Past the debounce, a change of the file itself would have been heard by now.
    std::thread::sleep(Duration::from_secs(3));
    assert_eq!(count.load(Ordering::SeqCst), 0, "a sibling woke the rule");
}
