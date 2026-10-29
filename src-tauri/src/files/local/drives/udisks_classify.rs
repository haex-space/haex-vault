//! Which udisks2 objects are drives in the sidebar, and what kind and state they have (spec 064
//! research R2, R3), as a pure function over a copy of their properties. The rule follows gvfs's
//! `should_include_volume`: `HintIgnore` hides, `HintSystem` does not (udisks sets it on every
//! internal disk).

use std::collections::HashMap;
use std::path::Path;

use super::mountinfo::is_shown_mount_point;
use super::{Drive, DriveKind, DriveRelease, DriveState};

/// `org.freedesktop.UDisks2.Block`, the properties the list needs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BlockProps {
    pub label: String,
    pub uuid: String,
    pub hint_ignore: bool,
    pub hint_system: bool,
    /// The encrypted block this one is the unlocked content of.
    pub crypto_backing: Option<String>,
    pub size: u64,
    /// The `Drive` object the block belongs to; none for loop and device-mapper blocks.
    pub drive: Option<String>,
}

/// `org.freedesktop.UDisks2.Filesystem`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FsProps {
    pub mount_points: Vec<String>,
}

/// `org.freedesktop.UDisks2.Encrypted`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EncProps {
    /// The unlocked block; none while locked.
    pub cleartext: Option<String>,
}

/// `org.freedesktop.UDisks2.Drive`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DriveProps {
    pub removable: bool,
    pub can_power_off: bool,
    pub ejectable: bool,
}

/// One object of `GetManagedObjects` with the interfaces the list reads.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UdisksObject {
    pub path: String,
    pub block: Option<BlockProps>,
    pub filesystem: Option<FsProps>,
    pub encrypted: Option<EncProps>,
    pub drive: Option<DriveProps>,
}

/// The drives among `objects`, unsorted; space stays unknown (the caller asks the file system).
pub fn classify(objects: &[UdisksObject]) -> Vec<Drive> {
    let by_path: HashMap<&str, &UdisksObject> = objects
        .iter()
        .map(|object| (object.path.as_str(), object))
        .collect();
    objects
        .iter()
        .filter_map(|object| classify_one(object, &by_path))
        .collect()
}

fn classify_one(object: &UdisksObject, by_path: &HashMap<&str, &UdisksObject>) -> Option<Drive> {
    let block = object.block.as_ref()?;
    if block.hint_ignore {
        return None;
    }
    if let Some(encrypted) = &object.encrypted {
        // Unlocked, the content block stands for it, if it holds a file system.
        if encrypted.cleartext.is_some() {
            return None;
        }
        return Some(Drive {
            id: object.path.clone(),
            key: key_of(&block.uuid),
            label: label_of(&block.label),
            path: None,
            kind: removable_or_internal(block, by_path),
            state: DriveState::Locked,
            release: DriveRelease::None,
            encrypted: true,
            size: Some(block.size),
            total: None,
            free: None,
        });
    }
    let filesystem = object.filesystem.as_ref()?;
    let backing = block
        .crypto_backing
        .as_deref()
        .and_then(|path| by_path.get(path))
        .and_then(|backing| backing.block.as_ref());
    if backing.is_some_and(|backing| backing.hint_ignore) {
        return None;
    }
    let mount = filesystem.mount_points.first();
    if mount.is_some_and(|point| !is_shown_mount_point(Path::new(point))) {
        return None;
    }
    let outer = backing.unwrap_or(block);
    let kind = if mount.is_some_and(|point| point == "/") {
        DriveKind::System
    } else {
        removable_or_internal(outer, by_path)
    };
    let release = match (mount, kind) {
        (None, _) | (_, DriveKind::System) => DriveRelease::None,
        (Some(_), DriveKind::Removable) => DriveRelease::Eject,
        (Some(_), DriveKind::Internal) => DriveRelease::Unmount,
    };
    Some(Drive {
        id: object.path.clone(),
        key: key_of(&outer.uuid),
        label: label_of(&block.label).or_else(|| backing.and_then(|b| label_of(&b.label))),
        path: mount.cloned(),
        kind,
        state: if mount.is_some() {
            DriveState::Mounted
        } else {
            DriveState::Unmounted
        },
        release,
        encrypted: backing.is_some(),
        size: Some(block.size),
        total: None,
        free: None,
    })
}

/// How the drive is switched off after the unmount.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Power {
    /// `Drive.PowerOff` on this drive object.
    Off(String),
    /// `Drive.Eject` on this drive object.
    Eject(String),
}

/// What releasing a mounted drive calls, in the order of contracts/commands.md.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseSteps {
    /// The block whose file system is unmounted.
    pub unmount: String,
    /// The encrypted block that is locked after it (FR-011).
    pub lock: Option<String>,
    /// For a removable drive whose other file systems are not mounted.
    pub power: Option<Power>,
}

/// The steps that release `drive`, one of `classify(objects)`; `None` when it is gone.
pub fn release_steps(objects: &[UdisksObject], drive: &Drive) -> Option<ReleaseSteps> {
    let by_path: HashMap<&str, &UdisksObject> = objects
        .iter()
        .map(|object| (object.path.as_str(), object))
        .collect();
    let block = by_path.get(drive.id.as_str())?.block.as_ref()?;
    let lock = block.crypto_backing.clone().filter(|_| drive.encrypted);
    let power = drive_of(block, &by_path)
        .filter(|_| drive.release == DriveRelease::Eject)
        .filter(|(path, _)| !others_mounted(objects, &by_path, path, &drive.id))
        .and_then(|(path, props)| {
            if props.can_power_off {
                Some(Power::Off(path.to_owned()))
            } else {
                props.ejectable.then(|| Power::Eject(path.to_owned()))
            }
        });
    Some(ReleaseSteps {
        unmount: drive.id.clone(),
        lock,
        power,
    })
}

/// The drive object of `block`, through its encrypted block for an unlocked one.
fn drive_of<'a>(
    block: &BlockProps,
    by_path: &HashMap<&str, &'a UdisksObject>,
) -> Option<(&'a str, &'a DriveProps)> {
    let outer = block
        .crypto_backing
        .as_deref()
        .and_then(|path| by_path.get(path))
        .and_then(|backing| backing.block.as_ref())
        .unwrap_or(block);
    let object = by_path.get(outer.drive.as_deref()?)?;
    Some((object.path.as_str(), object.drive.as_ref()?))
}

/// Whether another file system of the drive object `drive` is mounted: switching it off would
/// pull that one too.
fn others_mounted(
    objects: &[UdisksObject],
    by_path: &HashMap<&str, &UdisksObject>,
    drive: &str,
    except: &str,
) -> bool {
    objects.iter().any(|object| {
        object.path != except
            && object
                .filesystem
                .as_ref()
                .is_some_and(|fs| !fs.mount_points.is_empty())
            && object
                .block
                .as_ref()
                .and_then(|block| drive_of(block, by_path))
                .is_some_and(|(path, _)| path == drive)
    })
}

/// Removable when its drive says so, or when it has no drive and is no system block (a loop image).
fn removable_or_internal(block: &BlockProps, by_path: &HashMap<&str, &UdisksObject>) -> DriveKind {
    let drive = block
        .drive
        .as_deref()
        .and_then(|path| by_path.get(path))
        .and_then(|object| object.drive.as_ref());
    match drive {
        Some(drive) if drive.removable => DriveKind::Removable,
        Some(_) => DriveKind::Internal,
        None if !block.hint_system => DriveKind::Removable,
        None => DriveKind::Internal,
    }
}

fn key_of(uuid: &str) -> Option<String> {
    (!uuid.is_empty()).then(|| uuid.to_lowercase())
}

fn label_of(label: &str) -> Option<String> {
    (!label.is_empty()).then(|| label.to_owned())
}

#[cfg(test)]
#[path = "udisks_classify_tests.rs"]
mod tests;
