//! The status of a tool in a vault, derived from files and the catalog alone (spec 094, T016).

use std::fs;

use tempfile::TempDir;

use super::{derive, CliToolState};
use crate::cli_tools::catalog::{Catalog, VendorDefinition};
use crate::cli_tools::containment::IsolationLevel;
use crate::cli_tools::installed::InstallMarker;
use crate::cli_tools::platform::PlatformKey;

const HOST: Option<PlatformKey> = Some(PlatformKey::LinuxX64);

fn claude() -> VendorDefinition {
    Catalog::shipped().vendor("claude").unwrap().clone()
}

fn status(
    dir: &std::path::Path,
    host: Option<PlatformKey>,
    installing: bool,
) -> super::CliToolStatus {
    derive(&claude(), dir, host, installing, IsolationLevel::BestEffort)
}

fn install(dir: &std::path::Path, version: &str, platform: PlatformKey) -> std::path::PathBuf {
    let version_dir = dir.join(version);
    fs::create_dir_all(&version_dir).unwrap();
    fs::write(version_dir.join("claude"), b"program").unwrap();
    InstallMarker::new("claude", version, platform, &"a".repeat(64), 1, 7)
        .write(&version_dir)
        .unwrap();
    version_dir
}

#[test]
fn nothing_installed_offers_the_install_with_its_size_and_the_newest_listed_version() {
    let dir = TempDir::new().unwrap();
    let shown = status(dir.path(), HOST, false);
    let listed = &claude().versions[0];
    assert_eq!(shown.state, CliToolState::NotInstalled);
    assert_eq!(shown.vendor, "claude");
    assert_eq!(shown.display_name, "Claude Code");
    assert_eq!(shown.installed_version, None);
    assert_eq!(
        shown.latest_listed_version.as_deref(),
        Some(listed.version.as_str())
    );
    let artifact = &listed.platforms["linux-x64"];
    assert_eq!(shown.download_bytes, Some(artifact.size));
    assert_eq!(shown.unpacked_bytes, Some(artifact.size));
    assert_eq!(shown.isolation, IsolationLevel::BestEffort);
    assert_eq!(shown.reason, None);
}

#[test]
fn a_package_names_its_unpacked_size_for_the_space_it_takes() {
    let dir = TempDir::new().unwrap();
    let codex = Catalog::shipped().vendor("codex").unwrap().clone();
    let shown = derive(&codex, dir.path(), HOST, false, IsolationLevel::BestEffort);
    let artifact = &codex.versions[0].platforms["linux-x64"];
    assert_eq!(shown.download_bytes, Some(artifact.size));
    assert_eq!(shown.unpacked_bytes, artifact.unpacked_size);
    assert!(shown.unpacked_bytes > shown.download_bytes);
}

#[test]
fn a_complete_installation_is_installed_with_its_version() {
    let dir = TempDir::new().unwrap();
    install(dir.path(), "2.1.311", PlatformKey::LinuxX64);
    let shown = status(dir.path(), HOST, false);
    assert_eq!(shown.state, CliToolState::Installed);
    assert_eq!(shown.installed_version.as_deref(), Some("2.1.311"));
}

#[test]
fn a_running_install_wins_over_the_state_on_disk() {
    let dir = TempDir::new().unwrap();
    assert_eq!(
        status(dir.path(), HOST, true).state,
        CliToolState::Installing
    );
    install(dir.path(), "2.1.311", PlatformKey::LinuxX64);
    assert_eq!(
        status(dir.path(), HOST, true).state,
        CliToolState::Installing
    );
}

#[test]
fn a_damaged_or_foreign_installation_is_a_state_not_an_error() {
    let dir = TempDir::new().unwrap();
    let foreign = install(dir.path(), "2.1.311", PlatformKey::DarwinArm64);
    let shown = status(dir.path(), HOST, false);
    assert_eq!(shown.state, CliToolState::NeedsReinstall);
    assert_eq!(shown.reason.as_deref(), Some("platform"));
    assert_eq!(shown.installed_version, None);

    fs::remove_dir_all(&foreign).unwrap();
    let broken = install(dir.path(), "2.1.311", PlatformKey::LinuxX64);
    fs::remove_file(broken.join("claude")).unwrap();
    let shown = status(dir.path(), HOST, false);
    assert_eq!(shown.state, CliToolState::NeedsReinstall);
    assert_eq!(shown.reason.as_deref(), Some("incomplete"));
}

#[test]
fn a_device_no_program_is_listed_for_is_unsupported_and_offers_nothing() {
    let dir = TempDir::new().unwrap();
    let shown = status(dir.path(), None, false);
    assert_eq!(shown.state, CliToolState::Unsupported);
    assert_eq!(shown.latest_listed_version, None);
    assert_eq!(shown.download_bytes, None);

    // A vendor that lists nothing for this platform: only a Windows program.
    let catalog = Catalog::parse(
        r#"{"schema":1,"vendors":[{"vendor":"winonly","display_name":"Win","download_hosts":["dl.example.com"],
        "executable":{"win32-x64":"w.exe"},"config_env":"W_HOME","versions":[{"version":"1.0.0","platforms":
        {"win32-x64":{"url":"https://dl.example.com/w.exe","sha256":"0000000000000000000000000000000000000000000000000000000000000001",
        "size":10,"kind":"file"}}}]}]}"#,
    )
    .unwrap();
    let winonly = catalog.vendor("winonly").unwrap();
    let shown = derive(winonly, dir.path(), HOST, false, IsolationLevel::BestEffort);
    assert_eq!(shown.state, CliToolState::Unsupported);
}

#[test]
fn deriving_the_status_reads_only_the_given_folder() {
    // The folder does not exist: nothing is created, nothing fails.
    let dir = TempDir::new().unwrap();
    let absent = dir.path().join("cli-tools").join("claude");
    let shown = status(&absent, HOST, false);
    assert_eq!(shown.state, CliToolState::NotInstalled);
    assert!(!absent.exists());
}

#[test]
fn the_status_serialises_with_the_names_the_frontend_reads() {
    let dir = TempDir::new().unwrap();
    let json = serde_json::to_value(status(dir.path(), HOST, false)).unwrap();
    assert_eq!(json["state"], "not_installed");
    assert_eq!(json["displayName"], "Claude Code");
    assert!(json["downloadBytes"].is_u64());
    assert_eq!(json["isolation"], "best_effort");
    assert!(json["installedVersion"].is_null());
}
