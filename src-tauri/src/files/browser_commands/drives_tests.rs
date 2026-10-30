use std::sync::Arc;

use super::*;
use crate::files::local::drives::backend::fake::FakeBackend;
use crate::passwords::access::Caller;
use crate::passwords::model::ItemInput;
use crate::passwords::test_support::MARKER;
use crate::remote_storage::test_support::vault;
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
        open(&backend, None, "a").unwrap(),
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
        open(&backend, None, "b").unwrap(),
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
        open(&backend, None, "x").unwrap_err().code,
        FilesErrorCode::DriveGone
    );
}

#[test]
fn a_drive_pulled_while_mounting_is_gone() {
    let backend = FakeBackend::with(vec![stick("b", DriveState::Unmounted)]);
    *backend.mount_result.lock().unwrap() = Some(Err(DriveError::Gone));
    assert_eq!(
        open(&backend, None, "b").unwrap_err().code,
        FilesErrorCode::DriveGone
    );
}

#[test]
fn a_cancelled_system_prompt_is_reported_as_such() {
    let backend = FakeBackend::with(vec![stick("b", DriveState::Unmounted)]);
    *backend.mount_result.lock().unwrap() = Some(Err(DriveError::Cancelled));
    assert_eq!(
        open(&backend, None, "b").unwrap_err().code,
        FilesErrorCode::DriveCancelled
    );
}

#[test]
fn a_locked_drive_on_a_backend_without_passwords_is_unsupported() {
    let backend = FakeBackend::with(vec![stick("c", DriveState::Locked)]);
    assert_eq!(
        open(&backend, None, "c").unwrap_err().code,
        FilesErrorCode::DriveUnsupported
    );
}

/// A locked stick `c` whose password is `password`.
fn locked(password: &str) -> Arc<FakeBackend> {
    let backend = FakeBackend::with(vec![stick("c", DriveState::Locked)]);
    *backend.password.lock().unwrap() = Some(password.to_owned());
    Arc::new(backend)
}

/// Runs a drive command where it runs in the app: on a blocking thread of the runtime.
async fn on_blocking<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
    tokio::task::spawn_blocking(work).await.unwrap()
}

async fn saved(passwords: &PasswordsService, password: &str) -> String {
    passwords
        .create_item(
            &Caller::User,
            &[],
            ItemInput {
                title: Some("Stick".to_owned()),
                password: Some(password.to_owned()),
                url: Some(unlock::entry_url("c-key")),
                ..ItemInput::default()
            },
            None,
        )
        .await
        .unwrap()
}

async fn open_c(
    backend: &Arc<FakeBackend>,
    passwords: Option<&PasswordsService>,
) -> Result<DriveOpened, FilesError> {
    let (backend, passwords) = (backend.clone(), passwords.cloned());
    on_blocking(move || open(&*backend, passwords.as_ref(), "c")).await
}

async fn unlock_c(
    backend: &Arc<FakeBackend>,
    passwords: Option<&PasswordsService>,
    secret: DriveSecretInput,
    remember: DriveRemember,
) -> Result<DriveOpened, FilesError> {
    let (backend, passwords) = (backend.clone(), passwords.cloned());
    on_blocking(move || {
        unlock_drive(
            &*backend,
            passwords.as_ref(),
            "c",
            secret,
            remember,
            "Laufwerk: C",
        )
    })
    .await
}

fn typed(password: &str) -> DriveSecretInput {
    DriveSecretInput::Typed {
        password: password.to_owned(),
    }
}

fn needs(reason: DriveNeedsPassword, candidates: Vec<DriveEntry>) -> DriveOpened {
    DriveOpened::NeedsPassword {
        reason,
        message: None,
        candidates,
    }
}

fn opened_at_c() -> DriveOpened {
    DriveOpened::Opened {
        path: "/run/media/test/c".to_owned(),
        remember_failed: None,
    }
}

#[tokio::test]
async fn without_a_vault_a_locked_drive_asks_for_the_password() {
    let backend = locked(MARKER);
    assert_eq!(
        open_c(&backend, None).await.unwrap(),
        needs(DriveNeedsPassword::None, Vec::new())
    );
    assert!(backend.calls().is_empty(), "nothing unlocked");
}

#[tokio::test]
async fn the_one_saved_password_unlocks_without_a_dialog() {
    let (_dir, db) = vault();
    let passwords = PasswordsService::new(db);
    let backend = locked(MARKER);
    assert_eq!(
        open_c(&backend, Some(&passwords)).await.unwrap(),
        needs(DriveNeedsPassword::None, Vec::new())
    );

    saved(&passwords, MARKER).await;
    assert_eq!(
        open_c(&backend, Some(&passwords)).await.unwrap(),
        opened_at_c()
    );
    assert_eq!(*backend.unlocked_with.lock().unwrap(), [MARKER]);
}

#[tokio::test]
async fn a_saved_password_that_fails_opens_the_dialog_with_the_system_s_text() {
    let (_dir, db) = vault();
    let passwords = PasswordsService::new(db);
    let backend = locked("the right one");
    saved(&passwords, MARKER).await;
    assert_eq!(
        open_c(&backend, Some(&passwords)).await.unwrap(),
        DriveOpened::NeedsPassword {
            reason: DriveNeedsPassword::SavedFailed,
            message: Some("wrong passphrase".to_owned()),
            candidates: Vec::new(),
        }
    );
}

#[tokio::test]
async fn several_saved_passwords_are_offered() {
    let (_dir, db) = vault();
    let passwords = PasswordsService::new(db);
    let backend = locked(MARKER);
    saved(&passwords, MARKER).await;
    saved(&passwords, "another").await;
    let DriveOpened::NeedsPassword {
        reason, candidates, ..
    } = open_c(&backend, Some(&passwords)).await.unwrap()
    else {
        panic!("the dialog");
    };
    assert_eq!(reason, DriveNeedsPassword::Several);
    assert_eq!(candidates.len(), 2);
    assert!(backend.calls().is_empty(), "nothing tried");
}

#[tokio::test]
async fn a_typed_password_reaches_the_system_unchanged() {
    let backend = locked(MARKER);
    assert_eq!(
        unlock_c(&backend, None, typed(MARKER), DriveRemember::No)
            .await
            .unwrap(),
        opened_at_c()
    );
    assert_eq!(*backend.unlocked_with.lock().unwrap(), [MARKER]);

    let wrong = unlock_c(&backend, None, typed("falsch"), DriveRemember::No)
        .await
        .unwrap_err();
    assert_eq!(wrong.code, FilesErrorCode::UnlockFailed);
    assert_eq!(wrong.message, "wrong passphrase");
}

#[tokio::test]
async fn a_remembered_password_opens_the_drive_next_time() {
    let (_dir, db) = vault();
    let passwords = PasswordsService::new(db);
    let backend = locked(MARKER);
    unlock_c(&backend, Some(&passwords), typed("falsch"), DriveRemember::Yes)
        .await
        .unwrap_err();
    assert!(
        unlock::matching_entries(&passwords, "c-key")
            .await
            .unwrap()
            .is_empty(),
        "a failed unlock remembers nothing"
    );

    assert_eq!(
        unlock_c(&backend, Some(&passwords), typed(MARKER), DriveRemember::Yes)
            .await
            .unwrap(),
        opened_at_c()
    );
    let found = unlock::matching_entries(&passwords, "c-key").await.unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].title, "Laufwerk: C");
    assert_eq!(
        open_c(&backend, Some(&passwords)).await.unwrap(),
        opened_at_c()
    );
}

#[tokio::test]
async fn a_picked_entry_with_another_address_is_asked_about_after_the_unlock() {
    let (_dir, db) = vault();
    let passwords = PasswordsService::new(db);
    let backend = locked(MARKER);
    let item_id = passwords
        .create_item(
            &Caller::User,
            &[],
            ItemInput {
                title: Some("Bank".to_owned()),
                password: Some(MARKER.to_owned()),
                url: Some("https://bank.example".to_owned()),
                ..ItemInput::default()
            },
            None,
        )
        .await
        .unwrap();
    let picked = DriveSecretInput::Entry {
        item_id: item_id.clone(),
    };
    assert_eq!(
        unlock_c(&backend, Some(&passwords), picked, DriveRemember::Yes)
            .await
            .unwrap(),
        DriveOpened::Opened {
            path: "/run/media/test/c".to_owned(),
            remember_failed: Some(DriveRememberFailed::UrlTaken),
        }
    );

    let (linked, service) = (backend.clone(), passwords.clone());
    on_blocking(move || link(&*linked, Some(&service), "c-key", &item_id))
        .await
        .unwrap();
    assert_eq!(
        unlock::matching_entries(&passwords, "c-key")
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn a_picked_entry_needs_the_vault() {
    let backend = locked(MARKER);
    let picked = DriveSecretInput::Entry {
        item_id: "x".to_owned(),
    };
    assert_eq!(
        unlock_c(&backend, None, picked, DriveRemember::No)
            .await
            .unwrap_err()
            .code,
        FilesErrorCode::NoVault
    );
}

#[tokio::test]
async fn a_typed_password_never_reaches_the_log() {
    crate::test_log::install();
    let backend = locked("the right one");
    // A wrong one, and a right one whose remembering fails without a vault: both log.
    unlock_c(&backend, None, typed(MARKER), DriveRemember::No)
        .await
        .unwrap_err();
    *backend.password.lock().unwrap() = Some(MARKER.to_owned());
    let opened = unlock_c(&backend, None, typed(MARKER), DriveRemember::Yes)
        .await
        .unwrap();
    assert_eq!(
        opened,
        DriveOpened::Opened {
            path: "/run/media/test/c".to_owned(),
            remember_failed: Some(DriveRememberFailed::Failed),
        }
    );
    assert!(!crate::test_log::all().contains(MARKER));
    assert!(!format!("{opened:?} {:?}", backend.calls()).contains(MARKER));
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
