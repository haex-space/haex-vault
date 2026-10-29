use std::fs::{self, File};
use std::sync::{Arc, Barrier};
use std::time::{Duration, SystemTime};

use super::*;

fn files(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .expect("read")
        .map(|entry| entry.expect("entry").file_name().to_string_lossy().into())
        .collect();
    names.sort();
    names
}

fn age(path: &Path, by: Duration) {
    File::options()
        .write(true)
        .open(path)
        .expect("open")
        .set_modified(SystemTime::now() - by)
        .expect("set mtime");
}

#[test]
fn an_offered_link_is_claimed_once_and_the_offerer_learns_it() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().to_path_buf();
    let taker = std::thread::spawn({
        let path = path.clone();
        move || loop {
            if let Some(url) = claim_next(&path, true).expect("claim") {
                return url;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    });
    let offered = offer(&path, "haex-vault://contact/abc").expect("offer");
    assert_eq!(offered, Offered::Taken);
    assert_eq!(taker.join().expect("taker"), "haex-vault://contact/abc");
    assert!(files(&path).is_empty(), "{:?}", files(&path));
}

#[test]
fn of_several_claimers_exactly_one_gets_the_link() {
    let dir = tempfile::tempdir().expect("dir");
    fs::write(dir.path().join("one.url"), "haex-vault://contact/abc").expect("write");
    let start = Arc::new(Barrier::new(8));
    let claimers: Vec<_> = (0..8)
        .map(|_| {
            let path = dir.path().to_path_buf();
            let start = Arc::clone(&start);
            std::thread::spawn(move || {
                start.wait();
                claim_next(&path, true).expect("claim")
            })
        })
        .collect();
    let got: Vec<String> = claimers
        .into_iter()
        .filter_map(|claimer| claimer.join().expect("claimer"))
        .collect();
    assert_eq!(got, vec!["haex-vault://contact/abc".to_string()]);
    assert!(files(dir.path()).is_empty());
}

#[test]
fn a_link_nobody_takes_is_removed_and_reported() {
    let dir = tempfile::tempdir().expect("dir");
    let started = Instant::now();
    let offered = offer(dir.path(), "haex-vault://contact/abc").expect("offer");
    assert_eq!(offered, Offered::NoTaker);
    assert!(started.elapsed() >= HANDOFF_TIMEOUT);
    assert!(files(dir.path()).is_empty());
}

#[test]
fn a_stale_file_is_not_taken_and_a_very_old_one_is_removed() {
    let dir = tempfile::tempdir().expect("dir");
    let stale = dir.path().join("stale.url");
    fs::write(&stale, "haex-vault://contact/stale").expect("write");
    age(&stale, HANDOFF_TIMEOUT + Duration::from_secs(1));
    assert_eq!(claim_next(dir.path(), true).expect("claim"), None);
    assert!(stale.exists(), "its writer may still be about to remove it");

    let ancient = dir.path().join("old.claimed-1");
    fs::write(&ancient, "x").expect("write");
    age(&ancient, HANDOFF_TIMEOUT * 3);
    let abandoned = dir.path().join("abandoned.tmp");
    fs::write(&abandoned, "x").expect("write");
    age(&abandoned, HANDOFF_TIMEOUT * 3);
    age(&stale, HANDOFF_TIMEOUT * 3);
    assert_eq!(claim_next(dir.path(), true).expect("claim"), None);
    assert!(files(dir.path()).is_empty(), "{:?}", files(dir.path()));
}

#[test]
fn a_missing_directory_or_foreign_files_hold_nothing() {
    let dir = tempfile::tempdir().expect("dir");
    assert_eq!(
        claim_next(&dir.path().join("none"), true).expect("claim"),
        None
    );
    fs::write(dir.path().join("notes.txt"), "x").expect("write");
    assert_eq!(claim_next(dir.path(), true).expect("claim"), None);
    assert!(dir.path().join("notes.txt").exists());
}

#[test]
fn a_window_without_focus_leaves_a_fresh_link_to_the_others() {
    let dir = tempfile::tempdir().expect("dir");
    let link = dir.path().join("one.url");
    fs::write(&link, "mailto:ben@example.org").expect("write");
    assert_eq!(claim_next(dir.path(), false).expect("claim"), None);
    assert!(link.exists(), "the focused window may still take it");

    age(&link, UNFOCUSED_DELAY + Duration::from_millis(100));
    assert_eq!(
        claim_next(dir.path(), false).expect("claim").as_deref(),
        Some("mailto:ben@example.org")
    );
}

#[test]
fn a_focused_window_takes_a_fresh_link_at_once() {
    let dir = tempfile::tempdir().expect("dir");
    fs::write(dir.path().join("one.url"), "mailto:ben@example.org").expect("write");
    assert_eq!(
        claim_next(dir.path(), true).expect("claim").as_deref(),
        Some("mailto:ben@example.org")
    );
}
