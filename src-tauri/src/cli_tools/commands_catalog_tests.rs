//! Catalog selection and the Abacus installation lifecycle (spec 096).

use std::fs;

use tempfile::TempDir;

use super::super::{plan_for, plan_for_host, remove_tool, revert_tool, Plan};
use super::{host, tools};
use crate::cli_tools::catalog::{Catalog, CatalogVersion, VendorDefinition};
use crate::cli_tools::containment::IsolationLevel;
use crate::cli_tools::install::{InstallContext, InstallError};
use crate::cli_tools::installed::{InstallMarker, InstalledFile, CONFIRMED_FILE};
use crate::cli_tools::platform::{host_has_avx2, PlatformKey};
use crate::cli_tools::status::{derive, CliToolState};
use crate::instances::vault_paths::VaultPaths;

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
fn a_vendor_with_no_tested_version_reports_that_state_and_refuses_the_install() {
    let data = TempDir::new().unwrap();

    let tools = tools(&data);
    let vendor = abacus_listing(&[]);
    let abacus = abacus_status(&vendor, &data);
    assert_eq!(abacus.state, CliToolState::NoTestedVersion);
    assert_eq!(abacus.download_bytes, None);
    assert_eq!(abacus.latest_listed_version, None);

    let error = plan_for(&tools, &vendor, None).err().unwrap();
    assert_eq!(error, InstallError::NoTestedVersion);
    assert_eq!(error.code(), "cli_tools.no_tested_version");
    let error = plan_for(&tools, &vendor, Some("2.10.23")).err().unwrap();
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
