//! The run folder and the runtime handed to a delegate (spec 094, T010, FR-011).

use std::fs;

use tempfile::TempDir;

use super::{DelegateRuntime, RunFolder};

fn dirs() -> (TempDir, std::path::PathBuf, std::path::PathBuf) {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("home");
    let run_root = home.join("run");
    (dir, home, run_root)
}

#[test]
fn a_run_folder_lies_below_the_run_root_with_a_short_name_and_its_own_temp_folder() {
    let (_dir, home, run_root) = dirs();
    let run = RunFolder::create(&run_root, &home).unwrap();
    assert_eq!(run.path().parent(), Some(run_root.as_path()));
    let name = run
        .path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    assert_eq!(name.len(), 8);
    assert!(name.chars().all(|c| c.is_ascii_hexdigit()), "{name}");
    assert!(run.tmp().is_dir());
    assert!(run.tmp().starts_with(run.path()));
    assert!(run.runtime().is_dir());
    assert!(run.runtime().starts_with(run.path()));
    assert_eq!(run.home(), home);
}

#[cfg(unix)]
#[test]
fn the_runtime_dir_is_private_to_its_user() {
    use std::os::unix::fs::PermissionsExt;
    let (_dir, home, run_root) = dirs();
    let run = RunFolder::create(&run_root, &home).unwrap();
    let mode = fs::metadata(run.runtime()).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o700);
}

#[cfg(unix)]
#[test]
fn the_run_folder_is_private_before_sensitive_files_are_created() {
    use std::os::unix::fs::PermissionsExt;
    let (_dir, home, run_root) = dirs();
    let run = RunFolder::create(&run_root, &home).unwrap();
    let mode = fs::metadata(run.path()).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o700);
}

#[test]
fn a_run_folder_is_removed_with_everything_in_it_when_it_is_dropped() {
    let (_dir, home, run_root) = dirs();
    let run = RunFolder::create(&run_root, &home).unwrap();
    let path = run.path().to_path_buf();
    fs::write(path.join("auth.json"), "{}").unwrap();
    fs::write(run.tmp().join("scratch"), "x").unwrap();
    assert!(path.exists());
    drop(run);
    assert!(!path.exists());
    // The run root and the home stay.
    assert!(run_root.is_dir());
}

#[test]
fn two_runs_never_share_a_folder() {
    let (_dir, home, run_root) = dirs();
    let runs: Vec<RunFolder> = (0..16)
        .map(|_| RunFolder::create(&run_root, &home).unwrap())
        .collect();
    let mut paths: Vec<_> = runs.iter().map(|run| run.path().to_path_buf()).collect();
    paths.sort();
    paths.dedup();
    assert_eq!(paths.len(), 16);
}

#[test]
fn the_stub_runtime_keeps_everything_below_its_directory() {
    let dir = TempDir::new().unwrap();
    let runtime = DelegateRuntime::for_stub("claude", dir.path().join("stub.sh"), dir.path());
    assert_eq!(runtime.vendor.vendor, "claude");
    assert_eq!(runtime.executable, dir.path().join("stub.sh"));
    assert!(runtime.home.starts_with(dir.path()));
    assert_eq!(runtime.run_root, runtime.home.join("run"));
    let run = runtime.run_folder().unwrap();
    assert!(run.path().starts_with(&runtime.run_root));
    assert!(runtime.confirm().is_ok());
}

#[test]
#[should_panic(expected = "not in the catalog")]
fn a_stub_for_an_unknown_vendor_is_a_test_bug() {
    let dir = TempDir::new().unwrap();
    let _ = DelegateRuntime::for_stub("nobody", dir.path().join("x"), dir.path());
}

#[test]
fn the_approval_socket_lies_in_the_run_folder_when_its_path_is_short_enough() {
    let (_dir, home, run_root) = dirs();
    let mut run = RunFolder::create(&run_root, &home).unwrap();
    let socket = run.approval_socket();
    assert_eq!(socket, run.path().join("approval.sock"));
}

#[cfg(unix)]
#[test]
fn a_vault_path_too_long_for_a_socket_falls_back_to_the_temp_folder_and_cleans_up() {
    let dir = TempDir::new().unwrap();
    // A vault name of 64 characters, as a long vault name would be, below a deep data folder.
    let home = dir
        .path()
        .join("a-deep-data-folder-of-some-user")
        .join("vaults")
        .join("v".repeat(64))
        .join("home");
    let run_root = home.join("run");
    let mut run = RunFolder::create(&run_root, &home).unwrap();
    assert!(run.path().join("approval.sock").as_os_str().len() > 104);

    let socket = run.approval_socket();

    assert!(socket.as_os_str().len() <= 104, "{socket:?}");
    assert!(socket.starts_with(std::env::temp_dir()));
    let name = socket.file_name().unwrap().to_string_lossy().into_owned();
    assert!(
        name.starts_with("haex-") && name.ends_with(".sock"),
        "{name}"
    );
    assert!(
        !name.contains(&"v".repeat(8)),
        "the name carries nothing of the vault"
    );
    // The socket can really be bound there, and goes away with the run.
    let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
    assert!(socket.exists());
    drop(listener);
    drop(run);
    assert!(!socket.exists());
}
