use std::time::Duration;

use super::*;

const SHORT: Duration = Duration::from_secs(1);

fn root_with_watched() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join(WATCHED)).unwrap();
    root
}

#[test]
fn without_the_grant_the_step_passes_and_says_so() {
    let root = root_with_watched();
    let step = check(Ok(false), root.path(), || panic!("no watch"), SHORT);
    assert!(step.ok);
    assert_eq!(step.detail.as_deref(), Some("granted: false"));
}

#[test]
fn an_unknown_state_fails_the_step() {
    let root = root_with_watched();
    let step = check(Err("gone".to_owned()), root.path(), || {}, SHORT);
    assert!(!step.ok);
    assert_eq!(step.detail.as_deref(), Some("access unknown: gone"));
}

#[test]
fn with_the_grant_it_lists_and_sees_the_new_file() {
    let root = root_with_watched();
    let touched = root.path().join(WATCHED).join(TOUCHED);
    let step = check(
        Ok(true),
        root.path(),
        || std::fs::write(&touched, b"probe").unwrap(),
        Duration::from_secs(10),
    );
    assert!(step.ok, "{step:?}");
    assert_eq!(
        step.detail.as_deref(),
        Some("granted: true, 1 entries, watch event seen")
    );
}

#[test]
fn with_the_grant_a_missing_event_fails_the_step() {
    let root = root_with_watched();
    let step = check(Ok(true), root.path(), || {}, SHORT);
    assert!(!step.ok);
    assert_eq!(
        step.detail.as_deref(),
        Some("granted: true, 1 entries, no watch event within 1 s")
    );
}

#[test]
fn with_the_grant_an_unreadable_root_fails_the_step() {
    let root = tempfile::tempdir().unwrap();
    let step = check(Ok(true), &root.path().join("gone"), || {}, SHORT);
    assert!(!step.ok);
    assert!(step
        .detail
        .as_deref()
        .is_some_and(|detail| detail.starts_with("granted: true, listing failed")));
}
