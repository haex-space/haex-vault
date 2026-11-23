//! What a vault has installed, read from the file tree (spec 094, T011, FR-007, FR-013).

use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use super::{
    confirm, older_remains, previous_version, prune_older, resolve, revert, scan, sweep_runs,
    sweep_staging, Found, InstallMarker, InstalledFile, Misfit, Resolution, RevertError,
    CONFIRMED_FILE, MARKER_FILE,
};
use crate::cli_tools::catalog::{Catalog, VendorDefinition};
use crate::cli_tools::platform::PlatformKey;

const HOST: Option<PlatformKey> = Some(PlatformKey::LinuxX64);

fn claude() -> VendorDefinition {
    Catalog::shipped().vendor("claude").unwrap().clone()
}

fn codex() -> VendorDefinition {
    Catalog::shipped().vendor("codex").unwrap().clone()
}

/// A complete version folder: marker plus a program of `size` bytes.
fn install(vendor_dir: &Path, vendor: &VendorDefinition, version: &str, size: usize) -> PathBuf {
    install_for(vendor_dir, vendor, version, PlatformKey::LinuxX64, size)
}

fn install_for(
    vendor_dir: &Path,
    vendor: &VendorDefinition,
    version: &str,
    platform: PlatformKey,
    size: usize,
) -> PathBuf {
    let dir = vendor_dir.join(version);
    let executable = dir.join(vendor.executable_for(platform).unwrap());
    fs::create_dir_all(executable.parent().unwrap()).unwrap();
    fs::write(&executable, vec![7u8; size]).unwrap();
    InstallMarker::new(
        &vendor.vendor,
        version,
        platform,
        &"a".repeat(64),
        1234,
        size as u64,
        vec![InstalledFile {
            path: vendor.executable_for(platform).unwrap().replace('\\', "/"),
            size: size as u64,
        }],
    )
    .write(&dir)
    .unwrap();
    dir
}

fn usable_versions(found: &[Found]) -> Vec<&str> {
    found
        .iter()
        .filter(|entry| matches!(entry, Found::Usable(_)))
        .map(Found::version)
        .collect()
}

#[test]
fn a_complete_installation_is_usable_with_its_program_path() {
    let dir = TempDir::new().unwrap();
    let vendor = claude();
    let version_dir = install(dir.path(), &vendor, "2.1.311", 40);
    let found = scan(dir.path(), &vendor, HOST);
    assert_eq!(found.len(), 1);
    let Found::Usable(installation) = &found[0] else {
        panic!("{found:?}")
    };
    assert_eq!(installation.version, "2.1.311");
    assert_eq!(installation.executable, version_dir.join("claude"));
    assert!(!installation.confirmed);
    assert_eq!(installation.marker.executable_size, 40);
    assert!(matches!(
        resolve(dir.path(), &vendor, HOST),
        Resolution::Usable(_)
    ));
}

#[test]
fn a_package_program_is_found_at_the_path_the_catalog_names() {
    let dir = TempDir::new().unwrap();
    let vendor = codex();
    let version_dir = install(dir.path(), &vendor, "0.169.0", 12);
    let Resolution::Usable(installation) = resolve(dir.path(), &vendor, HOST) else {
        panic!("codex not usable")
    };
    assert_eq!(
        installation.executable,
        version_dir.join("bin").join("codex")
    );
}

#[test]
fn a_confirmed_marker_file_is_reported() {
    let dir = TempDir::new().unwrap();
    let vendor = claude();
    let version_dir = install(dir.path(), &vendor, "2.1.311", 4);
    fs::write(version_dir.join(CONFIRMED_FILE), "").unwrap();
    let Resolution::Usable(installation) = resolve(dir.path(), &vendor, HOST) else {
        panic!("not usable")
    };
    assert!(installation.confirmed);
}

#[test]
fn nothing_installed_is_not_installed() {
    let dir = TempDir::new().unwrap();
    let vendor = claude();
    assert_eq!(resolve(dir.path(), &vendor, HOST), Resolution::NotInstalled);
    assert_eq!(
        resolve(&dir.path().join("absent"), &vendor, HOST),
        Resolution::NotInstalled
    );
}

#[test]
fn a_folder_without_a_marker_is_never_usable() {
    let dir = TempDir::new().unwrap();
    let vendor = claude();
    let version_dir = dir.path().join("2.1.311");
    fs::create_dir_all(&version_dir).unwrap();
    fs::write(version_dir.join("claude"), "program").unwrap();
    assert_eq!(
        resolve(dir.path(), &vendor, HOST),
        Resolution::NeedsReinstall(Misfit::Incomplete)
    );
}

#[test]
fn a_marker_that_does_not_match_its_folder_or_vendor_is_incomplete() {
    let vendor = claude();
    for (folder, marker_vendor, marker_version) in [
        ("2.1.311", "claude", "2.1.310"),
        ("2.1.311", "codex", "2.1.311"),
    ] {
        let dir = TempDir::new().unwrap();
        let version_dir = install(dir.path(), &vendor, folder, 4);
        InstallMarker::new(
            marker_vendor,
            marker_version,
            PlatformKey::LinuxX64,
            &"a".repeat(64),
            1,
            4,
            vec![InstalledFile {
                path: "claude".into(),
                size: 4,
            }],
        )
        .write(&version_dir)
        .unwrap();
        assert_eq!(
            resolve(dir.path(), &vendor, HOST),
            Resolution::NeedsReinstall(Misfit::Incomplete),
            "{marker_vendor} {marker_version}"
        );
    }
}

#[test]
fn an_unreadable_marker_or_another_schema_is_incomplete() {
    let dir = TempDir::new().unwrap();
    let vendor = claude();
    let version_dir = install(dir.path(), &vendor, "2.1.311", 4);
    fs::write(version_dir.join(MARKER_FILE), "{ not json").unwrap();
    assert_eq!(
        resolve(dir.path(), &vendor, HOST),
        Resolution::NeedsReinstall(Misfit::Incomplete)
    );
    let mut marker = InstallMarker::new(
        "claude",
        "2.1.311",
        PlatformKey::LinuxX64,
        &"a".repeat(64),
        1,
        4,
        vec![InstalledFile {
            path: "claude".into(),
            size: 4,
        }],
    );
    marker.schema = 2;
    marker.write(&version_dir).unwrap();
    assert_eq!(
        resolve(dir.path(), &vendor, HOST),
        Resolution::NeedsReinstall(Misfit::Incomplete)
    );
}

#[test]
fn an_installation_for_another_platform_needs_a_reinstall_and_is_not_usable() {
    let dir = TempDir::new().unwrap();
    let vendor = claude();
    install_for(dir.path(), &vendor, "2.1.311", PlatformKey::DarwinArm64, 8);
    assert_eq!(
        resolve(dir.path(), &vendor, HOST),
        Resolution::NeedsReinstall(Misfit::Platform)
    );
    let found = scan(dir.path(), &vendor, HOST);
    assert!(usable_versions(&found).is_empty());
    // The same files are fine on the platform they were installed for.
    assert!(matches!(
        resolve(dir.path(), &vendor, Some(PlatformKey::DarwinArm64)),
        Resolution::Usable(_)
    ));
    // A device no vendor releases for fits nothing.
    assert_eq!(
        resolve(dir.path(), &vendor, None),
        Resolution::NeedsReinstall(Misfit::Platform)
    );
}

#[test]
fn a_missing_program_or_one_of_another_size_is_incomplete() {
    let dir = TempDir::new().unwrap();
    let vendor = claude();
    let version_dir = install(dir.path(), &vendor, "2.1.311", 100);

    fs::write(version_dir.join("claude"), vec![7u8; 99]).unwrap();
    assert_eq!(
        resolve(dir.path(), &vendor, HOST),
        Resolution::NeedsReinstall(Misfit::Incomplete),
        "a program cut short"
    );
    fs::write(version_dir.join("claude"), vec![7u8; 101]).unwrap();
    assert_eq!(
        resolve(dir.path(), &vendor, HOST),
        Resolution::NeedsReinstall(Misfit::Incomplete),
        "a program of another size"
    );
    fs::remove_file(version_dir.join("claude")).unwrap();
    assert_eq!(
        resolve(dir.path(), &vendor, HOST),
        Resolution::NeedsReinstall(Misfit::Incomplete),
        "a missing program"
    );
    fs::create_dir(version_dir.join("claude")).unwrap();
    assert_eq!(
        resolve(dir.path(), &vendor, HOST),
        Resolution::NeedsReinstall(Misfit::Incomplete),
        "a folder where the program should be"
    );
}

#[test]
fn every_package_file_recorded_by_the_marker_must_exist_with_its_size() {
    let dir = TempDir::new().unwrap();
    let vendor = codex();
    let version_dir = install(dir.path(), &vendor, "0.169.0", 12);
    let mut marker = InstallMarker::read(&version_dir).unwrap();
    marker.files.push(InstalledFile {
        path: "lib/runtime.so".into(),
        size: 8,
    });
    marker.write(&version_dir).unwrap();
    assert_eq!(
        resolve(dir.path(), &vendor, HOST),
        Resolution::NeedsReinstall(Misfit::Incomplete),
        "a package with a missing library is incomplete"
    );

    fs::create_dir_all(version_dir.join("lib")).unwrap();
    fs::write(version_dir.join("lib/runtime.so"), b"short").unwrap();
    assert_eq!(
        resolve(dir.path(), &vendor, HOST),
        Resolution::NeedsReinstall(Misfit::Incomplete),
        "a package file with a different size is incomplete"
    );
    fs::write(version_dir.join("lib/runtime.so"), b"12345678").unwrap();
    assert!(matches!(
        resolve(dir.path(), &vendor, HOST),
        Resolution::Usable(_)
    ));
}

#[test]
fn an_install_marker_cannot_name_paths_outside_its_version_folder() {
    let dir = TempDir::new().unwrap();
    let vendor = claude();
    let version_dir = install(dir.path(), &vendor, "2.1.311", 4);
    let mut marker = InstallMarker::read(&version_dir).unwrap();
    marker.files[0].path = "../outside".into();
    marker.write(&version_dir).unwrap();
    assert_eq!(
        resolve(dir.path(), &vendor, HOST),
        Resolution::NeedsReinstall(Misfit::Incomplete)
    );
}

#[test]
fn the_highest_usable_version_is_in_use_and_a_broken_newer_one_is_skipped() {
    let dir = TempDir::new().unwrap();
    let vendor = claude();
    install(dir.path(), &vendor, "2.1.9", 4);
    install(dir.path(), &vendor, "2.1.10", 4);
    let broken = install(dir.path(), &vendor, "2.2.0", 4);
    fs::remove_file(broken.join("claude")).unwrap();

    let found = scan(dir.path(), &vendor, HOST);
    let order: Vec<_> = found.iter().map(Found::version).collect();
    // Numeric order: 2.1.10 is newer than 2.1.9.
    assert_eq!(order, ["2.2.0", "2.1.10", "2.1.9"]);
    assert_eq!(usable_versions(&found), ["2.1.10", "2.1.9"]);
    let Resolution::Usable(installation) = resolve(dir.path(), &vendor, HOST) else {
        panic!("none usable")
    };
    assert_eq!(installation.version, "2.1.10");
}

#[test]
fn staging_folders_the_lock_and_strays_are_not_versions() {
    let dir = TempDir::new().unwrap();
    let vendor = claude();
    install(dir.path(), &vendor, "2.1.311", 4);
    fs::create_dir(dir.path().join(".2.2.0.0123.staging")).unwrap();
    fs::write(dir.path().join(".install.lock"), "").unwrap();
    fs::create_dir(dir.path().join("notes")).unwrap();
    fs::write(dir.path().join("3.0.0"), "a file, not a folder").unwrap();
    let found = scan(dir.path(), &vendor, HOST);
    assert_eq!(
        found.iter().map(Found::version).collect::<Vec<_>>(),
        ["2.1.311"]
    );
}

#[test]
fn the_staging_sweep_removes_interrupted_attempts_of_every_vendor_and_nothing_else() {
    let vault = TempDir::new().unwrap();
    let tools = vault.path().join("cli-tools");
    let claude_dir = tools.join("claude");
    let codex_dir = tools.join("codex");
    install(&claude_dir, &claude(), "2.1.311", 4);
    install(&codex_dir, &codex(), "0.169.0", 4);
    for staging in [
        claude_dir.join(".2.1.312.aaaa.staging"),
        codex_dir.join(".0.170.0.bbbb.staging"),
    ] {
        fs::create_dir_all(staging.join("deep")).unwrap();
        fs::write(staging.join("deep").join("x.part"), "half").unwrap();
    }
    fs::write(claude_dir.join(".install.lock"), "").unwrap();

    assert_eq!(sweep_staging(vault.path(), &tools), 2);

    assert!(!claude_dir.join(".2.1.312.aaaa.staging").exists());
    assert!(!codex_dir.join(".0.170.0.bbbb.staging").exists());
    assert!(claude_dir.join("2.1.311").join(MARKER_FILE).is_file());
    assert!(codex_dir.join("0.169.0").join(MARKER_FILE).is_file());
    assert!(claude_dir.join(".install.lock").is_file());
    assert_eq!(sweep_staging(vault.path(), &tools), 0, "idempotent");
    assert_eq!(sweep_staging(vault.path(), &vault.path().join("absent")), 0);
}

#[test]
fn the_staging_sweep_restores_a_backup_when_its_version_folder_is_missing() {
    let vault = TempDir::new().unwrap();
    let tools = vault.path().join("cli-tools");
    let claude_dir = tools.join("claude");
    let backup = claude_dir.join(".2.1.311.0123456789abcdef.backup");
    fs::create_dir_all(&backup).unwrap();
    fs::write(backup.join("kept"), "previous installation").unwrap();

    assert_eq!(sweep_staging(vault.path(), &tools), 1);

    assert_eq!(
        fs::read(claude_dir.join("2.1.311/kept")).unwrap(),
        b"previous installation"
    );
    assert!(!backup.exists());
}

#[test]
fn the_run_sweep_empties_the_run_root_and_keeps_it() {
    let dir = TempDir::new().unwrap();
    let run_root = dir.path().join("run");
    fs::create_dir_all(run_root.join("0a1b2c3d").join("tmp")).unwrap();
    fs::write(run_root.join("0a1b2c3d").join("auth.json"), "{}").unwrap();
    fs::create_dir_all(run_root.join("ffffffff")).unwrap();
    fs::write(run_root.join("stray"), "x").unwrap();

    assert_eq!(sweep_runs(dir.path(), &run_root), 3);

    assert!(run_root.is_dir());
    assert_eq!(fs::read_dir(&run_root).unwrap().count(), 0);
    assert_eq!(sweep_runs(dir.path(), &run_root), 0);
    assert_eq!(sweep_runs(dir.path(), &dir.path().join("absent")), 0);
}

#[cfg(unix)]
#[test]
fn cleanup_does_not_follow_symlinked_run_or_staging_ancestors() {
    use std::os::unix::fs::symlink;

    let vault = TempDir::new().unwrap();
    let outside_runs = TempDir::new().unwrap();
    fs::write(outside_runs.path().join("keep.txt"), "other vault").unwrap();
    let home = vault.path().join("home");
    symlink(outside_runs.path(), &home).unwrap();
    assert_eq!(sweep_runs(vault.path(), &home.join("run")), 0);
    assert_eq!(
        fs::read(outside_runs.path().join("keep.txt")).unwrap(),
        b"other vault"
    );

    let outside_tools = TempDir::new().unwrap();
    let linked_vendor = outside_tools.path().join("claude");
    let staging = linked_vendor.join(".2.1.312.deadbeef.staging");
    fs::create_dir_all(&staging).unwrap();
    fs::write(staging.join("keep.txt"), "other vault").unwrap();
    let tools = vault.path().join("cli-tools");
    fs::create_dir(&tools).unwrap();
    symlink(&linked_vendor, tools.join("claude")).unwrap();
    assert_eq!(sweep_staging(vault.path(), &tools), 0);
    assert_eq!(fs::read(staging.join("keep.txt")).unwrap(), b"other vault");

    let other_vault = TempDir::new().unwrap();
    symlink(outside_tools.path(), other_vault.path().join("cli-tools")).unwrap();
    assert_eq!(
        sweep_staging(other_vault.path(), &other_vault.path().join("cli-tools")),
        0
    );
    assert_eq!(fs::read(staging.join("keep.txt")).unwrap(), b"other vault");
}

fn confirmed(version_dir: &Path) {
    fs::write(version_dir.join(CONFIRMED_FILE), "").unwrap();
}

fn folders(vendor_dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(vendor_dir)
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn confirming_creates_the_marker_once_and_says_whether_it_was_new() {
    let dir = TempDir::new().unwrap();
    let vendor = claude();
    let version_dir = install(dir.path(), &vendor, "2.1.311", 4);

    assert!(confirm(dir.path(), "2.1.311").unwrap());
    assert!(version_dir.join(CONFIRMED_FILE).is_file());
    assert!(!confirm(dir.path(), "2.1.311").unwrap(), "already there");
    assert!(confirm(dir.path(), "9.9.9").is_err(), "no such version");
    assert!(!dir.path().join("9.9.9").exists());
}

#[test]
fn pruning_after_a_confirmation_deletes_every_older_folder_and_keeps_the_newest() {
    let dir = TempDir::new().unwrap();
    let vendor = claude();
    install(dir.path(), &vendor, "2.1.309", 4);
    install(dir.path(), &vendor, "2.1.310", 4);
    let newest = install(dir.path(), &vendor, "2.1.311", 4);
    // An older folder that is not complete goes as well.
    fs::create_dir_all(dir.path().join("2.1.308")).unwrap();
    confirmed(&newest);

    assert_eq!(prune_older(dir.path(), &vendor, HOST, &|_| false), 3);

    assert_eq!(folders(dir.path()), ["2.1.311"]);
    assert!(matches!(
        resolve(dir.path(), &vendor, HOST),
        Resolution::Usable(_)
    ));
}

#[test]
fn an_unconfirmed_newest_version_removes_nothing() {
    let dir = TempDir::new().unwrap();
    let vendor = claude();
    install(dir.path(), &vendor, "2.1.310", 4);
    install(dir.path(), &vendor, "2.1.311", 4);

    assert_eq!(prune_older(dir.path(), &vendor, HOST, &|_| false), 0);

    assert_eq!(folders(dir.path()), ["2.1.310", "2.1.311"]);
}

#[test]
fn an_older_version_a_run_uses_is_kept_and_removed_by_the_next_prune() {
    let dir = TempDir::new().unwrap();
    let vendor = claude();
    install(dir.path(), &vendor, "2.1.310", 4);
    let newest = install(dir.path(), &vendor, "2.1.311", 4);
    confirmed(&newest);

    assert_eq!(
        prune_older(dir.path(), &vendor, HOST, &|version| version == "2.1.310"),
        0
    );
    assert_eq!(folders(dir.path()), ["2.1.310", "2.1.311"]);
    assert!(older_remains(&scan(dir.path(), &vendor, HOST)));

    assert_eq!(prune_older(dir.path(), &vendor, HOST, &|_| false), 1);
    assert_eq!(folders(dir.path()), ["2.1.311"]);
    assert!(!older_remains(&scan(dir.path(), &vendor, HOST)));
}

#[test]
fn a_newer_folder_that_is_broken_is_not_the_version_in_use_and_is_left_alone() {
    let dir = TempDir::new().unwrap();
    let vendor = claude();
    install(dir.path(), &vendor, "2.1.309", 4);
    let usable = install(dir.path(), &vendor, "2.1.310", 4);
    fs::create_dir_all(dir.path().join("2.1.311")).unwrap();
    confirmed(&usable);

    prune_older(dir.path(), &vendor, HOST, &|_| false);

    // The broken newer folder is not touched; the confirmed 2.1.310 replaced 2.1.309.
    assert_eq!(folders(dir.path()), ["2.1.310", "2.1.311"]);
}

#[cfg(unix)]
#[test]
fn a_failed_deletion_is_repeated_by_the_next_prune() {
    use std::os::unix::fs::PermissionsExt;

    // SAFETY: `geteuid` takes no arguments and cannot fail.
    if unsafe { libc::geteuid() } == 0 {
        // A superuser deletes a folder without write permission anyway: nothing to observe.
        return;
    }
    let dir = TempDir::new().unwrap();
    let vendor = claude();
    let old = install(dir.path(), &vendor, "2.1.310", 4);
    let newest = install(dir.path(), &vendor, "2.1.311", 4);
    confirmed(&newest);
    // The old folder holds a folder that cannot be emptied.
    let locked = old.join("locked");
    fs::create_dir(&locked).unwrap();
    fs::write(locked.join("file"), "x").unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o500)).unwrap();

    assert_eq!(prune_older(dir.path(), &vendor, HOST, &|_| false), 0);
    assert!(old.is_dir(), "the failed deletion leaves the folder");

    fs::set_permissions(&locked, fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(prune_older(dir.path(), &vendor, HOST, &|_| false), 1);
    assert_eq!(folders(dir.path()), ["2.1.311"]);
}

#[test]
fn reverting_deletes_the_unconfirmed_newest_and_leaves_the_older_one_in_use() {
    let dir = TempDir::new().unwrap();
    let vendor = claude();
    let old = install(dir.path(), &vendor, "2.1.310", 4);
    confirmed(&old);
    install(dir.path(), &vendor, "2.1.311", 4);
    assert_eq!(
        previous_version(&scan(dir.path(), &vendor, HOST)).as_deref(),
        Some("2.1.310")
    );

    assert_eq!(revert(dir.path(), &vendor, HOST).as_deref(), Ok("2.1.310"));

    assert_eq!(folders(dir.path()), ["2.1.310"]);
    let Resolution::Usable(installation) = resolve(dir.path(), &vendor, HOST) else {
        panic!("not usable")
    };
    assert_eq!(installation.version, "2.1.310");
}

#[test]
fn there_is_nothing_to_revert_when_the_newest_is_confirmed_or_alone() {
    let dir = TempDir::new().unwrap();
    let vendor = claude();
    // Nothing installed.
    assert_eq!(
        revert(dir.path(), &vendor, HOST),
        Err(RevertError::NothingToRevert)
    );
    // One version, unconfirmed: no way back.
    install(dir.path(), &vendor, "2.1.310", 4);
    assert_eq!(
        revert(dir.path(), &vendor, HOST),
        Err(RevertError::NothingToRevert)
    );
    assert!(previous_version(&scan(dir.path(), &vendor, HOST)).is_none());
    // Two versions, the newest confirmed: nothing to go back from.
    let newest = install(dir.path(), &vendor, "2.1.311", 4);
    confirmed(&newest);
    assert_eq!(
        revert(dir.path(), &vendor, HOST),
        Err(RevertError::NothingToRevert)
    );
    assert!(previous_version(&scan(dir.path(), &vendor, HOST)).is_none());
    assert_eq!(folders(dir.path()), ["2.1.310", "2.1.311"]);
}

#[test]
fn a_damaged_older_version_is_no_way_back() {
    let dir = TempDir::new().unwrap();
    let vendor = claude();
    fs::create_dir_all(dir.path().join("2.1.310")).unwrap();
    install(dir.path(), &vendor, "2.1.311", 4);
    assert_eq!(
        revert(dir.path(), &vendor, HOST),
        Err(RevertError::NothingToRevert)
    );
    assert_eq!(folders(dir.path()), ["2.1.310", "2.1.311"]);
}

// The variant of a platform (spec 096): what was installed for it fits the host that falls back.

fn abacus() -> VendorDefinition {
    Catalog::shipped().vendor("abacusai").unwrap().clone()
}

#[test]
fn an_installation_for_the_variant_fits_a_host_whose_candidates_include_it() {
    let dir = TempDir::new().unwrap();
    let vendor = abacus();
    install_for(
        dir.path(),
        &vendor,
        "2.10.23",
        PlatformKey::LinuxX64Baseline,
        40,
    );

    let found = scan(dir.path(), &vendor, Some(PlatformKey::LinuxX64Baseline));

    assert_eq!(usable_versions(&found), ["2.10.23"]);
}

#[test]
fn an_installation_for_the_variant_does_not_fit_a_host_that_has_avx2() {
    let dir = TempDir::new().unwrap();
    let vendor = abacus();
    install_for(
        dir.path(),
        &vendor,
        "2.10.23",
        PlatformKey::LinuxX64Baseline,
        40,
    );

    let found = scan(dir.path(), &vendor, HOST);

    assert_eq!(
        resolve(dir.path(), &vendor, HOST),
        Resolution::NeedsReinstall(Misfit::Platform)
    );
    assert!(usable_versions(&found).is_empty());
}

#[test]
fn an_installation_for_the_base_key_fits_a_host_without_avx2() {
    let dir = TempDir::new().unwrap();
    let vendor = claude();
    install(dir.path(), &vendor, "2.1.311", 40);

    let found = scan(dir.path(), &vendor, Some(PlatformKey::LinuxX64Baseline));

    assert_eq!(usable_versions(&found), ["2.1.311"]);
}

#[test]
fn an_avx2_only_base_install_does_not_fit_a_host_without_avx2() {
    let dir = TempDir::new().unwrap();
    let vendor = Catalog::parse(
        r#"{"schema":1,"vendors":[{"vendor":"abacusai","display_name":"Abacus AI","download_hosts":["static.abacus.ai"],
        "executable":{"linux-x64":"abacusai","linux-x64-baseline":"abacusai"},"config_env":"ABACUSAI_HOME",
        "versions":[{"version":"2.10.23","platforms":{
        "linux-x64":{"url":"https://static.abacus.ai/x64.tgz","sha256":"0000000000000000000000000000000000000000000000000000000000000001","size":10,"kind":"tar.gz","unpacked_size":20,"requires_avx2":true},
        "linux-x64-baseline":{"url":"https://static.abacus.ai/base.tgz","sha256":"0000000000000000000000000000000000000000000000000000000000000002","size":11,"kind":"tar.gz","unpacked_size":21}}}]}]}"#,
    )
    .unwrap()
    .vendor("abacusai")
    .unwrap()
    .clone();
    let version = vendor
        .versions
        .iter()
        .find(|entry| entry.version == "2.10.23")
        .unwrap();
    assert!(version.platforms["linux-x64"].requires_avx2);
    assert!(version.platforms.contains_key("linux-x64-baseline"));
    install_for(dir.path(), &vendor, "2.10.23", PlatformKey::LinuxX64, 40);

    let found = scan(dir.path(), &vendor, Some(PlatformKey::LinuxX64Baseline));

    assert!(usable_versions(&found).is_empty());
    assert_eq!(
        resolve(dir.path(), &vendor, Some(PlatformKey::LinuxX64Baseline)),
        Resolution::NeedsReinstall(Misfit::Platform)
    );
}

#[test]
fn an_installation_for_another_platform_family_still_needs_a_reinstall_on_the_variant_host() {
    let dir = TempDir::new().unwrap();
    let vendor = claude();
    install_for(dir.path(), &vendor, "2.1.311", PlatformKey::LinuxArm64, 40);

    assert_eq!(
        resolve(dir.path(), &vendor, Some(PlatformKey::LinuxX64Baseline)),
        Resolution::NeedsReinstall(Misfit::Platform)
    );
}

#[test]
fn the_signature_file_next_to_the_program_does_not_break_the_completeness_check() {
    let dir = TempDir::new().unwrap();
    let vendor = abacus();
    let version_dir = install(dir.path(), &vendor, "2.10.23", 40);
    // The release archive carries the Sigstore bundle beside the program; the marker lists it.
    fs::write(version_dir.join("abacusai-linux-x64.sigstore"), b"bundle").unwrap();
    let mut marker = InstallMarker::read(&version_dir).unwrap();
    marker.files.push(InstalledFile {
        path: "abacusai-linux-x64.sigstore".into(),
        size: 6,
    });
    marker.write(&version_dir).unwrap();

    let found = scan(dir.path(), &vendor, HOST);

    assert_eq!(usable_versions(&found), ["2.10.23"]);
}
