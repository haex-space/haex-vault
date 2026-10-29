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
