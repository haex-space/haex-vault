//! The per-vault record of tool activity (spec 094, T012, FR-019).

use std::fs;

use tempfile::TempDir;

use super::{Record, MAX_BYTES, RECORD_FILE, ROTATED_FILE};

fn read(record: &Record) -> String {
    fs::read_to_string(record.path()).unwrap()
}

#[test]
fn a_line_has_a_timestamp_an_event_and_fixed_text() {
    let dir = TempDir::new().unwrap();
    let record = Record::new(dir.path().join("records"));
    record.append(
        "install_finished",
        &[("vendor", "claude"), ("version", "2.1.311")],
    );
    let text = read(&record);
    let line = text.lines().next().unwrap();
    let mut parts = line.split(' ');
    assert!(parts.next().unwrap().parse::<u64>().unwrap() > 1_700_000_000);
    assert_eq!(parts.next(), Some("install_finished"));
    assert_eq!(parts.next(), Some("vendor=claude"));
    assert_eq!(parts.next(), Some("version=2.1.311"));
    assert_eq!(parts.next(), None);
}

#[test]
fn a_value_cannot_carry_a_path_a_name_or_request_text() {
    let dir = TempDir::new().unwrap();
    let record = Record::new(dir.path());
    record.append(
        "install_failed",
        &[
            ("code", "/home/anna/vaults/Secret Vault/cli-tools"),
            ("note", "tell me a story\nabout my vault"),
            ("empty", ""),
        ],
    );
    let text = read(&record);
    assert_eq!(
        text.lines().count(),
        1,
        "a newline in a value must not add a line"
    );
    // No path separator and no space: a value is one token, and a path or a sentence cannot
    // pass as one. (Callers pass only vendor ids, versions and fixed codes.)
    assert!(!text.contains('/'));
    assert_eq!(text.trim_end().split(' ').count(), 5, "{text:?}");
    assert!(text.contains("empty=-"));
}

#[test]
fn a_long_value_is_cut() {
    let dir = TempDir::new().unwrap();
    let record = Record::new(dir.path());
    record.append("x", &[("v", &"a".repeat(500))]);
    assert!(read(&record).len() < 100);
}

#[test]
fn the_record_is_created_with_its_folder_and_appended_to() {
    let dir = TempDir::new().unwrap();
    let records = dir.path().join("home").join("records");
    let record = Record::new(&records);
    assert!(
        !records.exists(),
        "nothing is created before the first line"
    );
    record.append("one", &[]);
    record.append("two", &[]);
    assert_eq!(read(&record).lines().count(), 2);
    assert_eq!(record.path(), records.join(RECORD_FILE));
}

#[test]
fn the_record_rotates_before_it_passes_the_limit_and_keeps_one_older_generation() {
    let dir = TempDir::new().unwrap();
    let record = Record::new(dir.path());
    let filler = "a".repeat(60);
    let mut lines = 0usize;
    while !dir.path().join(ROTATED_FILE).exists() {
        record.append("fill", &[("data", &filler)]);
        lines += 1;
        assert!(lines < 5_000, "the record never rotated");
    }
    assert!(fs::metadata(record.path()).unwrap().len() < 200);
    assert!(fs::metadata(dir.path().join(ROTATED_FILE)).unwrap().len() <= MAX_BYTES);

    // A second rotation replaces the older generation instead of keeping a third.
    for _ in 0..lines * 2 {
        record.append("fill", &[("data", &filler)]);
    }
    let mut names: Vec<_> = fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(names, [RECORD_FILE, ROTATED_FILE]);
    assert!(fs::metadata(record.path()).unwrap().len() <= MAX_BYTES);
}

#[test]
fn a_record_that_cannot_be_written_never_fails_the_caller() {
    let dir = TempDir::new().unwrap();
    // A file where the records folder should be: creating the folder fails.
    let blocker = dir.path().join("records");
    fs::write(&blocker, "not a folder").unwrap();
    Record::new(&blocker).append("lost", &[("a", "b")]);
    assert_eq!(fs::read_to_string(&blocker).unwrap(), "not a folder");
}
