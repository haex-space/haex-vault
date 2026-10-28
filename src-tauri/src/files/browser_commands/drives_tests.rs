use super::*;
use crate::files::local::drives::backend::fake::FakeBackend;
use crate::files::local::drives::{Drive, DriveKind, DriveRelease};
use crate::files::FilesErrorCode;

fn stick(id: &str, state: DriveState) -> Drive {
    let mounted = state == DriveState::Mounted;
    Drive {
        id: id.to_owned(),
        key: Some(format!("{id}-key")),
        label: Some(id.to_uppercase()),
        path: mounted.then(|| format!("/run/media/anna/{id}")),
        kind: DriveKind::Removable,
        state,
        release: if mounted {
            DriveRelease::Eject
        } else {
            DriveRelease::None
        },
        encrypted: state == DriveState::Locked,
        size: Some(1),
        total: None,
        free: None,
    }
}

#[test]
fn a_mounted_drive_opens_at_its_path() {
    let backend = FakeBackend::with(vec![stick("a", DriveState::Mounted)]);
    assert_eq!(
        open(&backend, "a").unwrap(),
        DriveOpened::Opened {
            path: "/run/media/anna/a".to_owned(),
            remember_failed: None
        }
    );
    assert!(backend.calls().is_empty(), "nothing mounted");
}

#[test]
fn an_unmounted_drive_is_mounted_first() {
    let backend = FakeBackend::with(vec![stick("b", DriveState::Unmounted)]);
    assert_eq!(
        open(&backend, "b").unwrap(),
        DriveOpened::Opened {
            path: "/run/media/test/b".to_owned(),
            remember_failed: None
        }
    );
    assert_eq!(backend.calls(), ["mount b"]);
}

#[test]
fn an_unknown_drive_is_gone() {
    let backend = FakeBackend::with(vec![stick("a", DriveState::Mounted)]);
    assert_eq!(open(&backend, "x").unwrap_err().code, FilesErrorCode::DriveGone);
}

#[test]
fn a_drive_pulled_while_mounting_is_gone() {
    let backend = FakeBackend::with(vec![stick("b", DriveState::Unmounted)]);
    *backend.mount_result.lock().unwrap() = Some(Err(DriveError::Gone));
    assert_eq!(open(&backend, "b").unwrap_err().code, FilesErrorCode::DriveGone);
}

#[test]
fn a_cancelled_system_prompt_is_reported_as_such() {
    let backend = FakeBackend::with(vec![stick("b", DriveState::Unmounted)]);
    *backend.mount_result.lock().unwrap() = Some(Err(DriveError::Cancelled));
    assert_eq!(open(&backend, "b").unwrap_err().code, FilesErrorCode::DriveCancelled);
}

#[test]
fn a_locked_drive_is_not_unlocked_yet() {
    let backend = FakeBackend::with(vec![stick("c", DriveState::Locked)]);
    assert_eq!(
        open(&backend, "c").unwrap_err().code,
        FilesErrorCode::DriveUnsupported
    );
}
