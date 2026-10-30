use super::*;

#[test]
fn every_platform_has_its_x_platform_name() {
    for (os, name) in [
        ("linux", "linux"),
        ("windows", "windows"),
        ("macos", "macos"),
        ("android", "android"),
        ("ios", "ios"),
        ("freebsd", "linux"),
    ] {
        assert_eq!(platform_name(os), name, "{os}");
    }
    assert!(["linux", "windows", "macos", "android", "ios"].contains(&platform()));
}

#[test]
fn the_app_version_is_the_crate_version() {
    assert_eq!(app_version().to_string(), env!("CARGO_PKG_VERSION"));
}
