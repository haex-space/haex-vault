//! The mounted drives as the system reports them, without a drive service (spec 064 FR-024): on
//! Linux the mounts of `/proc/self/mountinfo`, on macOS `/` and `/Volumes`, on Windows the drive
//! letters. Every drive is mounted, nothing can be mounted, unlocked or ejected. macOS and Windows
//! use this until their own backends (spec 064 PR E, PR F).

use std::path::{Path, PathBuf};

use super::backend::{DriveBackend, DriveError};
use super::{Drive, DriveKind};

pub struct FallbackBackend;

impl DriveBackend for FallbackBackend {
    fn list(&self) -> Result<Vec<Drive>, DriveError> {
        Ok(mounted_drives())
    }
}

/// The mounted drives, the root as the system drive.
pub fn mounted_drives() -> Vec<Drive> {
    drives_at(mount_points())
}

fn drives_at(points: Vec<PathBuf>) -> Vec<Drive> {
    points
        .into_iter()
        .map(|point| {
            let kind = if point == Path::new("/") {
                DriveKind::System
            } else {
                DriveKind::Internal
            };
            Drive::mounted(point.to_string_lossy().into_owned(), &point, kind)
        })
        .collect()
}

#[cfg(target_os = "linux")]
fn mount_points() -> Vec<PathBuf> {
    let text = std::fs::read_to_string("/proc/self/mountinfo").unwrap_or_default();
    super::mountinfo::user_mount_points(&text)
}

#[cfg(target_os = "macos")]
fn mount_points() -> Vec<PathBuf> {
    let mut paths = vec![PathBuf::from("/")];
    if let Ok(read) = std::fs::read_dir("/Volumes") {
        let mut volumes: Vec<PathBuf> = read
            .filter_map(Result::ok)
            .map(|item| item.path())
            // The boot volume appears in /Volumes as a link to `/`.
            .filter(|path| std::fs::canonicalize(path).map_or(true, |real| real != Path::new("/")))
            .collect();
        volumes.sort();
        paths.extend(volumes);
    }
    paths
}

#[cfg(windows)]
fn mount_points() -> Vec<PathBuf> {
    (b'A'..=b'Z')
        .map(|letter| PathBuf::from(format!("{}:\\", letter as char)))
        .filter(|path| path.exists())
        .collect()
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn mount_points() -> Vec<PathBuf> {
    vec![PathBuf::from("/")]
}

#[cfg(test)]
#[path = "fallback_tests.rs"]
mod tests;
