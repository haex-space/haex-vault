//! The drives of this device (spec 044 FR-002, spec 064). [`volumes`] lists every drive with its
//! state from the platform's [`backend::DriveBackend`]: mounted, not mounted or locked. [`drives`]
//! keeps only the mounted ones; agents and other own devices see nothing else (spec 064 research
//! R11).

use std::path::Path;

use serde::Serialize;
use ts_rs::TS;

pub mod android;
pub mod backend;
pub mod fallback;
pub mod monitor;
pub mod mountinfo;
#[cfg(all(target_os = "linux", not(target_os = "android")))]
pub mod udisks;
#[cfg(all(target_os = "linux", not(target_os = "android")))]
pub mod udisks_classify;

/// What kind of drive it is; picks the icon and the order (spec 064 FR-002, FR-003).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum DriveKind {
    /// The drive the system runs from.
    System,
    Internal,
    /// USB sticks, external disks, SD cards.
    Removable,
}

/// Whether the content of a drive can be reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum DriveState {
    Mounted,
    Unmounted,
    /// Encrypted and not unlocked yet.
    Locked,
}

/// Which button the sidebar shows beside a drive (spec 064 FR-008).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum DriveRelease {
    None,
    Unmount,
    /// Unmount, then power off or eject the device.
    Eject,
}

/// A drive in the sidebar (spec 064 data-model.md).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct Drive {
    /// The backend's handle for commands in this session; never stored.
    pub id: String,
    /// What stays the same across sessions and devices: the UUID of the encrypted container, else of
    /// the file system, lowercase (research R3).
    pub key: Option<String>,
    pub label: Option<String>,
    /// The mount path; set exactly when mounted.
    pub path: Option<String>,
    pub kind: DriveKind,
    pub state: DriveState,
    pub release: DriveRelease,
    /// An encrypted container lies below, locked or not.
    pub encrypted: bool,
    #[ts(type = "number | null")]
    pub size: Option<u64>,
    #[ts(type = "number | null")]
    pub total: Option<u64>,
    #[ts(type = "number | null")]
    pub free: Option<u64>,
}

impl Drive {
    /// A mounted drive at `path` with what the file system says about its space.
    pub fn mounted(id: String, path: &Path, kind: DriveKind) -> Self {
        let (total, free) = space(path).unzip();
        Self {
            id,
            key: None,
            label: None,
            path: Some(path.to_string_lossy().into_owned()),
            kind,
            state: DriveState::Mounted,
            release: DriveRelease::None,
            encrypted: false,
            size: total,
            total,
            free,
        }
    }
}

/// The name agents and other own devices see: the label, else the last part of the path as before
/// spec 064, else the id.
pub fn display_name(drive: &Drive) -> String {
    if let Some(label) = drive.label.as_deref().filter(|label| !label.is_empty()) {
        return label.to_owned();
    }
    drive
        .path
        .as_deref()
        .map(Path::new)
        .map(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| drive.id.clone())
}

/// System, internal, removable; then by label ignoring case, missing labels last; then by id.
pub fn sort_drives(drives: &mut [Drive]) {
    drives.sort_by(|a, b| {
        a.kind
            .cmp(&b.kind)
            .then_with(|| match (&a.label, &b.label) {
                (Some(x), Some(y)) => x.to_lowercase().cmp(&y.to_lowercase()),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            })
            .then_with(|| a.id.cmp(&b.id))
    });
}

/// Only the mounted drives, in the given order.
pub fn mounted_only(drives: Vec<Drive>) -> Vec<Drive> {
    drives
        .into_iter()
        .filter(|drive| drive.state == DriveState::Mounted && drive.path.is_some())
        .collect()
}

/// Every drive of this device with its state, sorted. A backend that cannot list falls back to the
/// mounted drives the system reports (FR-024).
pub fn volumes() -> Vec<Drive> {
    current(&*backend::backend())
}

/// What `backend` lists, or the mounted drives when it cannot list; sorted.
pub fn current(backend: &dyn backend::DriveBackend) -> Vec<Drive> {
    let mut drives = match backend.list() {
        Ok(drives) => drives,
        Err(error) => {
            static LIST_FAILURE: std::sync::Once = std::sync::Once::new();
            LIST_FAILURE.call_once(|| {
                log::warn!(
                    "files: listing drives failed, showing the mounted ones only: {error:?}"
                );
            });
            fallback::mounted_drives()
        }
    };
    sort_drives(&mut drives);
    drives
}

/// The mounted drives of this device, the only ones agents and other own devices see.
pub fn drives() -> Vec<Drive> {
    mounted_only(volumes())
}

/// Total and free bytes of the file system holding `path`; `None` where unknown (Windows for now).
pub fn space(path: &Path) -> Option<(u64, u64)> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let c_path = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
        // SAFETY: `stats` is plain data the call fills; `c_path` is NUL-terminated and outlives it.
        let mut stats: libc::statvfs = unsafe { std::mem::zeroed() };
        if unsafe { libc::statvfs(c_path.as_ptr(), &mut stats) } != 0 {
            return None;
        }
        #[allow(clippy::unnecessary_cast)]
        let block = stats.f_frsize as u64;
        #[allow(clippy::unnecessary_cast)]
        Some((stats.f_blocks as u64 * block, stats.f_bavail as u64 * block))
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        None
    }
}

/// The rules every drive keeps (data-model.md "Invarianten"); for the tests of each backend.
#[cfg(test)]
pub(crate) fn invariants_hold(drive: &Drive) -> bool {
    let mounted = drive.state == DriveState::Mounted;
    mounted == drive.path.is_some()
        && (drive.kind != DriveKind::System || drive.release == DriveRelease::None)
        && (mounted
            || (drive.release == DriveRelease::None
                && drive.total.is_none()
                && drive.free.is_none()))
        && (drive.state != DriveState::Locked || drive.encrypted)
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
