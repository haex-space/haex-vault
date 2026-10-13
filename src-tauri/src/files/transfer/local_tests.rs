use std::cell::RefCell;
use std::path::{Path, PathBuf};

use tokio_util::sync::CancellationToken;

use super::*;
use crate::files::local::OwnPlaces;
use crate::files::FilesErrorCode;

pub(super) fn folder() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let real = std::fs::canonicalize(dir.path()).unwrap();
    (dir, real)
}

pub(super) fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

pub(super) fn job(op: TransferOp, sources: &[PathBuf], target: Option<&Path>) -> Job {
    Job {
        op,
        sources: sources.to_vec(),
        target: target.map(Path::to_path_buf),
    }
}

/// Plenty of space, everything on one file system.
fn prepared(job: &Job) -> Result<Totals, FilesError> {
    prepare(job, |_| None, |_, _| true)
}

/// Runs `job` with the answers in `answers` (taken in order) and no cancellation.
fn run_answering(job: &Job, answers: &[(ConflictChoice, bool)]) -> (Outcome, Vec<String>) {
    let asked = RefCell::new(Vec::new());
    let mut answers = answers.iter().copied();
    let cancel = CancellationToken::new();
    let totals = prepared(job).unwrap();
    let outcome = run(
        job,
        &totals,
        Hooks {
            cancel: &cancel,
            ask: &mut |name| {
                asked.borrow_mut().push(name.to_owned());
                answers.next()
            },
            progress: &mut |_| {},
            removal: Removal::Permanent,
            rename: &|from, to| std::fs::rename(from, to),
        },
    );
    (outcome, asked.into_inner())
}

#[test]
fn a_copy_writes_a_part_file_and_names_it_at_the_end() {
    let (_dir, real) = folder();
    std::fs::create_dir_all(real.join("von")).unwrap();
    std::fs::create_dir_all(real.join("nach")).unwrap();
    let content = vec![7u8; 3 * CHUNK + 5];
    std::fs::write(real.join("von/gross.bin"), &content).unwrap();
    let job = job(
        TransferOp::Copy,
        &[real.join("von/gross.bin")],
        Some(&real.join("nach")),
    );
    let totals = prepared(&job).unwrap();
    assert_eq!(totals.items, 1);
    assert_eq!(totals.bytes, content.len() as u64);
    let seen = RefCell::new(Vec::new());
    let cancel = CancellationToken::new();
    let outcome = run(
        &job,
        &totals,
        Hooks {
            cancel: &cancel,
            ask: &mut |_| None,
            progress: &mut |_| seen.borrow_mut().push(names(&real.join("nach"))),
            removal: Removal::Permanent,
            rename: &|from, to| std::fs::rename(from, to),
        },
    );
    assert_eq!(outcome, Outcome::Done);
    let during = seen.borrow();
    let part = &during[0][0];
    assert!(
        part.starts_with(".gross.bin.holzi-part-"),
        "while copying: {during:?}"
    );
    assert_eq!(names(&real.join("nach")), ["gross.bin"]);
    assert_eq!(std::fs::read(real.join("nach/gross.bin")).unwrap(), content);
}

#[test]
fn a_cancelled_copy_leaves_no_part_file() {
    let (_dir, real) = folder();
    std::fs::create_dir_all(real.join("nach")).unwrap();
    std::fs::write(real.join("gross.bin"), vec![1u8; 4 * CHUNK]).unwrap();
    let job = job(
        TransferOp::Copy,
        &[real.join("gross.bin")],
        Some(&real.join("nach")),
    );
    let totals = prepared(&job).unwrap();
    let cancel = CancellationToken::new();
    let outcome = run(
        &job,
        &totals,
        Hooks {
            cancel: &cancel,
            ask: &mut |_| None,
            progress: &mut |_| cancel.cancel(),
            removal: Removal::Permanent,
            rename: &|from, to| std::fs::rename(from, to),
        },
    );
    assert_eq!(outcome, Outcome::Cancelled);
    assert!(names(&real.join("nach")).is_empty());
}

/// T048: the vault closes during a copy; the transfer's token is a child of the gate's.
#[test]
fn closing_the_vault_during_a_copy_leaves_no_part_file() {
    let (_dir, real) = folder();
    std::fs::create_dir_all(real.join("nach")).unwrap();
    std::fs::write(real.join("gross.bin"), vec![1u8; 4 * CHUNK]).unwrap();
    let job = job(
        TransferOp::Copy,
        &[real.join("gross.bin")],
        Some(&real.join("nach")),
    );
    let totals = prepared(&job).unwrap();
    let gate = CancellationToken::new();
    let transfer = gate.child_token();
    let outcome = run(
        &job,
        &totals,
        Hooks {
            cancel: &transfer,
            ask: &mut |_| None,
            progress: &mut |_| gate.cancel(),
            removal: Removal::Permanent,
            rename: &|from, to| std::fs::rename(from, to),
        },
    );
    assert_eq!(outcome, Outcome::Cancelled);
    assert!(names(&real.join("nach")).is_empty());
}

fn conflict_scene() -> (tempfile::TempDir, PathBuf, Job) {
    let (dir, real) = folder();
    std::fs::create_dir_all(real.join("von")).unwrap();
    std::fs::create_dir_all(real.join("nach")).unwrap();
    for name in ["a.txt", "b.txt"] {
        std::fs::write(real.join("von").join(name), "neu").unwrap();
        std::fs::write(real.join("nach").join(name), "alt").unwrap();
    }
    let job = job(
        TransferOp::Copy,
        &[real.join("von/a.txt"), real.join("von/b.txt")],
        Some(&real.join("nach")),
    );
    (dir, real, job)
}

#[test]
fn a_conflict_can_replace() {
    let (_dir, real, job) = conflict_scene();
    let (outcome, asked) = run_answering(&job, &[(ConflictChoice::Replace, true)]);
    assert_eq!(outcome, Outcome::Done);
    assert_eq!(asked, ["a.txt"], "for all asks once");
    assert_eq!(names(&real.join("nach")), ["a.txt", "b.txt"]);
    for name in ["a.txt", "b.txt"] {
        let text = std::fs::read_to_string(real.join("nach").join(name)).unwrap();
        assert_eq!(text, "neu");
    }
}

#[test]
fn a_conflict_can_keep_both() {
    let (_dir, real, job) = conflict_scene();
    let (outcome, asked) = run_answering(
        &job,
        &[
            (ConflictChoice::KeepBoth, false),
            (ConflictChoice::KeepBoth, false),
        ],
    );
    assert_eq!(outcome, Outcome::Done);
    assert_eq!(asked, ["a.txt", "b.txt"], "without for all every name asks");
    assert_eq!(
        names(&real.join("nach")),
        ["a (2).txt", "a.txt", "b (2).txt", "b.txt"]
    );
    let kept = std::fs::read_to_string(real.join("nach/a.txt")).unwrap();
    assert_eq!(kept, "alt");
}

#[test]
fn a_conflict_can_skip() {
    let (_dir, real, job) = conflict_scene();
    let (outcome, _) = run_answering(&job, &[(ConflictChoice::Skip, true)]);
    assert_eq!(outcome, Outcome::Done);
    for name in ["a.txt", "b.txt"] {
        let text = std::fs::read_to_string(real.join("nach").join(name)).unwrap();
        assert_eq!(text, "alt");
    }
}

#[test]
fn no_answer_cancels() {
    let (_dir, real, job) = conflict_scene();
    let (outcome, _) = run_answering(&job, &[]);
    assert_eq!(outcome, Outcome::Cancelled);
    assert_eq!(names(&real.join("nach")), ["a.txt", "b.txt"]);
}

#[test]
fn a_folder_onto_a_folder_merges_and_asks_only_for_files() {
    let (_dir, real) = folder();
    std::fs::create_dir_all(real.join("von/Fotos")).unwrap();
    std::fs::create_dir_all(real.join("nach/Fotos")).unwrap();
    std::fs::write(real.join("von/Fotos/neu.jpg"), "n").unwrap();
    std::fs::write(real.join("von/Fotos/beide.jpg"), "n").unwrap();
    std::fs::write(real.join("nach/Fotos/alt.jpg"), "a").unwrap();
    std::fs::write(real.join("nach/Fotos/beide.jpg"), "a").unwrap();
    let job = job(
        TransferOp::Copy,
        &[real.join("von/Fotos")],
        Some(&real.join("nach")),
    );
    let (outcome, asked) = run_answering(&job, &[(ConflictChoice::Skip, false)]);
    assert_eq!(outcome, Outcome::Done);
    assert_eq!(asked, ["beide.jpg"]);
    assert_eq!(
        names(&real.join("nach/Fotos")),
        ["alt.jpg", "beide.jpg", "neu.jpg"]
    );
}

#[test]
fn copying_into_the_same_folder_keeps_both_without_asking() {
    let (_dir, real) = folder();
    std::fs::write(real.join("x.txt"), "x").unwrap();
    std::fs::create_dir(real.join("Ordner")).unwrap();
    std::fs::write(real.join("Ordner/innen.txt"), "i").unwrap();
    let job = job(
        TransferOp::Copy,
        &[real.join("x.txt"), real.join("Ordner")],
        Some(&real),
    );
    let (outcome, asked) = run_answering(&job, &[]);
    assert_eq!(outcome, Outcome::Done);
    assert!(asked.is_empty());
    assert_eq!(names(&real), ["Ordner", "Ordner (2)", "x (2).txt", "x.txt"]);
    assert_eq!(names(&real.join("Ordner (2)")), ["innen.txt"]);
}

#[test]
fn a_folder_cannot_go_into_itself_or_below() {
    let (_dir, real) = folder();
    std::fs::create_dir_all(real.join("a/b")).unwrap();
    for op in [TransferOp::Copy, TransferOp::Move] {
        for target in [real.join("a"), real.join("a/b")] {
            let error = prepared(&job(op, &[real.join("a")], Some(&target))).unwrap_err();
            assert_eq!(
                error.code,
                FilesErrorCode::IntoItself,
                "{op:?} into {target:?}"
            );
        }
    }
    // A sibling whose name starts the same is not inside.
    std::fs::create_dir(real.join("ab")).unwrap();
    assert!(prepared(&job(
        TransferOp::Copy,
        &[real.join("a")],
        Some(&real.join("ab"))
    ))
    .is_ok());
}

#[test]
fn too_little_space_is_refused_before_the_start() {
    let (_dir, real) = folder();
    std::fs::create_dir(real.join("nach")).unwrap();
    std::fs::write(real.join("gross.bin"), vec![0u8; 1000]).unwrap();
    let copy = job(
        TransferOp::Copy,
        &[real.join("gross.bin")],
        Some(&real.join("nach")),
    );
    let error = prepare(&copy, |_| Some(999), |_, _| true).unwrap_err();
    assert_eq!(error.code, FilesErrorCode::NoSpace);
    assert!(prepare(&copy, |_| Some(1000), |_, _| true).is_ok());
    // A move within one file system needs no space; across file systems it does.
    let moving = job(
        TransferOp::Move,
        &[real.join("gross.bin")],
        Some(&real.join("nach")),
    );
    assert!(prepare(&moving, |_| Some(0), |_, _| true).is_ok());
    let error = prepare(&moving, |_| Some(0), |_, _| false).unwrap_err();
    assert_eq!(error.code, FilesErrorCode::NoSpace);
}

#[cfg(unix)]
#[test]
fn a_move_on_one_file_system_is_a_rename() {
    use std::os::unix::fs::MetadataExt;
    let (_dir, real) = folder();
    std::fs::create_dir(real.join("nach")).unwrap();
    std::fs::create_dir(real.join("Ordner")).unwrap();
    std::fs::write(real.join("Ordner/a.txt"), "a").unwrap();
    let inode = std::fs::metadata(real.join("Ordner")).unwrap().ino();
    let job = job(
        TransferOp::Move,
        &[real.join("Ordner")],
        Some(&real.join("nach")),
    );
    let (outcome, _) = run_answering(&job, &[]);
    assert_eq!(outcome, Outcome::Done);
    assert!(!real.join("Ordner").exists());
    assert_eq!(
        std::fs::metadata(real.join("nach/Ordner")).unwrap().ino(),
        inode
    );
}

#[test]
fn a_move_across_file_systems_copies_and_deletes() {
    let (_dir, real) = folder();
    std::fs::create_dir(real.join("nach")).unwrap();
    std::fs::create_dir(real.join("Ordner")).unwrap();
    std::fs::write(real.join("Ordner/a.txt"), "a").unwrap();
    let job = job(
        TransferOp::Move,
        &[real.join("Ordner")],
        Some(&real.join("nach")),
    );
    let totals = prepared(&job).unwrap();
    let cancel = CancellationToken::new();
    let renames = RefCell::new(0);
    let outcome = run(
        &job,
        &totals,
        Hooks {
            cancel: &cancel,
            ask: &mut |_| None,
            progress: &mut |_| {},
            removal: Removal::Permanent,
            // Only the part files of the copy may be renamed; the move itself crosses devices.
            rename: &|from, to| {
                if from
                    .file_name()
                    .is_some_and(|n| n.to_string_lossy().contains(".holzi-part-"))
                {
                    *renames.borrow_mut() += 1;
                    std::fs::rename(from, to)
                } else {
                    Err(std::io::Error::from(std::io::ErrorKind::CrossesDevices))
                }
            },
        },
    );
    assert_eq!(outcome, Outcome::Done);
    assert!(!real.join("Ordner").exists());
    assert_eq!(
        std::fs::read_to_string(real.join("nach/Ordner/a.txt")).unwrap(),
        "a"
    );
    assert_eq!(*renames.borrow(), 1);
}

#[test]
fn moving_into_the_folder_it_is_in_does_nothing() {
    let (_dir, real) = folder();
    std::fs::write(real.join("x.txt"), "x").unwrap();
    let job = job(TransferOp::Move, &[real.join("x.txt")], Some(&real));
    let (outcome, asked) = run_answering(&job, &[]);
    assert_eq!(outcome, Outcome::Done);
    assert!(asked.is_empty());
    assert_eq!(names(&real), ["x.txt"]);
}

#[test]
fn a_permanent_delete_removes_files_and_folders() {
    let (_dir, real) = folder();
    std::fs::create_dir_all(real.join("Ordner/tief")).unwrap();
    std::fs::write(real.join("Ordner/tief/a.txt"), "a").unwrap();
    std::fs::write(real.join("x.txt"), "x").unwrap();
    std::fs::write(real.join("bleibt.txt"), "b").unwrap();
    let job = job(
        TransferOp::Delete,
        &[real.join("Ordner"), real.join("x.txt")],
        None,
    );
    let totals = prepared(&job).unwrap();
    assert_eq!(totals.items, 2, "files below the selection");
    let (outcome, _) = run_answering(&job, &[]);
    assert_eq!(outcome, Outcome::Done);
    assert_eq!(names(&real), ["bleibt.txt"]);
}

#[cfg(unix)]
#[test]
fn a_link_is_copied_and_deleted_as_a_link() {
    let (_dir, real) = folder();
    std::fs::create_dir_all(real.join("ziel")).unwrap();
    std::fs::write(real.join("ziel/a.txt"), "a").unwrap();
    std::fs::create_dir(real.join("nach")).unwrap();
    std::os::unix::fs::symlink(real.join("ziel"), real.join("link")).unwrap();
    let copy = job(
        TransferOp::Copy,
        &[real.join("link")],
        Some(&real.join("nach")),
    );
    assert_eq!(run_answering(&copy, &[]).0, Outcome::Done);
    let copied = std::fs::symlink_metadata(real.join("nach/link")).unwrap();
    assert!(copied.file_type().is_symlink());
    let delete = job(TransferOp::Delete, &[real.join("link")], None);
    assert_eq!(run_answering(&delete, &[]).0, Outcome::Done);
    assert!(real.join("ziel/a.txt").exists(), "the target stays");
}

#[test]
fn moving_or_deleting_a_folder_that_holds_holzis_data_touches_it() {
    let (_dir, real) = folder();
    std::fs::create_dir_all(real.join("share/holzi")).unwrap();
    std::fs::create_dir_all(real.join("nach")).unwrap();
    let own = OwnPlaces::new(vec![real.join("share/holzi")]);
    let share = [real.join("share")];

    assert!(touches_own(&job(TransferOp::Delete, &share, None), &own));
    assert!(touches_own(
        &job(TransferOp::Move, &share, Some(&real.join("nach"))),
        &own
    ));
    // Reading holzi's data is allowed.
    assert!(!touches_own(
        &job(TransferOp::Copy, &share, Some(&real.join("nach"))),
        &own
    ));
}

#[test]
fn a_folder_that_would_merge_into_holzis_data_touches_it() {
    let (_dir, real) = folder();
    std::fs::create_dir_all(real.join("home/.config/holzi")).unwrap();
    std::fs::create_dir_all(real.join("backup/.config/holzi")).unwrap();
    std::fs::create_dir_all(real.join("backup/Fotos")).unwrap();
    let own = OwnPlaces::new(vec![real.join("home/.config/holzi")]);
    let home = real.join("home");

    assert!(touches_own(
        &job(
            TransferOp::Copy,
            &[real.join("backup/.config")],
            Some(&home)
        ),
        &own
    ));
    assert!(!touches_own(
        &job(TransferOp::Copy, &[real.join("backup/Fotos")], Some(&home)),
        &own
    ));
}
