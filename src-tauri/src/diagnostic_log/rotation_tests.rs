use std::fs;
use std::path::Path;

use super::*;

const SMALL: Limits = Limits {
    files: 3,
    bytes: 200,
};

fn read(folder: &Path, name: &str) -> String {
    fs::read_to_string(folder.join(name)).unwrap_or_default()
}

fn names(folder: &Path) -> Vec<String> {
    let mut found: Vec<String> = fs::read_dir(folder)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    found.sort();
    found
}

/// A record of exactly `len` bytes including its line break, naming its number.
fn record(number: usize, len: usize) -> String {
    let head = format!("[r{number:03}]");
    format!("{head}{}", "x".repeat(len - 1 - head.len()))
}

#[test]
fn a_file_past_the_limit_rotates_and_a_new_one_starts() {
    let dir = tempfile::tempdir().unwrap();
    let mut log = RotatingFile::open(dir.path(), SMALL).unwrap();
    log.write_line(&record(1, 100)).unwrap();
    log.write_line(&record(2, 100)).unwrap();
    assert_eq!(
        names(dir.path()),
        ["haex.log"],
        "exactly at the limit stays"
    );

    log.write_line(&record(3, 100)).unwrap();

    assert_eq!(names(dir.path()), ["haex.log", "haex.log.1"]);
    assert!(read(dir.path(), "haex.log.1").starts_with("[r001]"));
    assert!(read(dir.path(), "haex.log.1").contains("[r002]"));
    assert_eq!(read(dir.path(), "haex.log").lines().count(), 1);
    assert!(read(dir.path(), "haex.log").starts_with("[r003]"));
}

#[test]
fn with_n_files_kept_the_oldest_is_deleted() {
    let dir = tempfile::tempdir().unwrap();
    let mut log = RotatingFile::open(dir.path(), SMALL).unwrap();
    for number in 1..=10 {
        log.write_line(&record(number, 150)).unwrap();
    }

    assert_eq!(names(dir.path()), ["haex.log", "haex.log.1", "haex.log.2"]);
    let all = format!(
        "{}{}{}",
        read(dir.path(), "haex.log.2"),
        read(dir.path(), "haex.log.1"),
        read(dir.path(), "haex.log")
    );
    let kept: Vec<&str> = all.lines().map(|line| &line[..6]).collect();
    assert_eq!(kept, ["[r008]", "[r009]", "[r010]"], "the newest, in order");
}

#[test]
fn a_rotation_never_splits_a_record() {
    let dir = tempfile::tempdir().unwrap();
    let mut log = RotatingFile::open(dir.path(), SMALL).unwrap();
    let stack = format!("{}\n    at one\n    at two", record(1, 120));
    log.write_line(&stack).unwrap();
    log.write_line(&record(2, 120)).unwrap();

    assert_eq!(read(dir.path(), "haex.log.1"), format!("{stack}\n"));
    assert_eq!(
        read(dir.path(), "haex.log"),
        format!("{}\n", record(2, 120))
    );
}

#[test]
fn a_record_above_the_size_is_written_whole() {
    let dir = tempfile::tempdir().unwrap();
    let mut log = RotatingFile::open(dir.path(), SMALL).unwrap();
    let long = "y".repeat(500);
    log.write_line(&long).unwrap();
    assert_eq!(read(dir.path(), "haex.log"), format!("{long}\n"));
}

#[test]
fn opening_appends_to_the_file_of_an_earlier_run() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("haex.log"), "old\n").unwrap();
    let mut log = RotatingFile::open(dir.path(), SMALL).unwrap();
    log.write_line("new").unwrap();
    assert_eq!(read(dir.path(), "haex.log"), "old\nnew\n");
}

#[test]
fn a_write_error_is_a_value_and_the_next_write_tries_again() {
    let dir = tempfile::tempdir().unwrap();
    let mut log = RotatingFile::open(dir.path(), SMALL).unwrap();
    log.write_line(&record(1, 150)).unwrap();
    // A folder where the oldest archive belongs makes the next rotation fail.
    fs::create_dir(dir.path().join("haex.log.2")).unwrap();

    assert!(log.write_line(&record(2, 150)).is_err());

    fs::remove_dir(dir.path().join("haex.log.2")).unwrap();
    log.write_line(&record(3, 150)).unwrap();
    assert!(read(dir.path(), "haex.log").starts_with("[r003]"));
    assert!(read(dir.path(), "haex.log.1").starts_with("[r001]"));
}

#[test]
fn switching_to_smaller_limits_keeps_only_the_newest_whole_records() {
    let dir = tempfile::tempdir().unwrap();
    let big = Limits {
        files: 5,
        bytes: 1000,
    };
    let mut log = RotatingFile::open(dir.path(), big).unwrap();
    for number in 1..=40 {
        log.write_line(&record(number, 100)).unwrap();
    }
    assert!(dir.path().join("haex.log.3").exists());

    log.set_limits(SMALL).unwrap();

    let mut files = names(dir.path());
    files.sort();
    assert_eq!(files, ["haex.log", "haex.log.1", "haex.log.2"]);
    let mut total = 0;
    let mut all = String::new();
    for name in ["haex.log.2", "haex.log.1", "haex.log"] {
        let text = read(dir.path(), name);
        assert!(text.len() as u64 <= SMALL.bytes, "{name} is {}", text.len());
        total += text.len() as u64;
        all.push_str(&text);
    }
    assert!(total <= SMALL.bytes * SMALL.files as u64);
    let numbers: Vec<&str> = all.lines().map(|line| &line[..6]).collect();
    assert_eq!(numbers.last(), Some(&"[r040]"), "the newest record stays");
    assert_eq!(
        numbers.len(),
        6,
        "two records of 100 bytes fit one file of 200"
    );
    assert!(all.lines().all(|line| line.len() == 99));
}

#[test]
fn a_failed_compaction_is_retried_before_the_next_append() {
    let dir = tempfile::tempdir().unwrap();
    let big = Limits {
        files: 5,
        bytes: 1000,
    };
    let mut log = RotatingFile::open(dir.path(), big).unwrap();
    for number in 1..=40 {
        log.write_line(&record(number, 100)).unwrap();
    }
    log.file = None;
    fs::remove_file(dir.path().join("haex.log")).unwrap();
    fs::create_dir(dir.path().join("haex.log")).unwrap();

    assert!(log.set_limits(SMALL).is_err());
    fs::remove_dir(dir.path().join("haex.log")).unwrap();
    log.write_line("recovered").unwrap();

    assert!(!dir.path().join("haex.log.3").exists());
    assert!(read(dir.path(), "haex.log").contains("recovered"));
}

#[test]
fn a_partial_append_is_removed_before_the_next_record() {
    let dir = tempfile::tempdir().unwrap();
    let mut log = RotatingFile::open(dir.path(), SMALL).unwrap();
    log.write_line("kept").unwrap();
    let size = log.size;
    log.file.as_mut().unwrap().write_all(b"partial").unwrap();
    log.repair_to = Some(size);

    log.write_line("next").unwrap();

    assert_eq!(read(dir.path(), "haex.log"), "kept\nnext\n");
}

#[test]
fn a_partial_append_is_repaired_before_compaction_replaces_the_current_file() {
    let dir = tempfile::tempdir().unwrap();
    let big = Limits {
        files: 5,
        bytes: 1000,
    };
    let mut log = RotatingFile::open(dir.path(), big).unwrap();
    for number in 1..=8 {
        log.write_line(&record(number, 100)).unwrap();
    }
    let size = log.size;
    log.file
        .as_mut()
        .unwrap()
        .write_all(b"partial-record")
        .unwrap();
    log.repair_to = Some(size);

    log.set_limits(SMALL).unwrap();

    assert!(names(dir.path())
        .iter()
        .filter(|name| name.starts_with(FILE_NAME))
        .all(|name| !read(dir.path(), name).contains("partial-record")));
}

#[test]
fn an_interrupted_compaction_finishes_on_open() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("haex.log"), "new current\n").unwrap();
    fs::write(dir.path().join("haex.log.compact-1"), "new archive\n").unwrap();
    fs::write(dir.path().join(COMPACT_READY), "2").unwrap();

    let _log = RotatingFile::open(dir.path(), SMALL).unwrap();

    assert_eq!(read(dir.path(), "haex.log"), "new current\n");
    assert_eq!(read(dir.path(), "haex.log.1"), "new archive\n");
    assert!(!dir.path().join(COMPACT_READY).exists());
}

#[test]
fn opening_with_smaller_limits_than_the_files_left_brings_them_within() {
    let dir = tempfile::tempdir().unwrap();
    let big = Limits {
        files: 5,
        bytes: 1000,
    };
    {
        let mut log = RotatingFile::open(dir.path(), big).unwrap();
        for number in 1..=40 {
            log.write_line(&record(number, 100)).unwrap();
        }
    }
    let log = RotatingFile::open(dir.path(), SMALL);
    assert!(log.is_ok());
    assert_eq!(names(dir.path()), ["haex.log", "haex.log.1", "haex.log.2"]);
}

#[test]
fn switching_to_larger_limits_changes_nothing_on_disk() {
    let dir = tempfile::tempdir().unwrap();
    let mut log = RotatingFile::open(dir.path(), SMALL).unwrap();
    for number in 1..=5 {
        log.write_line(&record(number, 100)).unwrap();
    }
    let before = (read(dir.path(), "haex.log"), read(dir.path(), "haex.log.1"));
    log.set_limits(Limits {
        files: 5,
        bytes: 1000,
    })
    .unwrap();
    assert_eq!(
        before,
        (read(dir.path(), "haex.log"), read(dir.path(), "haex.log.1"))
    );
}
