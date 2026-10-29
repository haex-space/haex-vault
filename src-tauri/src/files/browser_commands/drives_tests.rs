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
    assert_eq!(
        open(&backend, "x").unwrap_err().code,
        FilesErrorCode::DriveGone
    );
}

#[test]
fn a_drive_pulled_while_mounting_is_gone() {
    let backend = FakeBackend::with(vec![stick("b", DriveState::Unmounted)]);
    *backend.mount_result.lock().unwrap() = Some(Err(DriveError::Gone));
    assert_eq!(
        open(&backend, "b").unwrap_err().code,
        FilesErrorCode::DriveGone
    );
}

#[test]
fn a_cancelled_system_prompt_is_reported_as_such() {
    let backend = FakeBackend::with(vec![stick("b", DriveState::Unmounted)]);
    *backend.mount_result.lock().unwrap() = Some(Err(DriveError::Cancelled));
    assert_eq!(
        open(&backend, "b").unwrap_err().code,
        FilesErrorCode::DriveCancelled
    );
}

#[test]
fn a_locked_drive_is_not_unlocked_yet() {
    let backend = FakeBackend::with(vec![stick("c", DriveState::Locked)]);
    assert_eq!(
        open(&backend, "c").unwrap_err().code,
        FilesErrorCode::DriveUnsupported
    );
}

/// Holders that write what they are asked into the backend's record, so one list shows the order.
struct FakeHolders<'a> {
    calls: &'a std::sync::Mutex<Vec<String>>,
    transfers: usize,
}

impl Holders for FakeHolders<'_> {
    fn transfers(&self, _prefix: &Path) -> usize {
        self.transfers
    }

    fn cancel_transfers(&self, prefix: &Path) {
        self.calls
            .lock()
            .unwrap()
            .push(format!("cancel transfers {}", prefix.display()));
    }

    fn release_under(&self, prefix: &Path) {
        self.calls
            .lock()
            .unwrap()
            .push(format!("release under {}", prefix.display()));
    }
}

fn holders(backend: &FakeBackend, transfers: usize) -> FakeHolders<'_> {
    FakeHolders {
        calls: &backend.calls,
        transfers,
    }
}

#[test]
fn a_release_frees_haex_s_own_handles_before_the_system_releases() {
    let backend = FakeBackend::with(vec![stick("a", DriveState::Mounted)]);
    assert_eq!(
        release(&backend, &holders(&backend, 1), "a", true).unwrap(),
        DriveReleased::Released { ejected: true }
    );
    assert_eq!(
        backend.calls(),
        [
            "cancel transfers /run/media/anna/a",
            "release under /run/media/anna/a",
            "release a"
        ]
    );
}

#[test]
fn running_transfers_are_reported_and_nothing_happens() {
    let backend = FakeBackend::with(vec![stick("a", DriveState::Mounted)]);
    assert_eq!(
        release(&backend, &holders(&backend, 2), "a", false).unwrap(),
        DriveReleased::TransfersRunning { count: 2 }
    );
    assert!(backend.calls().is_empty());
}

#[test]
fn without_transfers_no_question_is_needed() {
    let backend = FakeBackend::with(vec![stick("a", DriveState::Mounted)]);
    *backend.release_result.lock().unwrap() = Some(Ok(false));
    assert_eq!(
        release(&backend, &holders(&backend, 0), "a", false).unwrap(),
        DriveReleased::Released { ejected: false }
    );
    assert_eq!(
        backend.calls().last().map(String::as_str),
        Some("release a")
    );
}

#[test]
fn an_unknown_or_unmounted_drive_is_not_released() {
    let backend = FakeBackend::with(vec![stick("b", DriveState::Unmounted)]);
    let code = |id| {
        release(&backend, &holders(&backend, 0), id, true)
            .unwrap_err()
            .code
    };
    assert_eq!(code("x"), FilesErrorCode::DriveGone);
    assert_eq!(code("b"), FilesErrorCode::DriveUnsupported);
    assert!(backend.calls().is_empty());
}

#[test]
fn a_drive_another_program_holds_is_busy() {
    let backend = FakeBackend::with(vec![stick("a", DriveState::Mounted)]);
    *backend.release_result.lock().unwrap() = Some(Err(DriveError::Busy));
    assert_eq!(
        release(&backend, &holders(&backend, 0), "a", true)
            .unwrap_err()
            .code,
        FilesErrorCode::DriveBusy
    );
}
