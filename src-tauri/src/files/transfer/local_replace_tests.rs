//! Replacing and retrying with a stand-in for the file system's rename (FR-020): a failure keeps
//! what was there.

use std::cell::RefCell;
use std::path::Path;

use tokio_util::sync::CancellationToken;

use super::tests::{folder, job, names};
use super::*;

/// Runs `job` with no question answered and `rename` standing in for the file system.
fn run_renaming(
    job: &Job,
    answer: Option<ConflictChoice>,
    rename: &dyn Fn(&Path, &Path) -> std::io::Result<()>,
) -> (Outcome, Vec<String>) {
    let asked = RefCell::new(Vec::new());
    let cancel = CancellationToken::new();
    let outcome = run(
        job,
        &Totals::default(),
        Hooks {
            cancel: &cancel,
            ask: &mut |name| {
                asked.borrow_mut().push(name.to_owned());
                answer.map(|choice| (choice, false))
            },
            progress: &mut |_| {},
            removal: Removal::Permanent,
            rename,
        },
    );
    (outcome, asked.into_inner())
}

#[test]
fn a_failed_replacing_move_keeps_the_old_target() {
    let (_dir, real) = folder();
    std::fs::create_dir_all(real.join("von")).unwrap();
    std::fs::create_dir_all(real.join("nach")).unwrap();
    std::fs::write(real.join("von/a.txt"), "neu").unwrap();
    std::fs::write(real.join("nach/a.txt"), "alt").unwrap();
    let job = job(
        TransferOp::Move,
        &[real.join("von/a.txt")],
        Some(&real.join("nach")),
    );

    let (outcome, _) = run_renaming(&job, Some(ConflictChoice::Replace), &|_, _| {
        Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
    });

    assert!(matches!(outcome, Outcome::Failed(_)));
    assert_eq!(
        std::fs::read_to_string(real.join("nach/a.txt")).unwrap(),
        "alt"
    );
    assert_eq!(names(&real.join("nach")), ["a.txt"]);
    assert_eq!(
        std::fs::read_to_string(real.join("von/a.txt")).unwrap(),
        "neu"
    );
}

#[test]
fn a_replacing_copy_of_a_folder_that_fails_keeps_the_old_folder() {
    let (_dir, real) = folder();
    std::fs::create_dir_all(real.join("von/Fotos")).unwrap();
    std::fs::create_dir_all(real.join("nach")).unwrap();
    std::fs::write(real.join("von/Fotos/neu.jpg"), "neu").unwrap();
    // A file against a folder asks; the old one is a file.
    std::fs::write(real.join("nach/Fotos"), "alt").unwrap();
    let job = job(
        TransferOp::Copy,
        &[real.join("von/Fotos")],
        Some(&real.join("nach")),
    );

    let (outcome, _) = run_renaming(&job, Some(ConflictChoice::Replace), &|_, _| {
        Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
    });

    assert!(matches!(outcome, Outcome::Failed(_)));
    assert_eq!(
        std::fs::read_to_string(real.join("nach/Fotos")).unwrap(),
        "alt"
    );
    assert_eq!(names(&real.join("nach")), ["Fotos"]);
}

#[test]
fn a_retried_move_passes_over_what_the_first_try_moved() {
    let (_dir, real) = folder();
    std::fs::create_dir_all(real.join("von")).unwrap();
    std::fs::create_dir_all(real.join("nach")).unwrap();
    // The first try moved a.txt, then failed on b.txt.
    std::fs::write(real.join("nach/a.txt"), "a").unwrap();
    std::fs::write(real.join("von/b.txt"), "b").unwrap();
    let job = job(
        TransferOp::Move,
        &[real.join("von/a.txt"), real.join("von/b.txt")],
        Some(&real.join("nach")),
    );

    let (outcome, asked) = run_renaming(&job, Some(ConflictChoice::Replace), &|from, to| {
        std::fs::rename(from, to)
    });

    assert_eq!(outcome, Outcome::Done);
    assert!(asked.is_empty(), "nothing to ask: {asked:?}");
    assert_eq!(
        std::fs::read_to_string(real.join("nach/a.txt")).unwrap(),
        "a"
    );
    assert_eq!(
        std::fs::read_to_string(real.join("nach/b.txt")).unwrap(),
        "b"
    );
}
