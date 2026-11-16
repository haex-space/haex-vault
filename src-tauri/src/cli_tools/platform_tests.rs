//! The platform key of a device (spec 094, T005).

use super::PlatformKey;

#[test]
fn the_names_of_the_standard_library_map_to_the_catalog_keys() {
    let cases = [
        ("linux", "x86_64", "linux-x64"),
        ("linux", "aarch64", "linux-arm64"),
        ("macos", "x86_64", "darwin-x64"),
        ("macos", "aarch64", "darwin-arm64"),
        ("windows", "x86_64", "win32-x64"),
        ("windows", "aarch64", "win32-arm64"),
    ];
    for (os, arch, key) in cases {
        let found = PlatformKey::from_parts(os, arch).unwrap_or_else(|| panic!("{os} {arch}"));
        assert_eq!(found.as_str(), key);
        assert_eq!(PlatformKey::parse(key), Some(found));
        assert_eq!(found.to_string(), key);
    }
}

#[test]
fn a_combination_no_vendor_releases_for_has_no_key() {
    for (os, arch) in [
        ("linux", "riscv64"),
        ("linux", "x86"),
        ("android", "aarch64"),
        ("ios", "aarch64"),
        ("freebsd", "x86_64"),
        ("windows", "x86"),
    ] {
        assert_eq!(PlatformKey::from_parts(os, arch), None, "{os} {arch}");
    }
}

#[test]
fn only_the_listed_key_names_parse() {
    for text in [
        "",
        "linux",
        "Linux-x64",
        "linux-x64 ",
        "linux-musl-x64",
        "darwin-x86_64",
    ] {
        assert_eq!(PlatformKey::parse(text), None, "{text:?}");
    }
    assert_eq!(PlatformKey::ALL.len(), 6);
}

#[test]
fn only_the_windows_keys_are_windows() {
    let windows: Vec<_> = PlatformKey::ALL
        .into_iter()
        .filter(|key| key.is_windows())
        .map(PlatformKey::as_str)
        .collect();
    assert_eq!(windows, ["win32-x64", "win32-arm64"]);
}

#[test]
fn the_host_key_matches_the_build_target_on_a_supported_desktop() {
    if cfg!(any(target_os = "android", target_os = "ios")) {
        return;
    }
    let host = PlatformKey::host().expect("a desktop target this build runs on");
    assert_eq!(
        host,
        PlatformKey::from_parts(std::env::consts::OS, std::env::consts::ARCH).unwrap()
    );
}
