//! The status of a tool in a vault, derived from files and the catalog alone (spec 094, T016).

use std::fs;

use tempfile::TempDir;

use super::{derive, CliToolState};
use crate::cli_tools::catalog::{Catalog, CatalogVersion, VendorDefinition};
use crate::cli_tools::containment::IsolationLevel;
use crate::cli_tools::installed::{InstallMarker, InstalledFile};
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
    derive(
        &claude(),
        dir,
        host,
        true,
        installing,
        IsolationLevel::BestEffort,
    )
}

fn install(dir: &std::path::Path, version: &str, platform: PlatformKey) -> std::path::PathBuf {
    let version_dir = dir.join(version);
    fs::create_dir_all(&version_dir).unwrap();
    fs::write(version_dir.join("claude"), b"program").unwrap();
    InstallMarker::new(
        "claude",
        version,
        platform,
        &"a".repeat(64),
        1,
        7,
        vec![InstalledFile {
            path: "claude".into(),
            size: 7,
        }],
    )
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
    let shown = derive(
        &codex,
        dir.path(),
        HOST,
        true,
        false,
        IsolationLevel::BestEffort,
    );
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
    let shown = derive(
        winonly,
        dir.path(),
        HOST,
        true,
        false,
        IsolationLevel::BestEffort,
    );
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

// What the row says about updates (spec 094 US3).

/// Claude with exactly `versions` listed for this platform, newest first.
fn listing(versions: &[&str]) -> VendorDefinition {
    let mut vendor = claude();
    let artifact = vendor.versions[0].platforms["linux-x64"].clone();
    vendor.versions = versions
        .iter()
        .map(|version| CatalogVersion {
            version: (*version).to_string(),
            platforms: [("linux-x64".to_string(), artifact.clone())].into(),
        })
        .collect();
    vendor
}

fn status_for(vendor: &VendorDefinition, dir: &std::path::Path) -> super::CliToolStatus {
    derive(vendor, dir, HOST, true, false, IsolationLevel::BestEffort)
}

#[test]
fn an_older_version_than_the_newest_listed_offers_the_update() {
    let dir = TempDir::new().unwrap();
    let vendor = listing(&["1.1.0", "1.0.0"]);
    install(dir.path(), "1.0.0", PlatformKey::LinuxX64);

    let shown = status_for(&vendor, dir.path());

    assert_eq!(shown.state, CliToolState::UpdateAvailable);
    assert_eq!(shown.installed_version.as_deref(), Some("1.0.0"));
    assert_eq!(shown.latest_listed_version.as_deref(), Some("1.1.0"));
    assert!(!shown.untested && !shown.outdated);

    install(dir.path(), "1.0.5", PlatformKey::LinuxX64);
    let shown = status_for(&vendor, dir.path());
    assert_eq!(shown.state, CliToolState::UpdateAvailable);
    assert!(
        !shown.untested,
        "an unlisted version below the newest catalog version is not untested"
    );
}

#[test]
fn the_newest_listed_version_has_no_update_to_offer() {
    let dir = TempDir::new().unwrap();
    let vendor = listing(&["1.1.0", "1.0.0"]);
    install(dir.path(), "1.1.0", PlatformKey::LinuxX64);

    let shown = status_for(&vendor, dir.path());

    assert_eq!(shown.state, CliToolState::Installed);
    assert!(!shown.untested && !shown.outdated);
}

#[test]
fn a_version_above_the_list_is_untested_and_still_in_use() {
    let dir = TempDir::new().unwrap();
    let vendor = listing(&["1.1.0", "1.0.0"]);
    install(dir.path(), "2.0.0", PlatformKey::LinuxX64);

    let shown = status_for(&vendor, dir.path());

    assert_eq!(shown.state, CliToolState::Installed, "no update from it");
    assert_eq!(shown.installed_version.as_deref(), Some("2.0.0"));
    assert!(shown.untested);
    assert!(!shown.outdated);
}

#[test]
fn a_version_below_the_oldest_listed_one_is_outdated_and_offers_the_update() {
    let dir = TempDir::new().unwrap();
    let vendor = listing(&["1.1.0", "1.0.0"]);
    install(dir.path(), "0.9.0", PlatformKey::LinuxX64);

    let shown = status_for(&vendor, dir.path());

    assert_eq!(shown.state, CliToolState::UpdateAvailable);
    assert!(shown.outdated);
    assert!(!shown.untested, "outdated says it already");
}

#[test]
fn an_unconfirmed_update_names_the_version_to_go_back_to_and_a_confirmed_one_does_not() {
    let dir = TempDir::new().unwrap();
    let vendor = listing(&["1.1.0", "1.0.0"]);
    let old = install(dir.path(), "1.0.0", PlatformKey::LinuxX64);
    fs::write(old.join(".confirmed"), "").unwrap();
    let new = install(dir.path(), "1.1.0", PlatformKey::LinuxX64);

    let shown = status_for(&vendor, dir.path());
    assert_eq!(shown.installed_version.as_deref(), Some("1.1.0"));
    assert!(!shown.confirmed);
    assert_eq!(shown.previous_version.as_deref(), Some("1.0.0"));

    fs::write(new.join(".confirmed"), "").unwrap();
    let shown = status_for(&vendor, dir.path());
    assert!(shown.confirmed);
    assert_eq!(shown.previous_version, None);
}

#[test]
fn a_single_unconfirmed_version_has_no_way_back() {
    let dir = TempDir::new().unwrap();
    let vendor = listing(&["1.0.0"]);
    install(dir.path(), "1.0.0", PlatformKey::LinuxX64);

    let shown = status_for(&vendor, dir.path());

    assert!(!shown.confirmed);
    assert_eq!(shown.previous_version, None);
}

#[test]
fn an_installing_tool_with_an_update_available_shows_as_installing() {
    let dir = TempDir::new().unwrap();
    let vendor = listing(&["1.1.0", "1.0.0"]);
    install(dir.path(), "1.0.0", PlatformKey::LinuxX64);

    let shown = derive(
        &vendor,
        dir.path(),
        HOST,
        true,
        true,
        IsolationLevel::BestEffort,
    );

    assert_eq!(shown.state, CliToolState::Installing);
    assert_eq!(shown.installed_version.as_deref(), Some("1.0.0"));
}

// The variant of a platform, the AVX2 rule and a vendor with no tested version (spec 096).

const AVX_VENDOR: &str = r#"{"schema":1,"vendors":[{"vendor":"avxtool","display_name":"Avx","download_hosts":["dl.example.com"],
    "executable":{"linux-x64":"t","linux-x64-baseline":"t","darwin-x64":"t","win32-x64":"t.exe"},"config_env":"T_HOME",
    "versions":[{"version":"1.0.0","platforms":{
    "linux-x64":{"url":"https://dl.example.com/x64.tgz","sha256":"0000000000000000000000000000000000000000000000000000000000000001","size":10,"kind":"tar.gz","unpacked_size":20,"requires_avx2":true},
    "linux-x64-baseline":{"url":"https://dl.example.com/base.tgz","sha256":"0000000000000000000000000000000000000000000000000000000000000002","size":11,"kind":"tar.gz","unpacked_size":21},
    "darwin-x64":{"url":"https://dl.example.com/mac.tgz","sha256":"0000000000000000000000000000000000000000000000000000000000000003","size":12,"kind":"tar.gz","unpacked_size":22,"requires_avx2":true},
    "win32-x64":{"url":"https://dl.example.com/win.zip","sha256":"0000000000000000000000000000000000000000000000000000000000000004","size":13,"kind":"zip","unpacked_size":23,"requires_avx2":true}}}]}]}"#;

fn avx_vendor() -> VendorDefinition {
    Catalog::parse(AVX_VENDOR)
        .unwrap()
        .vendor("avxtool")
        .unwrap()
        .clone()
}

fn avx_status(host: PlatformKey, avx2: bool) -> super::CliToolStatus {
    let dir = TempDir::new().unwrap();
    derive(
        &avx_vendor(),
        dir.path(),
        Some(host),
        avx2,
        false,
        IsolationLevel::BestEffort,
    )
}

#[test]
fn a_linux_host_without_avx2_is_offered_the_variant() {
    let shown = avx_status(PlatformKey::LinuxX64Baseline, false);
    assert_eq!(shown.state, CliToolState::NotInstalled);
    assert_eq!(shown.download_bytes, Some(11));
    assert_eq!(shown.unpacked_bytes, Some(21));

    let shown = avx_status(PlatformKey::LinuxX64, true);
    assert_eq!(shown.download_bytes, Some(10));
}

#[test]
fn a_macos_or_windows_x64_host_without_avx2_reads_unsupported_where_the_program_needs_it() {
    for host in [PlatformKey::DarwinX64, PlatformKey::Win32X64] {
        let shown = avx_status(host, false);
        assert_eq!(shown.state, CliToolState::Unsupported, "{host}");
        assert_eq!(shown.download_bytes, None, "{host}");
        let shown = avx_status(host, true);
        assert_eq!(shown.state, CliToolState::NotInstalled, "{host}");
    }
    // A zip's size on disk is the unpacked size, like a tar.gz.
    assert_eq!(
        avx_status(PlatformKey::Win32X64, true).unpacked_bytes,
        Some(23)
    );
}

#[test]
fn claude_code_stays_installable_on_a_host_without_avx2() {
    let dir = TempDir::new().unwrap();
    for host in [PlatformKey::LinuxX64Baseline, PlatformKey::DarwinX64] {
        let shown = derive(
            &claude(),
            dir.path(),
            Some(host),
            false,
            false,
            IsolationLevel::BestEffort,
        );
        assert_eq!(shown.state, CliToolState::NotInstalled, "{host}");
        assert!(shown.download_bytes.is_some(), "{host}");
    }
}

#[test]
fn a_vendor_with_no_tested_version_reads_no_tested_version_whatever_the_host() {
    let catalog = Catalog::parse(
        r#"{"schema":1,"vendors":[{"vendor":"newtool","display_name":"New","download_hosts":["dl.example.com"],
        "executable":{"linux-x64":"t"},"config_env":"T_HOME","versions":[]}]}"#,
    )
    .unwrap();
    let vendor = catalog.vendor("newtool").unwrap();
    let dir = TempDir::new().unwrap();
    for host in [HOST, None] {
        let shown = derive(
            vendor,
            dir.path(),
            host,
            true,
            false,
            IsolationLevel::BestEffort,
        );
        assert_eq!(shown.state, CliToolState::NoTestedVersion);
        assert_eq!(shown.latest_listed_version, None);
        assert_eq!(shown.download_bytes, None);
    }
}

#[test]
fn an_installation_for_the_base_key_is_in_use_on_a_host_without_avx2() {
    let dir = TempDir::new().unwrap();
    install(dir.path(), "2.1.311", PlatformKey::LinuxX64);

    let shown = derive(
        &claude(),
        dir.path(),
        Some(PlatformKey::LinuxX64Baseline),
        false,
        false,
        IsolationLevel::BestEffort,
    );

    assert_eq!(shown.installed_version.as_deref(), Some("2.1.311"));
    assert_ne!(shown.state, CliToolState::NeedsReinstall);
}
