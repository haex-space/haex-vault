//! Tests for what a start without an answer reports.

use std::path::PathBuf;

use tokio::sync::watch;

use super::process::{start_report, BrowserProcess, Launch};

fn launch(workdir: PathBuf, headless: bool) -> Launch {
    Launch {
        executable: PathBuf::from("chromium"),
        downloads: workdir.join("downloads"),
        workdir,
        headless,
    }
}

#[test]
fn a_headless_start_reports_the_end_and_the_error_output() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("holzi-stderr.log"),
        format!("{}\nFATAL: no display\n", "x".repeat(3000)),
    )
    .unwrap();
    let (_report, exit) = watch::channel(Some(false));
    let report = start_report(
        &BrowserProcess::new(1, exit),
        &launch(dir.path().to_owned(), true),
    );
    assert!(report.contains("browser ended, success false"), "{report}");
    assert!(report.ends_with("FATAL: no display)"), "{report}");
    assert!(
        report.len() < 2100,
        "only the end of the output: {}",
        report.len()
    );
}

#[test]
fn a_start_with_a_window_adds_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (_report, exit) = watch::channel(None);
    let report = start_report(
        &BrowserProcess::new(1, exit),
        &launch(dir.path().to_owned(), false),
    );
    assert_eq!(report, "");
}
