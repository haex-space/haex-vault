//! Which versions a run uses, and what that holds back (spec 094, T047, FR-010, FR-022).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use tempfile::TempDir;

use super::{prune, vendor_in_use, version_in_use, Lease};
use crate::cli_tools::catalog::{Catalog, VendorDefinition};
use crate::cli_tools::installed::{InstallMarker, InstalledFile, CONFIRMED_FILE};
use crate::cli_tools::platform::PlatformKey;

const HOST: Option<PlatformKey> = Some(PlatformKey::LinuxX64);

fn claude() -> Arc<VendorDefinition> {
    Arc::new(Catalog::shipped().vendor("claude").unwrap().clone())
}

/// A complete version folder of the Claude tool.
fn install(vendor_dir: &Path, version: &str) -> PathBuf {
    let dir = vendor_dir.join(version);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("claude"), [7u8; 8]).unwrap();
    InstallMarker::new(
        "claude",
        version,
        PlatformKey::LinuxX64,
        &"a".repeat(64),
        1,
        8,
        vec![InstalledFile {
            path: "claude".into(),
            size: 8,
        }],
    )
    .write(&dir)
    .unwrap();
    dir
}

fn lease(vendor_dir: &Path, version: &str) -> Lease {
    Lease::take(
        vendor_dir.to_path_buf(),
        version.to_string(),
        claude(),
        HOST,
        None,
    )
}

fn versions(vendor_dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(vendor_dir)
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn a_version_is_in_use_exactly_as_long_as_a_lease_on_it_lives() {
    let dir = TempDir::new().unwrap();
    let first = lease(dir.path(), "1.0.0");
    let second = lease(dir.path(), "1.0.0");
    assert!(version_in_use(dir.path(), "1.0.0"));
    assert!(!version_in_use(dir.path(), "1.1.0"));
    assert!(vendor_in_use(dir.path()));
    drop(first);
    assert!(version_in_use(dir.path(), "1.0.0"), "one lease is left");
    drop(second);
    assert!(!version_in_use(dir.path(), "1.0.0"));
    assert!(!vendor_in_use(dir.path()));
}

#[test]
fn the_use_of_one_vault_says_nothing_about_another() {
    let (one, other) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let _lease = lease(one.path(), "1.0.0");
    assert!(vendor_in_use(one.path()));
    assert!(!vendor_in_use(other.path()));
    assert!(!version_in_use(other.path(), "1.0.0"));
}

#[test]
fn confirming_the_newest_version_deletes_the_older_ones() {
    let dir = TempDir::new().unwrap();
    install(dir.path(), "1.0.0");
    install(dir.path(), "1.1.0");
    let run = lease(dir.path(), "1.1.0");

    run.confirm().unwrap();

    assert!(dir.path().join("1.1.0").join(CONFIRMED_FILE).is_file());
    assert_eq!(versions(dir.path()), ["1.1.0"]);
}

#[test]
fn an_older_version_that_a_run_still_uses_stays_until_that_run_ends() {
    let dir = TempDir::new().unwrap();
    install(dir.path(), "1.0.0");
    install(dir.path(), "1.1.0");
    let old_run = lease(dir.path(), "1.0.0");
    let new_run = lease(dir.path(), "1.1.0");

    new_run.confirm().unwrap();
    assert_eq!(
        versions(dir.path()),
        ["1.0.0", "1.1.0"],
        "the old version stays while a run uses it"
    );

    drop(old_run);
    assert_eq!(
        versions(dir.path()),
        ["1.1.0"],
        "the end of the last run on it removes it"
    );
}

#[test]
fn a_run_on_the_newest_version_never_deletes_anything_when_it_ends() {
    let dir = TempDir::new().unwrap();
    install(dir.path(), "1.0.0");
    install(dir.path(), "1.1.0");
    // Not confirmed: the older one is the way back.
    drop(lease(dir.path(), "1.1.0"));
    assert_eq!(versions(dir.path()), ["1.0.0", "1.1.0"]);
}

#[test]
fn an_unconfirmed_newest_version_keeps_the_previous_one() {
    let dir = TempDir::new().unwrap();
    install(dir.path(), "1.0.0");
    install(dir.path(), "1.1.0");
    assert_eq!(prune(dir.path(), &claude(), HOST), 0);
    assert_eq!(versions(dir.path()), ["1.0.0", "1.1.0"]);
}

#[test]
fn confirming_again_changes_nothing_and_does_not_tell_anyone() {
    let dir = TempDir::new().unwrap();
    install(dir.path(), "1.1.0");
    let told = Arc::new(AtomicUsize::new(0));
    let counter = told.clone();
    let run = Lease::take(
        dir.path().to_path_buf(),
        "1.1.0".to_string(),
        claude(),
        HOST,
        Some(Arc::new(move |vendor: &str| {
            assert_eq!(vendor, "claude");
            counter.fetch_add(1, Ordering::SeqCst);
        })),
    );

    run.confirm().unwrap();
    run.confirm().unwrap();

    assert_eq!(told.load(Ordering::SeqCst), 1, "told once, at the change");
}

#[test]
fn a_pending_deletion_is_told_to_the_windows_when_the_last_run_ends() {
    let dir = TempDir::new().unwrap();
    install(dir.path(), "1.0.0");
    install(dir.path(), "1.1.0");
    let told = Arc::new(AtomicUsize::new(0));
    let counter = told.clone();
    let notify: super::ChangeNotifier = Arc::new(move |_: &str| {
        counter.fetch_add(1, Ordering::SeqCst);
    });
    let old_run = Lease::take(
        dir.path().to_path_buf(),
        "1.0.0".to_string(),
        claude(),
        HOST,
        Some(notify),
    );
    lease(dir.path(), "1.1.0").confirm().unwrap();

    drop(old_run);

    assert_eq!(versions(dir.path()), ["1.1.0"]);
    assert_eq!(told.load(Ordering::SeqCst), 1);
}

#[test]
fn confirming_a_version_that_is_gone_is_an_error_and_creates_nothing() {
    let dir = TempDir::new().unwrap();
    let run = lease(dir.path(), "1.0.0");
    assert!(run.confirm().is_err());
    assert!(!dir.path().join("1.0.0").exists());
}
