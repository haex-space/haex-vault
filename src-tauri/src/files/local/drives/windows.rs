//! The drives of Windows (spec 064 research R6): the volumes with a drive letter, their kind from
//! the storage bus, ejecting through the PnP manager with a veto buffer (so Windows shows no dialog
//! of its own and says why it refused), and BitLocker To Go through the system's own unlock dialog
//! (FR-019). Volumes without a letter stay hidden; internal drives offer no unmount, which would
//! need an administrator. The pure parts come first, so their tests run on every platform.

use super::backend::DriveError;
use super::{Drive, DriveKind, DriveRelease, DriveState};

/// What `GetVolumeInformationW` fails with on a locked BitLocker volume, also for a standard user
/// (spike T070).
pub const FVE_E_LOCKED_VOLUME: u32 = 0x8031_0000;

/// `STORAGE_BUS_TYPE` values of removable media: USB, SD, MMC.
const REMOVABLE_BUSES: [i32; 3] = [7, 12, 13];

/// The paths of `GetVolumePathNamesForVolumeNameW`: strings separated by NUL, ending in two.
pub fn path_names(buffer: &[u16]) -> Vec<String> {
    buffer
        .split(|&unit| unit == 0)
        .filter(|name| !name.is_empty())
        .map(String::from_utf16_lossy)
        .collect()
}

/// A drive root such as `E:\`, the only kind of path a drive is shown at.
pub fn is_drive_root(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() == 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'\\'
}

/// System for the Windows directory's drive, removable for USB, SD and MMC or removable media,
/// internal otherwise. Without a bus (the query failed) it is internal.
pub fn kind_of(bus: Option<i32>, removable_media: bool, system: bool) -> DriveKind {
    if system {
        DriveKind::System
    } else if removable_media || bus.is_some_and(|bus| REMOVABLE_BUSES.contains(&bus)) {
        DriveKind::Removable
    } else {
        DriveKind::Internal
    }
}

/// The volume serial as the stable key, lowercase hex.
pub fn key_of(serial: u32) -> String {
    format!("{serial:08x}")
}

/// What the backend learned about one lettered volume.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VolumeFacts {
    /// `E:\`; also the drive's id.
    pub root: String,
    pub label: Option<String>,
    pub serial: Option<u32>,
    pub locked: bool,
    pub bus: Option<i32>,
    pub removable_media: bool,
    pub system: bool,
    /// Total and free bytes.
    pub space: Option<(u64, u64)>,
}

/// The drive the sidebar shows for `facts`. Every lettered volume is mounted unless BitLocker
/// keeps it locked.
pub fn drive_of(facts: VolumeFacts) -> Drive {
    let kind = kind_of(facts.bus, facts.removable_media, facts.system);
    let state = if facts.locked {
        DriveState::Locked
    } else {
        DriveState::Mounted
    };
    let (total, free) = facts.space.unzip();
    Drive {
        id: facts.root.clone(),
        key: facts.serial.map(key_of),
        label: facts.label.filter(|label| !label.is_empty()),
        path: (!facts.locked).then(|| facts.root.clone()),
        kind,
        state,
        release: if kind == DriveKind::Removable && state == DriveState::Mounted {
            DriveRelease::Eject
        } else {
            DriveRelease::None
        },
        encrypted: facts.locked,
        size: total,
        total,
        free,
    }
}

/// What a `PNP_VETO_TYPE` from `CM_Request_Device_EjectW` means; `None` for no veto. `name` is the
/// vetoing program or device, for the log.
pub fn veto_error(veto: i32, name: &str) -> Option<DriveError> {
    match veto {
        0 => None,
        // Pending close, an application, a service, an open handle, another device or driver.
        1..=7 | 11 => {
            log::info!("files: ejecting refused by {name:?} (veto {veto})");
            Some(DriveError::Busy)
        }
        13 => Some(DriveError::Gone),
        _ => Some(DriveError::Failed(format!(
            "Windows did not eject it (veto {veto}: {name})"
        ))),
    }
}

#[cfg(windows)]
#[path = "windows/system.rs"]
mod system;

#[cfg(windows)]
pub use system::WindowsBackend;
#[cfg(test)]
#[path = "windows_tests.rs"]
mod tests;
