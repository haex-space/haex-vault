//! Tests for the profile on disk (spec 065, T022).

use std::path::Path;

use serde_json::Value;

use super::profile::{is_lasting, no_saved_credentials, pack, unpack, Workdir};

fn write(root: &Path, rel: &str, data: &str) {
    let path = root.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, data).unwrap();
}

fn names(zip: &[u8]) -> Vec<String> {
    let archive = zip::ZipArchive::new(std::io::Cursor::new(zip)).unwrap();
    let mut names: Vec<String> = archive.file_names().map(str::to_owned).collect();
    names.sort();
    names
}

#[test]
fn only_the_lasting_parts_enter_the_zip() {
    let dir = tempfile::tempdir().unwrap();
    let profile = dir.path();
    write(profile, "Local State", "{}");
    write(profile, "Default/Preferences", "{}");
    write(profile, "Default/Network/Cookies", "cookies");
    write(profile, "Default/Network/Cookies-journal", "");
    write(profile, "Default/Local Storage/leveldb/000003.log", "ls");
    write(
        profile,
        "Default/IndexedDB/https_a.de_0.indexeddb.leveldb/CURRENT",
        "idb",
    );
    write(profile, "Default/Cache/Cache_Data/data_0", "cache");
    write(profile, "Default/Code Cache/js/index", "code");
    write(profile, "Default/History", "history");
    write(profile, "Default/Login Data", "passwords");
    write(profile, "GrShaderCache/data_0", "gpu");
    assert_eq!(
        names(&pack(profile).unwrap()),
        [
            "Default/IndexedDB/https_a.de_0.indexeddb.leveldb/CURRENT",
            "Default/Local Storage/leveldb/000003.log",
            "Default/Network/Cookies",
            "Default/Network/Cookies-journal",
            "Default/Preferences",
            "Local State",
        ]
    );
}

#[test]
fn lasting_means_the_part_itself_or_below_it() {
    assert!(is_lasting("Default/Network/Cookies"));
    assert!(is_lasting("Default/Network/Cookies-wal"));
    assert!(is_lasting("Default/Session Storage/000003.log"));
    assert!(!is_lasting("Default/Network/Cookies2"));
    assert!(!is_lasting("Default/Local Storage Backup/x"));
    assert!(!is_lasting("Default/Login Data"));
}

#[test]
fn a_packed_profile_unpacks_to_the_same_files() {
    let from = tempfile::tempdir().unwrap();
    write(from.path(), "Default/Network/Cookies", "the cookie jar");
    write(from.path(), "Local State", r#"{"a":1}"#);
    let zip = pack(from.path()).unwrap();
    let to = tempfile::tempdir().unwrap();
    unpack(&zip, to.path()).unwrap();
    assert_eq!(
        std::fs::read_to_string(to.path().join("Default/Network/Cookies")).unwrap(),
        "the cookie jar"
    );
    assert_eq!(
        std::fs::read_to_string(to.path().join("Local State")).unwrap(),
        r#"{"a":1}"#
    );
}

#[test]
fn unpacking_writes_nothing_outside_the_lasting_parts() {
    use std::io::Write;
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    for name in ["../escape", "Default/Login Data", "Default/Network/Cookies"] {
        writer.start_file(name, options).unwrap();
        writer.write_all(b"x").unwrap();
    }
    let zip = writer.finish().unwrap().into_inner();
    let outer = tempfile::tempdir().unwrap();
    let profile = outer.path().join("profile");
    std::fs::create_dir(&profile).unwrap();
    unpack(&zip, &profile).unwrap();
    assert!(!outer.path().join("escape").exists());
    assert!(!profile.join("Default/Login Data").exists());
    assert!(profile.join("Default/Network/Cookies").exists());
}

#[test]
fn the_working_directory_is_private_and_starts_clean() {
    let root = tempfile::tempdir().unwrap();
    let workdir = Workdir::of(root.path(), "anna");
    // A crashed run left something behind.
    write(&workdir.profile(), "Default/Network/Cookies", "old");
    write(&workdir.downloads(), "half.crdownload", "old");
    workdir.prepare(None).unwrap();
    assert!(!workdir.profile().join("Default/Network/Cookies").exists());
    assert!(!workdir.downloads().join("half.crdownload").exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for dir in [&workdir.dir, &workdir.profile(), &workdir.downloads()] {
            let mode = std::fs::metadata(dir).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o700, "{} is owner-only", dir.display());
        }
    }
    workdir.remove().unwrap();
    assert!(!workdir.dir.exists());
    workdir.remove().unwrap();
}

#[test]
fn the_browser_saves_no_passwords_and_fills_no_forms() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "Default/Preferences",
        r#"{"credentials_enable_service":true,"autofill":{"profile_enabled":true,"x":1},"keep":2}"#,
    );
    no_saved_credentials(dir.path()).unwrap();
    let preferences: Value =
        serde_json::from_slice(&std::fs::read(dir.path().join("Default/Preferences")).unwrap())
            .unwrap();
    assert_eq!(preferences["credentials_enable_service"], false);
    assert_eq!(preferences["credentials_enable_autosignin"], false);
    assert_eq!(preferences["profile"]["password_manager_enabled"], false);
    assert_eq!(preferences["autofill"]["profile_enabled"], false);
    assert_eq!(preferences["autofill"]["credit_card_enabled"], false);
    assert_eq!(preferences["autofill"]["x"], 1, "the rest stays");
    assert_eq!(preferences["keep"], 2);
}

#[test]
fn a_fresh_profile_gets_the_same_settings() {
    let root = tempfile::tempdir().unwrap();
    let workdir = Workdir::of(root.path(), "anna");
    workdir.prepare(None).unwrap();
    let preferences: Value = serde_json::from_slice(
        &std::fs::read(workdir.profile().join("Default/Preferences")).unwrap(),
    )
    .unwrap();
    assert_eq!(preferences["credentials_enable_service"], false);
}
