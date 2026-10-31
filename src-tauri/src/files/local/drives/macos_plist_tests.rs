use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::*;
use crate::files::local::drives::invariants_hold;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src/files/local/drives/fixtures/macos")
}

fn read<T: DeserializeOwned>(path: &Path) -> T {
    parse(&std::fs::read(path).unwrap()).unwrap()
}

/// The `info-<id>.plist` files of a scenario folder, by device identifier.
fn infos(dir: &Path) -> HashMap<String, VolumeInfo> {
    std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|entry| {
            let path = entry.unwrap().path();
            let name = path.file_name()?.to_str()?.to_owned();
            let id = name
                .strip_prefix("info-")?
                .strip_suffix(".plist")?
                .to_owned();
            Some((id, read(&path)))
        })
        .collect()
}

/// The drives of one recorded scenario, with `replace` swapped in for its info files.
fn scenario(arch: &str, name: &str, replace: &[(&str, &str)]) -> Vec<Drive> {
    let dir = fixtures().join(arch).join(name);
    let mut infos = infos(&dir);
    for (id, file) in replace {
        infos.insert((*id).to_owned(), read(&fixtures().join(arch).join(file)));
    }
    let drives = classify(
        &read(&dir.join("list.plist")),
        &read(&dir.join("apfs-list.plist")),
        &infos,
    );
    assert!(drives.iter().all(invariants_hold), "{drives:#?}");
    drives
}

fn labels(drives: &[Drive]) -> Vec<&str> {
    drives
        .iter()
        .filter_map(|drive| drive.label.as_deref())
        .collect()
}

/// None of the volumes macOS keeps for itself shows (SC-006).
fn no_system_volumes(drives: &[Drive]) {
    for name in [
        "EFI",
        "Preboot",
        "Recovery",
        "VM",
        "Update",
        "Data",
        "Macintosh HD - Data",
        "Hardware",
        "xART",
        "iSCPreboot",
    ] {
        assert!(!labels(drives).contains(&name), "{name} shows: {drives:#?}");
    }
    assert!(
        drives.iter().all(|drive| !drive
            .label
            .as_deref()
            .is_some_and(|label| label.ends_with("_Cryptex"))),
        "a cryptex shows: {drives:#?}"
    );
}

#[test]
fn a_stick_that_is_not_mounted_shows_beside_the_system_drive() {
    for (arch, id) in [("arm64", "disk7s1"), ("x86_64", "disk2s1")] {
        let drives = scenario(arch, "fat-unmounted", &[]);
        no_system_volumes(&drives);
        assert_eq!(drives.len(), 2, "{arch}: {drives:#?}");
        let system = drives.iter().find(|d| d.kind == DriveKind::System).unwrap();
        assert_eq!(system.path.as_deref(), Some("/"), "{arch}");
        assert_eq!(system.label.as_deref(), Some("Macintosh HD"), "{arch}");
        let stick = drives.iter().find(|d| d.id == id).unwrap();
        assert_eq!(stick.label.as_deref(), Some("E2EFAT"), "{arch}");
        assert_eq!(stick.state, DriveState::Unmounted, "{arch}");
        assert_eq!(stick.kind, DriveKind::Removable, "{arch}");
        assert_eq!(stick.release, DriveRelease::None, "{arch}");
        assert!(!stick.encrypted, "{arch}");
        let key = stick.key.as_deref().unwrap();
        assert_eq!(key, key.to_lowercase(), "{arch}");
    }
}

#[test]
fn a_mounted_stick_lies_under_volumes_and_ejects() {
    for (arch, id) in [("arm64", "disk7s1"), ("x86_64", "disk2s1")] {
        let file = format!("fat-mounted-info-{id}.plist");
        let drives = scenario(arch, "fat-unmounted", &[(id, file.as_str())]);
        let stick = drives.iter().find(|d| d.id == id).unwrap();
        assert_eq!(stick.state, DriveState::Mounted, "{arch}");
        assert_eq!(stick.path.as_deref(), Some("/Volumes/E2EFAT"), "{arch}");
        assert_eq!(stick.release, DriveRelease::Eject, "{arch}");
    }
}

#[test]
fn a_locked_apfs_volume_is_locked_and_keeps_its_key_once_unlocked() {
    for (arch, id) in [("arm64", "disk8s1"), ("x86_64", "disk3s1")] {
        let drives = scenario(arch, "apfs-locked", &[]);
        no_system_volumes(&drives);
        let safe = drives.iter().find(|d| d.id == id).unwrap();
        assert_eq!(safe.state, DriveState::Locked, "{arch}");
        assert!(safe.encrypted, "{arch}");
        assert_eq!(safe.kind, DriveKind::Removable, "{arch}");
        assert_eq!(safe.path, None, "{arch}");
        // The image's partition and container hold no drive of their own.
        assert_eq!(
            drives
                .iter()
                .filter(|d| d.kind != DriveKind::System)
                .count(),
            1,
            "{arch}: {drives:#?}"
        );

        let file = format!("unlocked-info-{id}.plist");
        let unlocked = scenario(arch, "apfs-locked", &[(id, file.as_str())]);
        let open = unlocked.iter().find(|d| d.id == id).unwrap();
        assert_eq!(open.state, DriveState::Mounted, "{arch}");
        assert_eq!(open.path.as_deref(), Some("/Volumes/E2EPLAIN"), "{arch}");
        assert_eq!(open.key, safe.key, "{arch}");
    }
}

#[test]
fn missing_info_means_unknown_not_an_error() {
    for arch in ["arm64", "x86_64"] {
        let dir = fixtures().join(arch).join("apfs-locked");
        let drives = classify(
            &read(&dir.join("list.plist")),
            &read(&dir.join("apfs-list.plist")),
            &HashMap::new(),
        );
        assert!(drives.iter().all(invariants_hold), "{drives:#?}");
        no_system_volumes(&drives);
        // From the list and the APFS list alone: the system drive and the locked volume.
        assert!(drives.iter().any(|d| d.kind == DriveKind::System), "{arch}");
        assert!(
            drives.iter().any(|d| d.state == DriveState::Locked),
            "{arch}: {drives:#?}"
        );
    }
    let empty: DiskList = parse(
        br#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict/></plist>"#,
    )
    .unwrap();
    assert!(classify(&empty, &ApfsList::default(), &HashMap::new()).is_empty());
}

#[test]
fn a_volume_mounted_outside_volumes_does_not_show() {
    let list = DiskList {
        disks: vec![ListDisk {
            id: "disk9".into(),
            partitions: vec![ListVolume {
                id: "disk9s1".into(),
                content: "Microsoft Basic Data".into(),
                name: Some("ELSEWHERE".into()),
                mount_point: Some("/private/tmp/elsewhere".into()),
                ..Default::default()
            }],
            ..Default::default()
        }],
    };
    assert!(classify(&list, &ApfsList::default(), &HashMap::new()).is_empty());
}
