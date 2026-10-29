//! `/proc/self/mountinfo` on Linux (spec 044 research R6): the mounted file systems that hold the
//! user's files, skipping virtual ones. The drive list falls back to it without udisks2 (spec 064
//! FR-024), and the udisks2 list uses [`is_user_drive`] for mounted drives (research R2).

use std::path::{Path, PathBuf};

/// File system types that never hold the user's files (research R6).
const VIRTUAL_TYPES: &[&str] = &[
    "proc",
    "sysfs",
    "devtmpfs",
    "devpts",
    "cgroup",
    "cgroup2",
    "securityfs",
    "debugfs",
    "tracefs",
    "configfs",
    "fusectl",
    "pstore",
    "bpf",
    "mqueue",
    "hugetlbfs",
    "autofs",
    "binfmt_misc",
    "efivarfs",
    "selinuxfs",
    "nsfs",
    "rpc_pipefs",
    "ramfs",
    "tmpfs",
    "overlay",
    "squashfs",
    "fuse.portal",
    "fuse.gvfsd-fuse",
];

/// One mount of `/proc/self/mountinfo`: its mount point and file system type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mount {
    pub point: PathBuf,
    pub fs_type: String,
}

/// The mounts of a `mountinfo` text: the mount point is field 5 (octal escapes such as `\040`
/// decoded), the type is the first field after ` - `.
pub fn parse_mountinfo(text: &str) -> Vec<Mount> {
    text.lines()
        .filter_map(|line| {
            let (left, right) = line.split_once(" - ")?;
            let point = left.split(' ').nth(4)?;
            let fs_type = right.split(' ').next()?;
            Some(Mount {
                point: PathBuf::from(unescape(point)),
                fs_type: fs_type.to_owned(),
            })
        })
        .collect()
}

/// `\040`, `\011`, `\012`, `\134` and other three-digit octal escapes of mountinfo.
fn unescape(field: &str) -> String {
    let bytes = field.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let octal = bytes
            .get(i + 1..i + 4)
            .filter(|digits| bytes[i] == b'\\' && digits.iter().all(|b| (b'0'..=b'7').contains(b)));
        match octal {
            Some(digits) => {
                let value = digits
                    .iter()
                    .fold(0u32, |acc, b| acc * 8 + u32::from(b - b'0'));
                out.push(u8::try_from(value).unwrap_or(b'?'));
                i += 4;
            }
            None => {
                out.push(bytes[i]);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Whether a Linux mount is a drive the user would look into: not virtual, and the root or a
/// removable/extra mount under the usual places.
pub fn is_user_drive(mount: &Mount) -> bool {
    if VIRTUAL_TYPES.contains(&mount.fs_type.as_str()) {
        return false;
    }
    is_user_mount_point(&mount.point)
}

/// Whether a mount point is the root or lies under the places the user's drives are mounted at.
pub fn is_user_mount_point(point: &Path) -> bool {
    point == Path::new("/")
        || ["/media", "/run/media", "/mnt", "/home"]
            .iter()
            .any(|base| point.starts_with(base))
}

/// Whether the drive list shows a drive mounted at `point`: a user mount point, but not `/home`
/// itself, which is part of the system (spec 064 FR-001).
pub fn is_shown_mount_point(point: &Path) -> bool {
    is_user_mount_point(point) && point != Path::new("/home")
}

/// The mount points of this Linux device the drive list shows, each once.
pub fn user_mount_points(text: &str) -> Vec<PathBuf> {
    let mut seen = std::collections::BTreeSet::new();
    parse_mountinfo(text)
        .into_iter()
        .filter(is_user_drive)
        .filter(|mount| is_shown_mount_point(&mount.point))
        .map(|mount| mount.point)
        .filter(|point| seen.insert(point.clone()))
        .collect()
}

#[cfg(test)]
#[path = "mountinfo_tests.rs"]
mod tests;
