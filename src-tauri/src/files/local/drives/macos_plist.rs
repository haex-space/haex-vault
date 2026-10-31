//! Which `diskutil` volumes are drives in the sidebar, and what kind and state they have (spec 064
//! research R5), as a pure function over the plists of `diskutil list -plist`,
//! `diskutil apfs list -plist` and `diskutil info -plist <id>`. Apple documents none of their keys:
//! a missing key means "unknown", never an error. The fixtures were recorded on macOS 26 (Apple
//! silicon) and macOS 15 (Intel).

use std::collections::HashMap;

use serde::de::DeserializeOwned;
use serde::Deserialize;

use super::{Drive, DriveKind, DriveRelease, DriveState};

/// `diskutil list -plist`: every disk with its partitions or APFS volumes.
#[derive(Debug, Default, Deserialize)]
pub struct DiskList {
    #[serde(rename = "AllDisksAndPartitions", default)]
    pub disks: Vec<ListDisk>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ListDisk {
    #[serde(rename = "DeviceIdentifier", default)]
    pub id: String,
    #[serde(rename = "Content", default)]
    pub content: String,
    /// The system's own containers (iBoot, cryptexes): never a drive.
    #[serde(rename = "OSInternal", default)]
    pub os_internal: bool,
    #[serde(rename = "Partitions", default)]
    pub partitions: Vec<ListVolume>,
    #[serde(rename = "APFSVolumes", default)]
    pub apfs_volumes: Vec<ListVolume>,
    #[serde(rename = "VolumeName")]
    pub name: Option<String>,
    #[serde(rename = "MountPoint")]
    pub mount_point: Option<String>,
    #[serde(rename = "VolumeUUID")]
    pub uuid: Option<String>,
    #[serde(rename = "Size")]
    pub size: Option<u64>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ListVolume {
    #[serde(rename = "DeviceIdentifier", default)]
    pub id: String,
    #[serde(rename = "Content", default)]
    pub content: String,
    #[serde(rename = "VolumeName")]
    pub name: Option<String>,
    #[serde(rename = "MountPoint")]
    pub mount_point: Option<String>,
    #[serde(rename = "VolumeUUID")]
    pub uuid: Option<String>,
    #[serde(rename = "Size")]
    pub size: Option<u64>,
}

/// `diskutil apfs list -plist`: the roles and the lock state of every APFS volume.
#[derive(Debug, Default, Deserialize)]
pub struct ApfsList {
    #[serde(rename = "Containers", default)]
    pub containers: Vec<ApfsContainer>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ApfsContainer {
    #[serde(rename = "Volumes", default)]
    pub volumes: Vec<ApfsVolume>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ApfsVolume {
    #[serde(rename = "DeviceIdentifier", default)]
    pub id: String,
    #[serde(rename = "Roles", default)]
    pub roles: Vec<String>,
    #[serde(rename = "Locked")]
    pub locked: Option<bool>,
    #[serde(rename = "Encryption")]
    pub encryption: Option<bool>,
    #[serde(rename = "APFSVolumeUUID")]
    pub uuid: Option<String>,
}

/// `diskutil info -plist <id>` of one volume.
#[derive(Debug, Default, Deserialize)]
pub struct VolumeInfo {
    #[serde(rename = "Content")]
    pub content: Option<String>,
    #[serde(rename = "VolumeName")]
    pub name: Option<String>,
    #[serde(rename = "MountPoint")]
    pub mount_point: Option<String>,
    #[serde(rename = "FilesystemType")]
    pub filesystem: Option<String>,
    #[serde(rename = "VolumeUUID")]
    pub uuid: Option<String>,
    #[serde(rename = "Internal")]
    pub internal: Option<bool>,
    #[serde(rename = "RemovableMediaOrExternalDevice")]
    pub removable_or_external: Option<bool>,
    #[serde(rename = "Locked")]
    pub locked: Option<bool>,
    #[serde(rename = "Encryption")]
    pub encryption: Option<bool>,
    #[serde(rename = "Size")]
    pub size: Option<u64>,
    /// The whole disk to eject: the partition table's disk, or the APFS container.
    #[serde(rename = "ParentWholeDisk")]
    pub whole_disk: Option<String>,
}

/// Reads one of the plists above.
pub fn parse<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, plist::Error> {
    plist::from_bytes(bytes)
}

/// APFS roles of volumes nobody opens (research R5). `Data` stands behind `/` through the firmlink.
const HIDDEN_ROLES: [&str; 7] = [
    "Preboot", "Recovery", "VM", "Update", "Hardware", "xART", "Data",
];

/// Partition contents that hold no file system of their own: the EFI partition and the physical
/// stores of APFS containers, whose volumes are listed on the container.
fn is_store_or_efi(content: &str) -> bool {
    content == "EFI" || content == "Apple_Boot" || content.starts_with("Apple_APFS")
}

/// The device identifiers `classify` looks at: the volumes of every disk the system does not keep
/// for itself, or the disk itself when it has a file system without a partition table.
pub fn volume_ids(list: &DiskList) -> Vec<&str> {
    list.disks
        .iter()
        .filter(|disk| !disk.os_internal)
        .flat_map(|disk| {
            if disk.partitions.is_empty() && disk.apfs_volumes.is_empty() {
                vec![disk.id.as_str()]
            } else {
                disk.partitions
                    .iter()
                    .chain(&disk.apfs_volumes)
                    .map(|volume| volume.id.as_str())
                    .collect()
            }
        })
        .collect()
}

/// The volumes of `list` the sidebar shows, unsorted; `infos` holds `diskutil info` by device
/// identifier, any of which may be missing. Space stays unknown (the caller asks the file system).
pub fn classify(
    list: &DiskList,
    apfs: &ApfsList,
    infos: &HashMap<String, VolumeInfo>,
) -> Vec<Drive> {
    let apfs_by_id: HashMap<&str, &ApfsVolume> = apfs
        .containers
        .iter()
        .flat_map(|container| &container.volumes)
        .map(|volume| (volume.id.as_str(), volume))
        .collect();
    let mut drives = Vec::new();
    for disk in list.disks.iter().filter(|disk| !disk.os_internal) {
        if disk.partitions.is_empty() && disk.apfs_volumes.is_empty() {
            // A file system on the whole disk, without a partition table.
            let whole = ListVolume {
                id: disk.id.clone(),
                content: disk.content.clone(),
                name: disk.name.clone(),
                mount_point: disk.mount_point.clone(),
                uuid: disk.uuid.clone(),
                size: disk.size,
            };
            drives.extend(classify_one(&whole, None, infos.get(&disk.id)));
            continue;
        }
        for volume in disk.partitions.iter().chain(&disk.apfs_volumes) {
            let apfs = apfs_by_id.get(volume.id.as_str()).copied();
            drives.extend(classify_one(volume, apfs, infos.get(&volume.id)));
        }
    }
    drives
}

fn classify_one(
    volume: &ListVolume,
    apfs: Option<&ApfsVolume>,
    info: Option<&VolumeInfo>,
) -> Option<Drive> {
    let content = info
        .and_then(|info| info.content.as_deref())
        .unwrap_or(&volume.content);
    if is_store_or_efi(content) {
        return None;
    }
    let roles = apfs.map(|apfs| apfs.roles.as_slice()).unwrap_or_default();
    if roles
        .iter()
        .any(|role| HIDDEN_ROLES.contains(&role.as_str()))
    {
        return None;
    }
    let mount = non_empty(info.and_then(|info| info.mount_point.as_deref()))
        .or_else(|| non_empty(volume.mount_point.as_deref()));
    let system = mount == Some("/");
    // The system volume shows as `/` only, through its mounted snapshot.
    if roles.iter().any(|role| role == "System") && !system {
        return None;
    }
    if mount.is_some_and(|point| !system && !point.starts_with("/Volumes/")) {
        return None;
    }
    let locked = info
        .and_then(|info| info.locked)
        .or_else(|| apfs.and_then(|apfs| apfs.locked))
        .unwrap_or(false);
    let has_filesystem =
        apfs.is_some() || non_empty(info.and_then(|info| info.filesystem.as_deref())).is_some();
    if mount.is_none() && !locked && !has_filesystem {
        return None;
    }
    let kind = if system {
        DriveKind::System
    } else if info.and_then(|info| info.internal) == Some(false)
        || info.and_then(|info| info.removable_or_external) == Some(true)
    {
        DriveKind::Removable
    } else {
        DriveKind::Internal
    };
    let state = match (mount, locked) {
        (Some(_), _) => DriveState::Mounted,
        (None, true) => DriveState::Locked,
        (None, false) => DriveState::Unmounted,
    };
    let release = match (state, kind) {
        (DriveState::Mounted, DriveKind::Removable) => DriveRelease::Eject,
        (DriveState::Mounted, DriveKind::Internal) => DriveRelease::Unmount,
        _ => DriveRelease::None,
    };
    let key = non_empty(info.and_then(|info| info.uuid.as_deref()))
        .or_else(|| non_empty(volume.uuid.as_deref()))
        .or_else(|| non_empty(apfs.and_then(|apfs| apfs.uuid.as_deref())))
        .map(str::to_lowercase);
    let label = non_empty(info.and_then(|info| info.name.as_deref()))
        .or_else(|| non_empty(volume.name.as_deref()))
        .map(str::to_owned);
    let encrypted = info
        .and_then(|info| info.encryption)
        .or_else(|| apfs.and_then(|apfs| apfs.encryption))
        .unwrap_or(false);
    Some(Drive {
        id: volume.id.clone(),
        key,
        label,
        path: mount.map(str::to_owned),
        kind,
        state,
        release,
        encrypted,
        size: info.and_then(|info| info.size).or(volume.size),
        total: None,
        free: None,
    })
}

fn non_empty(text: Option<&str>) -> Option<&str> {
    text.filter(|text| !text.is_empty())
}

#[cfg(test)]
#[path = "macos_plist_tests.rs"]
mod tests;
