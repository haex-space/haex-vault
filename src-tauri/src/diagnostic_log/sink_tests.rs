use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use jiff::civil::date;
use log::Level;

use super::*;

fn log_text(folder: &std::path::Path) -> String {
    fs::read_to_string(folder.join("haex.log")).unwrap_or_default()
}

#[test]
fn a_record_is_dropped_while_no_writer_is_attached_and_never_buffered() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("logs");
    let sink = DiagnosticSink::new();

    sink.write_record(Level::Error, "haex_lib::unlock", "before the unlock");
    assert!(!sink.is_attached());
    assert!(!folder.exists(), "no folder is made before the unlock");

    sink.attach(&folder, LogLevel::Standard).unwrap();
    sink.write_record(Level::Error, "haex_lib::unlock", "after the unlock");
    sink.flush_sync();

    let text = log_text(&folder);
    assert!(!text.contains("before the unlock"), "{text}");
    assert!(text.contains("after the unlock"));
}

#[test]
fn an_attached_sink_writes_the_documented_line() {
    let dir = tempfile::tempdir().unwrap();
    let sink = DiagnosticSink::new();
    sink.attach(dir.path(), LogLevel::Standard).unwrap();
    sink.write_record(Level::Warn, "haex_lib::sync", "something went wrong");
    sink.flush_sync();

    let text = log_text(dir.path());
    let line = text.lines().next().unwrap();
    let (stamp, rest) = line.split_at(line.find("[WARN]").unwrap());
    assert_eq!(rest, "[WARN][haex_lib::sync] something went wrong");
    // [YYYY-MM-DD][HH:MM:SS.mmm]
    assert_eq!(stamp.len(), "[2026-11-21][07:02:02.123]".len(), "{stamp}");
    assert!(stamp.starts_with("[20"));
}

#[test]
fn the_line_carries_date_time_level_and_origin_in_order() {
    let at = date(2026, 11, 21).at(7, 2, 2, 123_000_000);
    assert_eq!(
        format_record(at, Level::Error, "ui", "boom"),
        "[2026-11-21][07:02:02.123][ERROR][ui] boom"
    );
    assert_eq!(
        format_record(at, Level::Debug, "haex_lib::x", "d"),
        "[2026-11-21][07:02:02.123][DEBUG][haex_lib::x] d"
    );
}

#[test]
fn a_message_with_a_line_break_is_one_record_with_indented_lines() {
    let at = date(2026, 11, 21).at(7, 2, 2, 0);
    let line = format_record(at, Level::Error, "ui", "TypeError: x\nat a\r\nat b");
    assert_eq!(
        line,
        "[2026-11-21][07:02:02.000][ERROR][ui] TypeError: x\n    at a\n    at b"
    );
}

#[test]
fn an_overlong_message_is_cut() {
    let at = date(2026, 11, 21).at(7, 2, 2, 0);
    let message = "x\n".repeat(MAX_RECORD / 2);
    let line = format_record(at, Level::Warn, "ui", &message);
    assert!(
        line.len() + 1 <= MAX_RECORD,
        "the serialized record is bounded"
    );
    assert!(line.ends_with("[cut]"));
    assert!(line.lines().skip(1).all(|line| line.starts_with("    ")));
}

#[test]
fn the_record_limit_includes_a_long_origin() {
    let at = date(2026, 11, 21).at(7, 2, 2, 0);
    let line = format_record(at, Level::Warn, &"x".repeat(MAX_RECORD), "message");
    assert!(line.len() + 1 <= MAX_RECORD);
}

#[test]
fn a_nested_sink_record_is_dropped_without_poisoning_the_guard() {
    let outer = SinkWriteGuard::enter().expect("outer sink write");
    assert!(SinkWriteGuard::enter().is_none(), "nested write is dropped");
    drop(outer);
    assert!(
        SinkWriteGuard::enter().is_some(),
        "a later write is allowed"
    );
}

#[test]
fn a_ui_record_gets_origin_ui_and_keeps_its_stack_indented() {
    let dir = tempfile::tempdir().unwrap();
    let sink = DiagnosticSink::new();
    let session = sink.attach(dir.path(), LogLevel::Standard).unwrap();
    sink.write_ui(&session, Level::Error, "Error: probe\nat here");
    sink.flush_sync();

    let text = log_text(dir.path());
    assert!(
        text.contains("[ERROR][ui] Error: probe\n    at here\n"),
        "{text}"
    );
}

#[test]
fn a_ui_record_of_another_session_is_dropped() {
    let dir = tempfile::tempdir().unwrap();
    let sink = DiagnosticSink::new();
    let first = sink.attach(dir.path(), LogLevel::Standard).unwrap();
    let second = sink.attach(dir.path(), LogLevel::Standard).unwrap();
    assert_ne!(first, second);

    sink.write_ui(&first, Level::Error, "for the earlier vault");
    sink.write_ui("", Level::Error, "without a token");
    sink.write_ui(&second, Level::Error, "for this vault");
    sink.flush_sync();

    let text = log_text(dir.path());
    assert!(!text.contains("earlier"), "{text}");
    assert!(!text.contains("without"));
    assert!(text.contains("for this vault"));
}

#[test]
fn a_record_the_level_does_not_allow_is_not_written() {
    let dir = tempfile::tempdir().unwrap();
    let sink = DiagnosticSink::new();
    let session = sink.attach(dir.path(), LogLevel::Standard).unwrap();
    sink.write_record(Level::Debug, "haex_lib::sync", "detail");
    sink.write_ui(&session, Level::Info, "ui information is kept");
    sink.write_ui(&session, Level::Debug, "ui detail");
    sink.write_record(Level::Info, "iroh::magicsock", "packet");
    sink.flush_sync();

    let text = log_text(dir.path());
    assert!(!text.contains("detail"), "{text}");
    assert!(!text.contains("packet"));
    assert!(text.contains("ui information is kept"));

    sink.set_level(LogLevel::Verbose).unwrap();
    sink.write_record(Level::Debug, "haex_lib::sync", "now detail");
    sink.write_record(Level::Info, "iroh::magicsock", "still no packet");
    sink.flush_sync();
    let text = log_text(dir.path());
    assert!(text.contains("now detail"));
    assert!(!text.contains("still no packet"));
}

#[test]
fn the_log_trait_filters_by_level_and_attachment() {
    let dir = tempfile::tempdir().unwrap();
    let sink = DiagnosticSink::new();
    let debug = log::Metadata::builder()
        .level(Level::Debug)
        .target("haex_lib::x")
        .build();
    assert!(!sink.enabled(&debug), "detached");
    sink.attach(dir.path(), LogLevel::Standard).unwrap();
    assert!(!sink.enabled(&debug));
    sink.set_level(LogLevel::Verbose).unwrap();
    assert!(sink.enabled(&debug));
}

#[test]
fn detaching_flushes_and_stops_writing() {
    let dir = tempfile::tempdir().unwrap();
    let sink = DiagnosticSink::new();
    sink.attach(dir.path(), LogLevel::Standard).unwrap();
    sink.write_record(Level::Info, "haex_lib::a", "kept");
    sink.detach();
    sink.write_record(Level::Info, "haex_lib::a", "dropped");

    let text = log_text(dir.path());
    assert!(text.contains("kept"));
    assert!(!text.contains("dropped"));
    assert!(!sink.is_attached());
    assert_eq!(sink.session(), None);
}

#[test]
fn a_second_attachment_moves_the_writing_and_leaves_the_first_folder_alone() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (dir.path().join("a"), dir.path().join("b"));
    let sink = DiagnosticSink::new();
    sink.attach(&a, LogLevel::Standard).unwrap();
    sink.write_record(Level::Info, "haex_lib::a", "in a");
    sink.attach(&b, LogLevel::Standard).unwrap();
    sink.write_record(Level::Info, "haex_lib::a", "in b");
    sink.flush_sync();

    assert!(log_text(&a).contains("in a") && !log_text(&a).contains("in b"));
    assert!(log_text(&b).contains("in b") && !log_text(&b).contains("in a"));
}

#[test]
fn a_failed_write_sets_the_flag_once_and_the_next_write_is_tried_again() {
    let dir = tempfile::tempdir().unwrap();
    let sink = DiagnosticSink::new();
    let announced = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&announced);
    sink.on_unwritable(move || {
        seen.fetch_add(1, Ordering::SeqCst);
    });
    sink.attach(dir.path(), LogLevel::Standard).unwrap();
    sink.write_record(Level::Info, "haex_lib::a", "fine");
    sink.flush_sync();
    assert!(!sink.unwritable());

    // A folder where the oldest archive belongs makes the next rotation fail.
    fs::create_dir(dir.path().join("haex.log.3")).unwrap();
    let long = "z".repeat(MAX_RECORD);
    for _ in 0..40 {
        sink.write_record(Level::Warn, "haex_lib::a", &long);
        sink.flush_sync();
        if sink.unwritable() {
            break;
        }
    }
    assert!(sink.unwritable());
    sink.write_record(Level::Warn, "haex_lib::a", &long);
    sink.flush_sync();
    assert_eq!(
        announced.load(Ordering::SeqCst),
        1,
        "announced once, not per line"
    );

    fs::remove_dir(dir.path().join("haex.log.3")).unwrap();
    sink.write_record(Level::Warn, "haex_lib::a", "recovered");
    sink.flush_sync();
    assert!(!sink.unwritable());
    assert!(log_text(dir.path()).contains("recovered"));
}

#[test]
fn an_attachment_that_cannot_open_leaves_the_sink_detached_and_flagged() {
    let dir = tempfile::tempdir().unwrap();
    let blocked = dir.path().join("logs");
    fs::write(&blocked, "a file where the folder belongs").unwrap();
    let sink = DiagnosticSink::new();

    assert!(sink.attach(&blocked, LogLevel::Standard).is_err());
    assert!(!sink.is_attached());
    assert!(sink.unwritable());
    sink.write_record(Level::Error, "haex_lib::a", "nowhere to go");

    let folder = dir.path().join("fine");
    sink.attach(&folder, LogLevel::Standard).unwrap();
    assert!(!sink.unwritable(), "a good attachment clears the flag");
}
