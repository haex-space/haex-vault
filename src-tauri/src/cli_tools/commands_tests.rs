//! The planning behind the commands: what is listed, what an install request comes to (spec 094,
//! T016, `contracts/tauri-commands.md`).

use std::fs;

use tempfile::TempDir;

use super::{plan_install, registry_key, status_of_all, Plan};
use crate::cli_tools::catalog::Catalog;
use crate::cli_tools::install::InstallError;
use crate::cli_tools::installed::InstallMarker;
use crate::cli_tools::platform::PlatformKey;
use crate::cli_tools::runtime::DelegateTools;
use crate::cli_tools::status::CliToolState;
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
    assert_eq!(ids, ["claude", "codex"]);
    assert!(statuses
        .iter()
        .all(|s| s.state == CliToolState::NotInstalled));
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
    assert_eq!(context.artifact, listed.platforms[host().as_str()]);
    assert_eq!(context.platform, host());
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
    InstallMarker::new("claude", &listed, host(), &"a".repeat(64), 1, 7)
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
