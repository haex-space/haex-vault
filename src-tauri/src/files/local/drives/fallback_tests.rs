use std::path::PathBuf;

use super::*;
use crate::files::local::drives::{invariants_hold, DriveRelease, DriveState};

#[test]
fn the_root_is_the_system_drive_and_others_are_internal() {
    let drives = drives_at(vec![PathBuf::from("/"), PathBuf::from("/mnt/backup")]);
    assert_eq!(drives[0].kind, DriveKind::System);
    assert_eq!(drives[1].kind, DriveKind::Internal);
    assert_eq!(drives[1].id, "/mnt/backup");
    assert_eq!(drives[1].path.as_deref(), Some("/mnt/backup"));
}

#[test]
fn fallback_drives_are_mounted_without_buttons() {
    for drive in drives_at(vec![PathBuf::from("/"), PathBuf::from("/media/x")]) {
        assert_eq!(drive.state, DriveState::Mounted);
        assert_eq!(drive.release, DriveRelease::None);
        assert_eq!(drive.key, None);
        assert!(invariants_hold(&drive), "{drive:?}");
    }
}

#[test]
fn nothing_can_be_mounted_or_ejected() {
    assert_eq!(FallbackBackend.mount("/"), Err(DriveError::Unsupported));
    assert_eq!(FallbackBackend.release("/"), Err(DriveError::Unsupported));
}
