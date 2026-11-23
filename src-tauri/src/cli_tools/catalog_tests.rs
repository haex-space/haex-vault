//! The catalog and its validation (spec 094, T006, FR-009, FR-015).

use serde_json::{json, Value};

use super::{ArtifactKind, Catalog};
use crate::cli_tools::platform::PlatformKey;

const HASH_A: &str = "86dc1fcc2d5b3a0f6f1d5e3c4b7a8d9e0f1a2b3c4d5e6f708192a3b4c8489ada";
const HASH_B: &str = "3b8ed0911111111111111111111111111111111111111111111111111dd71ba0";

fn artifact(host: &str, hash: &str) -> Value {
    json!({
        "url": format!("https://{host}/tool/1.0.0/tool"),
        "sha256": hash,
        "size": 1000,
        "kind": "file"
    })
}

fn vendor(name: &str, versions: &[&str]) -> Value {
    let versions: Vec<Value> = versions
        .iter()
        .map(|version| {
            json!({
                "version": version,
                "platforms": { "linux-x64": artifact("dl.example.com", HASH_A) }
            })
        })
        .collect();
    json!({
        "vendor": name,
        "display_name": "Tool",
        "download_hosts": ["dl.example.com"],
        "executable": { "linux-x64": "tool", "win32-x64": "tool.exe" },
        "env_set": [{ "name": "NO_UPDATE", "value": "1" }],
        "config_args": ["--quiet"],
        "env_remove_prefixes": ["TOOL_"],
        "config_env": "TOOL_HOME",
        "tmp_env": ["TOOL_TMP"],
        "versions": versions
    })
}

fn catalog(vendors: Vec<Value>) -> Value {
    json!({ "schema": 1, "vendors": vendors })
}

fn parse(value: &Value) -> Result<Catalog, String> {
    Catalog::parse(&value.to_string()).map_err(|error| error.to_string())
}

fn rejected(value: &Value, fragment: &str) {
    let error = parse(value).expect_err("the catalog must be refused");
    assert!(error.contains(fragment), "{error:?} lacks {fragment:?}");
}

#[test]
fn a_valid_catalog_reads_and_answers_by_vendor_version_and_platform() {
    let parsed = parse(&catalog(vec![vendor("tool", &["2.0.0", "1.9.0"])])).unwrap();
    let tool = parsed.vendor("tool").unwrap();
    assert!(parsed.vendor("other").is_none());
    assert_eq!(tool.versions.len(), 2);
    let artifact = tool.artifact("1.9.0", PlatformKey::LinuxX64).unwrap();
    assert_eq!(artifact.kind, ArtifactKind::File);
    assert_eq!(artifact.sha256, HASH_A);
    assert!(tool.artifact("1.9.0", PlatformKey::DarwinArm64).is_none());
    assert!(tool.artifact("0.1.0", PlatformKey::LinuxX64).is_none());
    assert_eq!(tool.executable_for(PlatformKey::Win32X64), Some("tool.exe"));
    assert_eq!(tool.executable_for(PlatformKey::LinuxArm64), None);
    assert_eq!(
        tool.newest_for(PlatformKey::LinuxX64, true)
            .unwrap()
            .version
            .version,
        "2.0.0"
    );
    assert!(tool.newest_for(PlatformKey::Win32X64, true).is_none());
}

#[test]
fn versions_must_be_strictly_descending_by_semver() {
    rejected(
        &catalog(vec![vendor("tool", &["1.9.0", "2.0.0"])]),
        "strictly descending",
    );
    rejected(
        &catalog(vec![vendor("tool", &["2.0.0", "2.0.0"])]),
        "strictly descending",
    );
    // Numeric, not textual: 2.10.0 is newer than 2.9.0.
    parse(&catalog(vec![vendor("tool", &["2.10.0", "2.9.0"])])).unwrap();
    rejected(
        &catalog(vec![vendor("tool", &["2.9.0", "2.10.0"])]),
        "strictly descending",
    );
    rejected(&catalog(vec![vendor("tool", &["latest"])]), "semver");
}

#[test]
fn a_vendor_needs_a_version_a_host_and_a_known_shape() {
    let mut no_host = vendor("tool", &["1.0.0"]);
    no_host["download_hosts"] = json!([]);
    rejected(&catalog(vec![no_host]), "no download host");
    let mut no_config = vendor("tool", &["1.0.0"]);
    no_config["config_env"] = json!("");
    rejected(&catalog(vec![no_config]), "config_env");
    rejected(&catalog(vec![vendor("Tool-X", &["1.0.0"])]), "vendor id");
    rejected(
        &catalog(vec![vendor("tool", &["1.0.0"]), vendor("tool", &["1.0.0"])]),
        "listed twice",
    );
    let mut unknown_key = vendor("tool", &["1.0.0"]);
    unknown_key["executable"]["linux-musl"] = json!("tool");
    rejected(&catalog(vec![unknown_key]), "unknown platform key");
    let mut wrong_schema = catalog(vec![vendor("tool", &["1.0.0"])]);
    wrong_schema["schema"] = json!(2);
    rejected(&wrong_schema, "schema 2");
    let mut stray = vendor("tool", &["1.0.0"]);
    stray["surprise"] = json!(true);
    assert!(parse(&catalog(vec![stray])).is_err());
}

#[test]
fn every_artifact_needs_a_valid_hash_an_https_url_on_the_vendors_host() {
    for bad in [
        "",
        "abc",
        &HASH_A.to_uppercase(),
        &HASH_A[..63],
        &format!("{HASH_A}0"),
    ] {
        let mut entry = vendor("tool", &["1.0.0"]);
        entry["versions"][0]["platforms"]["linux-x64"]["sha256"] = json!(bad);
        rejected(&catalog(vec![entry]), "sha256");
    }
    let mut not_hex = vendor("tool", &["1.0.0"]);
    not_hex["versions"][0]["platforms"]["linux-x64"]["sha256"] = json!(HASH_A.replace('8', "g"));
    rejected(&catalog(vec![not_hex]), "sha256");

    let mut plain = vendor("tool", &["1.0.0"]);
    plain["versions"][0]["platforms"]["linux-x64"]["url"] = json!("http://dl.example.com/tool");
    rejected(&catalog(vec![plain]), "not https");

    let mut foreign = vendor("tool", &["1.0.0"]);
    foreign["versions"][0]["platforms"]["linux-x64"] = artifact("evil.example.org", HASH_A);
    rejected(&catalog(vec![foreign]), "not a download host");

    let mut lookalike = vendor("tool", &["1.0.0"]);
    lookalike["versions"][0]["platforms"]["linux-x64"] =
        artifact("dl.example.com.evil.org", HASH_A);
    rejected(&catalog(vec![lookalike]), "not a download host");

    let mut empty = vendor("tool", &["1.0.0"]);
    empty["versions"][0]["platforms"]["linux-x64"]["size"] = json!(0);
    rejected(&catalog(vec![empty]), "size is 0");

    let mut no_executable = vendor("tool", &["1.0.0"]);
    no_executable["versions"][0]["platforms"]["linux-arm64"] = artifact("dl.example.com", HASH_B);
    rejected(&catalog(vec![no_executable]), "no executable path");
}

#[test]
fn a_package_names_its_unpacked_size_and_a_file_does_not() {
    let mut package = vendor("tool", &["1.0.0"]);
    package["versions"][0]["platforms"]["linux-x64"]["kind"] = json!("tar.gz");
    rejected(&catalog(vec![package.clone()]), "needs unpacked_size");
    package["versions"][0]["platforms"]["linux-x64"]["unpacked_size"] = json!(5000);
    let parsed = parse(&catalog(vec![package])).unwrap();
    let artifact = parsed
        .vendor("tool")
        .unwrap()
        .artifact("1.0.0", PlatformKey::LinuxX64)
        .unwrap();
    assert_eq!(artifact.kind, ArtifactKind::TarGz);
    assert_eq!(artifact.unpacked_size, Some(5000));

    let mut file = vendor("tool", &["1.0.0"]);
    file["versions"][0]["platforms"]["linux-x64"]["unpacked_size"] = json!(5000);
    rejected(&catalog(vec![file]), "no unpacked_size");
}

#[test]
fn the_shipped_catalog_is_valid_and_lists_the_delegate_tools() {
    let shipped = Catalog::shipped();
    let ids: Vec<_> = shipped.vendors.iter().map(|v| v.vendor.as_str()).collect();
    assert_eq!(ids, ["claude", "codex", "abacusai"]);
    for vendor in shipped.vendors.iter().filter(|v| !v.versions.is_empty()) {
        // Every platform of the first listed version has a program path, and the listed
        // versions are the ones an install may choose from.
        let newest = &vendor.versions[0];
        for key in newest.platforms.keys() {
            let platform = PlatformKey::parse(key).unwrap();
            assert!(
                vendor.executable_for(platform).is_some(),
                "{} {key}",
                vendor.vendor
            );
        }
        assert!(!vendor.config_env.is_empty());
    }
    let claude = shipped.vendor("claude").unwrap();
    assert!(claude
        .env_set
        .iter()
        .any(|pair| pair.name == "DISABLE_UPDATES" && pair.value == "1"));
    let codex = shipped.vendor("codex").unwrap();
    assert!(codex
        .config_args
        .windows(2)
        .any(|pair| pair == ["-c", "check_for_update_on_startup=false"]));
}

#[test]
fn a_zip_names_its_unpacked_size_like_a_tar_gz() {
    let mut package = vendor("tool", &["1.0.0"]);
    package["versions"][0]["platforms"]["linux-x64"]["kind"] = json!("zip");
    rejected(&catalog(vec![package.clone()]), "a zip needs unpacked_size");
    package["versions"][0]["platforms"]["linux-x64"]["unpacked_size"] = json!(5000);
    let parsed = parse(&catalog(vec![package])).unwrap();
    let artifact = parsed
        .vendor("tool")
        .unwrap()
        .artifact("1.0.0", PlatformKey::LinuxX64)
        .unwrap();
    assert_eq!(artifact.kind, ArtifactKind::Zip);
    assert_eq!(artifact.unpacked_size, Some(5000));
}

#[test]
fn an_artifact_may_require_avx2_and_does_not_by_default() {
    let mut entry = vendor("tool", &["1.0.0"]);
    let parsed = parse(&catalog(vec![entry.clone()])).unwrap();
    let artifact = parsed
        .vendor("tool")
        .unwrap()
        .artifact("1.0.0", PlatformKey::LinuxX64);
    assert!(!artifact.unwrap().requires_avx2);

    entry["versions"][0]["platforms"]["linux-x64"]["requires_avx2"] = json!(true);
    let parsed = parse(&catalog(vec![entry])).unwrap();
    let tool = parsed.vendor("tool").unwrap();
    assert!(
        tool.artifact("1.0.0", PlatformKey::LinuxX64)
            .unwrap()
            .requires_avx2
    );
    assert!(tool.newest_for(PlatformKey::LinuxX64, true).is_some());
    assert!(tool.newest_for(PlatformKey::LinuxX64, false).is_none());
}

fn with_variant() -> Value {
    let mut entry = vendor("tool", &["1.0.0"]);
    entry["executable"]["linux-x64-baseline"] = json!("tool-baseline");
    entry["versions"][0]["platforms"]["linux-x64-baseline"] = artifact("dl.example.com", HASH_B);
    entry
}

#[test]
fn a_variant_key_is_valid_and_wins_for_a_host_without_avx2() {
    let parsed = parse(&catalog(vec![with_variant()])).unwrap();
    let tool = parsed.vendor("tool").unwrap();
    let variant = PlatformKey::LinuxX64Baseline;
    let found = tool.newest_for(variant, false).unwrap();
    assert_eq!(found.platform, variant);
    assert_eq!(found.artifact.sha256, HASH_B);
    assert_eq!(tool.executable_for(variant), Some("tool-baseline"));
    // The base key keeps its own program.
    let base = tool.newest_for(PlatformKey::LinuxX64, true).unwrap();
    assert_eq!(base.platform, PlatformKey::LinuxX64);
    assert_eq!(tool.executable_for(PlatformKey::LinuxX64), Some("tool"));
}

#[test]
fn a_vendor_without_the_variant_falls_back_to_the_base_key() {
    let parsed = parse(&catalog(vec![vendor("tool", &["1.0.0"])])).unwrap();
    let tool = parsed.vendor("tool").unwrap();
    let found = tool
        .newest_for(PlatformKey::LinuxX64Baseline, false)
        .unwrap();
    assert_eq!(found.platform, PlatformKey::LinuxX64);
    assert_eq!(found.artifact.sha256, HASH_A);
    assert_eq!(
        tool.executable_for(PlatformKey::LinuxX64Baseline),
        Some("tool")
    );
    assert!(tool
        .artifact("1.0.0", PlatformKey::LinuxX64Baseline)
        .is_some());
}

#[test]
fn a_variant_entry_needs_its_executable_path_like_any_other_key() {
    let mut entry = with_variant();
    entry["executable"]
        .as_object_mut()
        .unwrap()
        .remove("linux-x64-baseline");
    rejected(&catalog(vec![entry]), "no executable path");
}

#[test]
fn a_vendor_may_ship_without_a_tested_version() {
    let parsed = parse(&catalog(vec![vendor("tool", &[])])).unwrap();
    let tool = parsed.vendor("tool").unwrap();
    assert!(tool.versions.is_empty());
    assert!(tool.newest_for(PlatformKey::LinuxX64, true).is_none());
}
