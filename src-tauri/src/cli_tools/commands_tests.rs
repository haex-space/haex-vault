//! The planning behind the commands: what is listed, what an install request comes to (spec 094,
//! T016, `contracts/tauri-commands.md`).

use std::fs;
use std::sync::Arc;

use tempfile::TempDir;

use super::{
    plan_for, plan_for_host, plan_install, registry_key, remove_tool, revert_tool, status_of_all,
    Plan,
};
use crate::cli_tools::catalog::{Catalog, CatalogVersion, VendorDefinition};
use crate::cli_tools::containment::IsolationLevel;
use crate::cli_tools::install::{InstallContext, InstallError};
use crate::cli_tools::installed::{InstallMarker, InstalledFile, CONFIRMED_FILE};
use crate::cli_tools::platform::{host_has_avx2, PlatformKey};
use crate::cli_tools::runtime::DelegateTools;
use crate::cli_tools::status::{derive, CliToolState};
use crate::cli_tools::uses::Lease;
use crate::instances::vault_paths::VaultPaths;
use crate::models::cancel::DownloadRegistry;

fn tools(data: &TempDir) -> DelegateTools {
    DelegateTools::new(VaultPaths::new(data.path()), "anna")
}

fn host() -> PlatformKey {
    PlatformKey::host().expect("a desktop platform")
}

#[test]
fn every_vendor_of_the_catalog_is_listed_in_catalog_order_and_nothing_touches_the_disk() {
    let data = TempDir::new().unwrap();
    let statuses = status_of_all(&tools(&data), &DownloadRegistry::default());
    let ids: Vec<_> = statuses.iter().map(|s| s.vendor.as_str()).collect();
    assert_eq!(ids, ["claude", "codex", "abacusai"]);
    // Abacus is listed before it has a tested version (spec 096).
    assert!(statuses[..2]
        .iter()
        .all(|s| s.state == CliToolState::NotInstalled));
    assert_eq!(statuses[2].state, CliToolState::NoTestedVersion);
    assert_eq!(
        fs::read_dir(data.path()).unwrap().count(),
        0,
        "reading the status writes nothing"
    );
}

#[test]
fn a_running_install_shows_as_installing_in_this_vault() {
    let data = TempDir::new().unwrap();
    let running = DownloadRegistry::default();
    let slot = running.begin(&registry_key("codex"));
    let statuses = status_of_all(&tools(&data), &running);
    assert_eq!(statuses[0].state, CliToolState::NotInstalled);
    assert_eq!(statuses[1].state, CliToolState::Installing);
    drop(slot);
    assert_eq!(
        status_of_all(&tools(&data), &running)[1].state,
        CliToolState::NotInstalled
    );
}

#[test]
fn an_unknown_vendor_or_version_is_refused_with_its_own_code() {
    let data = TempDir::new().unwrap();
    let tools = tools(&data);
    let error = plan_install(&tools, "nobody", None).err().unwrap();
    assert_eq!(error, InstallError::UnknownVendor);
    assert_eq!(error.code(), "cli_tools.unknown_vendor");
    let error = plan_install(&tools, "claude", Some("0.0.1")).err().unwrap();
    assert_eq!(error, InstallError::UnknownVersion);
    assert_eq!(error.code(), "cli_tools.unknown_version");
}

#[test]
fn an_install_takes_the_newest_listed_version_for_this_device_and_stays_in_the_vault() {
    let data = TempDir::new().unwrap();
    let tools = tools(&data);
    let Plan::Install(context) = plan_install(&tools, "claude", None).ok().unwrap() else {
        panic!("nothing is installed yet");
    };
    let listed = &Catalog::shipped().vendor("claude").unwrap().versions[0];
    assert_eq!(context.version, listed.version);
    let (platform, artifact) = listed
        .artifact_for(host(), host_has_avx2())
        .expect("Claude has an artifact for this host");
    assert_eq!(&context.artifact, artifact);
    assert_eq!(context.platform, platform);
    let paths = VaultPaths::new(data.path());
    assert_eq!(context.vendor_dir, paths.cli_tool_dir("anna", "claude"));
    assert_eq!(context.home, paths.home("anna"));
    assert_eq!(context.run_root, paths.run_root("anna"));
    assert_eq!(context.record.dir(), paths.records_dir("anna"));
}

#[test]
fn a_listed_version_that_is_given_is_the_one_installed() {
    let data = TempDir::new().unwrap();
    let listed = &Catalog::shipped().vendor("codex").unwrap().versions[0];
    let Plan::Install(context) = plan_install(&tools(&data), "codex", Some(&listed.version))
        .ok()
        .unwrap()
    else {
        panic!("nothing is installed yet");
    };
    assert_eq!(context.version, listed.version);
}

#[test]
fn the_version_already_in_use_is_not_downloaded_again() {
    let data = TempDir::new().unwrap();
    let paths = VaultPaths::new(data.path());
    let vendor = Catalog::shipped().vendor("claude").unwrap();
    let listed = vendor.versions[0].version.clone();
    let version_dir = paths.cli_tool_dir("anna", "claude").join(&listed);
    let executable = version_dir.join(vendor.executable_for(host()).unwrap());
    fs::create_dir_all(executable.parent().unwrap()).unwrap();
    fs::write(&executable, b"program").unwrap();
    InstallMarker::new(
        "claude",
        &listed,
        host(),
        &"a".repeat(64),
        1,
        7,
        vec![InstalledFile {
            path: Catalog::active()
                .vendor("claude")
                .unwrap()
                .executable_for(host())
                .unwrap()
                .into(),
            size: 7,
        }],
    )
    .write(&version_dir)
    .unwrap();

    assert!(matches!(
        plan_install(&tools(&data), "claude", None).ok().unwrap(),
        Plan::AlreadyInstalled
    ));
    // A damaged folder of that version is installed again.
    fs::remove_file(&executable).unwrap();
    assert!(matches!(
        plan_install(&tools(&data), "claude", None).ok().unwrap(),
        Plan::Install(_)
    ));
}

// Updates and the way back (spec 094 US3, FR-022).

/// The Claude definition with exactly `versions` listed, newest first, all for this device.
fn claude_listing(versions: &[&str]) -> VendorDefinition {
    let mut vendor = Catalog::shipped().vendor("claude").unwrap().clone();
    let (platform, artifact) = vendor.versions[0]
        .artifact_for(host(), host_has_avx2())
        .expect("Claude has an artifact for this host");
    let artifact = artifact.clone();
    vendor.versions = versions
        .iter()
        .map(|version| CatalogVersion {
            version: (*version).to_string(),
            platforms: [(platform.as_str().to_string(), artifact.clone())].into(),
        })
        .collect();
    vendor
}

/// A complete installed version of Claude in the vault `anna`.
fn put(data: &TempDir, version: &str, confirmed: bool) -> std::path::PathBuf {
    let paths = VaultPaths::new(data.path());
    let vendor = Catalog::shipped().vendor("claude").unwrap();
    let version_dir = paths.cli_tool_dir("anna", "claude").join(version);
    let relative = vendor.executable_for(host()).unwrap();
    let executable = version_dir.join(relative);
    fs::create_dir_all(executable.parent().unwrap()).unwrap();
    fs::write(&executable, b"program").unwrap();
    InstallMarker::new(
        "claude",
        version,
        host(),
        &"a".repeat(64),
        1,
        7,
        vec![InstalledFile {
            path: relative.into(),
            size: 7,
        }],
    )
    .write(&version_dir)
    .unwrap();
    if confirmed {
        fs::write(version_dir.join(CONFIRMED_FILE), "").unwrap();
    }
    version_dir
}

fn plan(
    data: &TempDir,
    vendor: &VendorDefinition,
    version: Option<&str>,
) -> Result<Plan, InstallError> {
    plan_for(&tools(data), vendor, version)
}

#[test]
fn a_newer_listed_version_is_an_update_that_leaves_the_version_in_use_alone() {
    let data = TempDir::new().unwrap();
    let vendor = claude_listing(&["1.1.0", "1.0.0"]);
    let old = put(&data, "1.0.0", true);

    let Plan::Install(context) = plan(&data, &vendor, None).ok().unwrap() else {
        panic!("an update was expected");
    };

    assert_eq!(context.version, "1.1.0");
    assert!(old.is_dir(), "planning does not touch the version in use");
}

#[test]
fn an_older_or_the_same_listed_version_is_never_installed_over_the_one_in_use() {
    let data = TempDir::new().unwrap();
    let vendor = claude_listing(&["1.1.0", "1.0.0"]);
    put(&data, "1.1.0", true);

    assert!(matches!(
        plan(&data, &vendor, Some("1.0.0")).ok().unwrap(),
        Plan::AlreadyInstalled
    ));
    assert!(matches!(
        plan(&data, &vendor, None).ok().unwrap(),
        Plan::AlreadyInstalled
    ));
}

#[test]
fn a_second_update_waits_while_the_version_in_use_is_unconfirmed_with_a_way_back() {
    let data = TempDir::new().unwrap();
    let vendor = claude_listing(&["1.2.0", "1.1.0", "1.0.0"]);
    put(&data, "1.0.0", true);
    put(&data, "1.1.0", false);

    let error = plan(&data, &vendor, None).err().unwrap();

    assert_eq!(error, InstallError::UnconfirmedUpdate);
    assert_eq!(error.code(), "cli_tools.unconfirmed_update");
}

#[test]
fn the_first_update_of_a_version_no_run_has_confirmed_is_allowed_and_keeps_it_as_the_way_back() {
    let data = TempDir::new().unwrap();
    let vendor = claude_listing(&["1.1.0", "1.0.0"]);
    put(&data, "1.0.0", false);

    assert!(matches!(
        plan(&data, &vendor, None).ok().unwrap(),
        Plan::Install(_)
    ));
}

#[test]
fn an_update_waits_while_a_fallback_is_pending_deletion_for_a_run_and_goes_on_after() {
    let data = TempDir::new().unwrap();
    let vendor = claude_listing(&["1.2.0", "1.1.0", "1.0.0"]);
    let vendor_dir = VaultPaths::new(data.path()).cli_tool_dir("anna", "claude");
    put(&data, "1.0.0", false);
    put(&data, "1.1.0", true);
    let run = Lease::take(
        vendor_dir.clone(),
        "1.0.0".into(),
        Arc::new(vendor.clone()),
        PlatformKey::host(),
        None,
    );

    let error = plan(&data, &vendor, None).err().unwrap();
    assert_eq!(error, InstallError::VersionInUse);
    assert_eq!(error.code(), "cli_tools.version_in_use");
    assert!(
        vendor_dir.join("1.0.0").is_dir(),
        "the run keeps its version"
    );

    // The run ends: the fallback goes with it, and the update is possible.
    drop(run);
    assert!(!vendor_dir.join("1.0.0").exists());
    assert!(matches!(
        plan(&data, &vendor, None).ok().unwrap(),
        Plan::Install(_)
    ));
}

#[test]
fn an_update_removes_a_pending_fallback_nothing_uses_any_more() {
    let data = TempDir::new().unwrap();
    let vendor = claude_listing(&["1.2.0", "1.1.0", "1.0.0"]);
    let vendor_dir = VaultPaths::new(data.path()).cli_tool_dir("anna", "claude");
    // A deletion an earlier session could not finish.
    put(&data, "1.0.0", true);
    put(&data, "1.1.0", true);

    assert!(matches!(
        plan(&data, &vendor, None).ok().unwrap(),
        Plan::Install(_)
    ));
    assert!(!vendor_dir.join("1.0.0").exists());
}

#[test]
fn going_back_deletes_the_unconfirmed_newest_and_the_status_follows() {
    let data = TempDir::new().unwrap();
    let vendor = claude_listing(&["1.1.0", "1.0.0"]);
    let tools = tools(&data);
    let vendor_dir = VaultPaths::new(data.path()).cli_tool_dir("anna", "claude");
    put(&data, "1.0.0", true);
    put(&data, "1.1.0", false);

    revert_tool(&tools, &vendor).unwrap();

    assert!(!vendor_dir.join("1.1.0").exists());
    assert!(vendor_dir.join("1.0.0").is_dir());
    let shown = derive(
        &vendor,
        &vendor_dir,
        PlatformKey::host(),
        crate::cli_tools::platform::host_has_avx2(),
        false,
        IsolationLevel::BestEffort,
    );
    assert_eq!(shown.installed_version.as_deref(), Some("1.0.0"));
    assert_eq!(shown.state, CliToolState::UpdateAvailable);
}

#[test]
fn going_back_is_refused_while_a_run_of_the_tool_is_in_progress_or_nothing_is_to_go_back_from() {
    let data = TempDir::new().unwrap();
    let vendor = claude_listing(&["1.1.0", "1.0.0"]);
    let tools = tools(&data);
    let vendor_dir = VaultPaths::new(data.path()).cli_tools("anna");
    let vendor_dir = vendor_dir.join("claude");
    put(&data, "1.0.0", true);
    put(&data, "1.1.0", false);

    let run = Lease::take(
        vendor_dir.clone(),
        "1.1.0".into(),
        Arc::new(vendor.clone()),
        PlatformKey::host(),
        None,
    );
    let error = revert_tool(&tools, &vendor).unwrap_err();
    assert_eq!(error, InstallError::InUse);
    assert_eq!(error.code(), "cli_tools.in_use");
    drop(run);

    revert_tool(&tools, &vendor).unwrap();
    let error = revert_tool(&tools, &vendor).unwrap_err();
    assert_eq!(error, InstallError::NothingToRevert);
    assert_eq!(error.code(), "cli_tools.nothing_to_revert");
}

#[test]
fn removing_a_tool_deletes_all_its_versions_and_leaves_the_home_and_other_tools_alone() {
    let data = TempDir::new().unwrap();
    let vendor = claude_listing(&["1.1.0", "1.0.0"]);
    let tools = tools(&data);
    let paths = VaultPaths::new(data.path());
    put(&data, "1.0.0", true);
    put(&data, "1.1.0", false);
    let other = paths.cli_tool_dir("anna", "codex").join("0.1.0");
    fs::create_dir_all(&other).unwrap();
    fs::write(other.join("codex"), b"program").unwrap();
    let home = paths.home("anna");
    fs::create_dir_all(&home).unwrap();
    fs::write(home.join("kept"), b"x").unwrap();

    remove_tool(&tools, &vendor).unwrap();

    assert!(!paths.cli_tool_dir("anna", "claude").exists());
    assert!(other.join("codex").is_file());
    assert!(home.join("kept").is_file());
    let shown = status_of_all(&tools, &DownloadRegistry::default());
    assert_eq!(shown[0].state, CliToolState::NotInstalled);
    // Nothing installed is already removed.
    remove_tool(&tools, &vendor).unwrap();
}

#[test]
fn removing_a_tool_is_refused_while_a_run_of_it_is_in_progress() {
    let data = TempDir::new().unwrap();
    let vendor = claude_listing(&["1.0.0"]);
    let tools = tools(&data);
    let vendor_dir = VaultPaths::new(data.path()).cli_tool_dir("anna", "claude");
    put(&data, "1.0.0", true);

    let run = Lease::take(
        vendor_dir.clone(),
        "1.0.0".into(),
        Arc::new(vendor.clone()),
        PlatformKey::host(),
        None,
    );
    let error = remove_tool(&tools, &vendor).unwrap_err();
    assert_eq!(error, InstallError::InUse);
    assert!(vendor_dir.join("1.0.0").is_dir());
    drop(run);

    remove_tool(&tools, &vendor).unwrap();
    assert!(!vendor_dir.exists());
}

// The variant of a platform, the AVX2 rule and a vendor with no tested version (spec 096).

const AVX_VENDOR: &str = r#"{"schema":1,"vendors":[{"vendor":"avxtool","display_name":"Avx","download_hosts":["dl.example.com"],
    "executable":{"linux-x64":"t","linux-x64-baseline":"t","darwin-x64":"t"},"config_env":"T_HOME",
    "versions":[{"version":"1.0.0","platforms":{
    "linux-x64":{"url":"https://dl.example.com/x64.tgz","sha256":"0000000000000000000000000000000000000000000000000000000000000001","size":10,"kind":"tar.gz","unpacked_size":20,"requires_avx2":true},
    "linux-x64-baseline":{"url":"https://dl.example.com/base.tgz","sha256":"0000000000000000000000000000000000000000000000000000000000000002","size":11,"kind":"tar.gz","unpacked_size":21},
    "darwin-x64":{"url":"https://dl.example.com/mac.tgz","sha256":"0000000000000000000000000000000000000000000000000000000000000003","size":12,"kind":"tar.gz","unpacked_size":22,"requires_avx2":true}}}]}]}"#;

fn avx_vendor() -> VendorDefinition {
    Catalog::parse(AVX_VENDOR)
        .unwrap()
        .vendor("avxtool")
        .unwrap()
        .clone()
}

fn planned(
    vendor: &VendorDefinition,
    version: Option<&str>,
    host: PlatformKey,
    avx2: bool,
) -> Result<Box<InstallContext>, InstallError> {
    let data = TempDir::new().unwrap();
    match plan_for_host(&tools(&data), vendor, version, Some(host), avx2)? {
        Plan::Install(context) => Ok(context),
        Plan::AlreadyInstalled => panic!("nothing is installed yet"),
    }
}

#[test]
fn a_host_without_avx2_gets_the_variant_of_a_vendor_that_lists_one() {
    let vendor = avx_vendor();
    let context = planned(&vendor, None, PlatformKey::LinuxX64Baseline, false).unwrap();
    assert_eq!(context.platform, PlatformKey::LinuxX64Baseline);
    assert_eq!(context.artifact.size, 11);

    // A host with AVX2 keeps the base key.
    let context = planned(&vendor, None, PlatformKey::LinuxX64, true).unwrap();
    assert_eq!(context.platform, PlatformKey::LinuxX64);
    assert_eq!(context.artifact.size, 10);
}

#[test]
fn a_vendor_without_the_variant_installs_the_base_key_on_a_host_without_avx2() {
    let data = TempDir::new().unwrap();
    let Plan::Install(context) = plan_for_host(
        &tools(&data),
        Catalog::shipped().vendor("claude").unwrap(),
        None,
        Some(PlatformKey::LinuxX64Baseline),
        false,
    )
    .ok()
    .unwrap() else {
        panic!("nothing is installed yet");
    };
    assert_eq!(context.platform, PlatformKey::LinuxX64);
}

#[test]
fn an_artifact_that_needs_avx2_is_not_offered_to_a_host_without_it() {
    let vendor = avx_vendor();
    assert_eq!(
        planned(&vendor, None, PlatformKey::DarwinX64, false).err(),
        Some(InstallError::UnsupportedPlatform)
    );
    assert_eq!(
        planned(&vendor, Some("1.0.0"), PlatformKey::DarwinX64, false).err(),
        Some(InstallError::UnsupportedPlatform)
    );
    let context = planned(&vendor, None, PlatformKey::DarwinX64, true).unwrap();
    assert_eq!(context.platform, PlatformKey::DarwinX64);
    // Claude Code and Codex set no such requirement and stay installable on the same host.
    let data = TempDir::new().unwrap();
    for name in ["claude", "codex"] {
        let vendor = Catalog::shipped().vendor(name).unwrap();
        let plan = plan_for_host(
            &tools(&data),
            vendor,
            None,
            Some(PlatformKey::DarwinX64),
            false,
        );
        assert!(plan.is_ok(), "{name}");
    }
}

#[test]
fn a_vendor_with_no_tested_version_is_listed_and_refuses_the_install() {
    let data = TempDir::new().unwrap();
    let statuses = status_of_all(&tools(&data), &DownloadRegistry::default());
    assert!(
        statuses.iter().any(|status| status.vendor == "abacusai"),
        "the vendor is listed"
    );

    let vendor = abacus_listing(&[]);
    let abacus = abacus_status(&vendor, &data);
    assert_eq!(abacus.state, CliToolState::NoTestedVersion);
    assert_eq!(abacus.download_bytes, None);
    assert_eq!(abacus.latest_listed_version, None);

    let error = plan_for(&tools(&data), &vendor, None).err().unwrap();
    assert_eq!(error, InstallError::NoTestedVersion);
    assert_eq!(error.code(), "cli_tools.no_tested_version");
    let error = plan_for(&tools(&data), &vendor, Some("2.10.23"))
        .err()
        .unwrap();
    assert_eq!(error, InstallError::NoTestedVersion);
}

// ---- Abacus AI: the life of a tool that is installed from a release (spec 096 US5, T039) ----

/// The shipped Abacus entry with the listed `versions` in place of the shipped ones.
fn abacus_listing(versions: &[&str]) -> VendorDefinition {
    use crate::cli_tools::catalog::{Artifact, ArtifactKind};
    let mut vendor = Catalog::shipped().vendor("abacusai").unwrap().clone();
    let artifact = Artifact {
        url: "https://static.abacus.ai/cli/releases/test/abacusai-test.tar.gz".to_string(),
        sha256: "0".repeat(64),
        size: 1,
        kind: ArtifactKind::TarGz,
        unpacked_size: Some(1),
        requires_avx2: false,
    };
    vendor.versions = versions
        .iter()
        .map(|version| CatalogVersion {
            version: (*version).to_string(),
            platforms: [(host().as_str().to_string(), artifact.clone())].into(),
        })
        .collect();
    vendor
}

/// A complete installed version of Abacus in the vault `anna`, with the signature file the
/// release carries next to the program.
fn put_abacus(data: &TempDir, version: &str, confirmed: bool) -> std::path::PathBuf {
    let paths = VaultPaths::new(data.path());
    let vendor = Catalog::shipped().vendor("abacusai").unwrap();
    let version_dir = paths.cli_tool_dir("anna", "abacusai").join(version);
    let relative = vendor.executable_for(host()).unwrap();
    fs::create_dir_all(&version_dir).unwrap();
    fs::write(version_dir.join(relative), b"program").unwrap();
    fs::write(version_dir.join("abacusai.sigstore"), b"bundle").unwrap();
    InstallMarker::new(
        "abacusai",
        version,
        host(),
        &"a".repeat(64),
        1,
        7,
        vec![InstalledFile {
            path: relative.into(),
            size: 7,
        }],
    )
    .write(&version_dir)
    .unwrap();
    if confirmed {
        fs::write(version_dir.join(CONFIRMED_FILE), "").unwrap();
    }
    version_dir
}

fn abacus_status(
    vendor: &VendorDefinition,
    data: &TempDir,
) -> crate::cli_tools::status::CliToolStatus {
    derive(
        vendor,
        &VaultPaths::new(data.path()).cli_tool_dir("anna", "abacusai"),
        PlatformKey::host(),
        host_has_avx2(),
        false,
        IsolationLevel::BestEffort,
    )
}

#[test]
fn abacus_updates_goes_back_and_is_removed_like_the_other_tools() {
    let data = TempDir::new().unwrap();
    let vendor = abacus_listing(&["1.1.0", "1.0.0"]);
    let tools = tools(&data);
    let vendor_dir = VaultPaths::new(data.path()).cli_tool_dir("anna", "abacusai");
    put_abacus(&data, "1.0.0", true);
    assert_eq!(
        abacus_status(&vendor, &data).state,
        CliToolState::UpdateAvailable,
        "a newer listed version is an update"
    );

    // The update is installed next to the version in use; until a run confirms it, the way back
    // stays.
    put_abacus(&data, "1.1.0", false);
    assert_eq!(abacus_status(&vendor, &data).state, CliToolState::Installed);
    revert_tool(&tools, &vendor).unwrap();
    assert!(!vendor_dir.join("1.1.0").exists());
    assert!(vendor_dir.join("1.0.0").is_dir());
    assert_eq!(
        abacus_status(&vendor, &data).state,
        CliToolState::UpdateAvailable
    );

    // Confirmation by a run keeps the newer one; removing takes every version and the signature
    // files with them and leaves the home alone.
    put_abacus(&data, "1.1.0", true);
    let home = VaultPaths::new(data.path()).home("anna");
    fs::create_dir_all(&home).unwrap();
    fs::write(home.join("kept"), b"x").unwrap();
    remove_tool(&tools, &vendor).unwrap();
    assert!(!vendor_dir.exists());
    assert!(home.join("kept").is_file());
    assert_eq!(
        abacus_status(&vendor, &data).state,
        CliToolState::NotInstalled
    );
}

#[test]
fn a_version_of_abacus_outside_the_list_is_shown_as_untested_and_the_extra_file_does_not_matter() {
    let data = TempDir::new().unwrap();
    let vendor = abacus_listing(&["1.1.0", "1.0.0"]);
    let version = put_abacus(&data, "2.0.0", true);

    let shown = abacus_status(&vendor, &data);

    assert_eq!(shown.state, CliToolState::Installed);
    assert_eq!(shown.installed_version.as_deref(), Some("2.0.0"));
    assert!(shown.untested);

    let mut marker = InstallMarker::read(&version).expect("installed marker");
    let original_platform = marker.platform.clone();
    marker.platform = if host() == PlatformKey::DarwinArm64 {
        PlatformKey::Win32X64
    } else {
        PlatformKey::DarwinArm64
    }
    .as_str()
    .to_owned();
    marker.write(&version).unwrap();
    let wrong_platform = abacus_status(&vendor, &data);
    assert_eq!(wrong_platform.state, CliToolState::NeedsReinstall);
    assert_eq!(wrong_platform.reason.as_deref(), Some("platform"));

    marker.platform = original_platform;
    marker.write(&version).unwrap();
    fs::remove_file(version.join(vendor.executable_for(host()).unwrap())).unwrap();
    let incomplete = abacus_status(&vendor, &data);
    assert_eq!(incomplete.state, CliToolState::NeedsReinstall);
    assert_eq!(incomplete.reason.as_deref(), Some("incomplete"));
}
