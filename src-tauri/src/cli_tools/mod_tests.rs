//! The sweep where a vault opens (spec 094, T013, FR-007).

use std::fs;

use tempfile::TempDir;

use super::{open_sweep, SweepReport};
use crate::instances::vault_paths::VaultPaths;

#[test]
fn a_vault_with_leftovers_is_clean_after_the_sweep_and_its_installations_stay() {
    let data = TempDir::new().unwrap();
    let paths = VaultPaths::new(data.path());
    let name = "anna";

    let claude = paths.cli_tool_dir(name, "claude");
    let installed = claude.join("2.1.311");
    fs::create_dir_all(&installed).unwrap();
    fs::write(installed.join("install.json"), "{}").unwrap();
    let staging = claude.join(".2.1.312.0a1b2c3d.staging");
    fs::create_dir_all(&staging).unwrap();
    fs::write(staging.join("claude.part"), "half a download").unwrap();

    let run = paths.run_root(name).join("0a1b2c3d");
    fs::create_dir_all(run.join("tmp")).unwrap();
    fs::write(run.join("auth.json"), "{}").unwrap();

    let records = paths.records_dir(name);
    fs::create_dir_all(&records).unwrap();
    fs::write(records.join("tools.log"), "1 install_finished\n").unwrap();

    let report = open_sweep(&paths, name);

    assert_eq!(
        report,
        SweepReport {
            staging: 1,
            runs: 1,
            pruned: 0
        }
    );
    assert!(!staging.exists());
    assert!(!run.exists());
    assert!(paths.run_root(name).is_dir(), "the run root stays");
    assert!(
        installed.join("install.json").is_file(),
        "an installation is not a leftover"
    );
    assert!(records.join("tools.log").is_file(), "the record stays");
    assert_eq!(open_sweep(&paths, name), SweepReport::default());
}

#[test]
fn a_vault_without_tool_folders_sweeps_nothing_and_creates_nothing() {
    let data = TempDir::new().unwrap();
    let paths = VaultPaths::new(data.path());
    assert_eq!(open_sweep(&paths, "anna"), SweepReport::default());
    assert!(!paths.folder("anna").exists());
}

#[test]
fn the_sweep_touches_only_the_opened_vault() {
    let data = TempDir::new().unwrap();
    let paths = VaultPaths::new(data.path());
    let other_run = paths.run_root("bob").join("deadbeef");
    fs::create_dir_all(&other_run).unwrap();
    let other_staging = paths.cli_tool_dir("bob", "codex").join(".0.1.0.x.staging");
    fs::create_dir_all(&other_staging).unwrap();

    assert_eq!(open_sweep(&paths, "anna"), SweepReport::default());
    assert!(other_run.is_dir());
    assert!(other_staging.is_dir());
}
