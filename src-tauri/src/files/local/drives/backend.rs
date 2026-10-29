//! The seam between the drive list and the platform (spec 064 plan): one [`DriveBackend`] per
//! platform, chosen once by [`backend`]. Every call blocks; callers run it off the async executor.

use std::sync::{Arc, OnceLock};

use super::Drive;
use crate::files::{FilesError, FilesErrorCode};

/// How a locked drive is unlocked on this platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnlockMode {
    /// With a password haex hands to the system (Linux, macOS).
    Passphrase,
    /// Only in the system's own dialog (Windows/BitLocker, FR-019).
    SystemDialog,
    None,
}

/// A drive password on its way to the system. It has no `Debug`, `Display` or `Serialize`, so it
/// cannot end up in a log line, an error or the window by accident (FR-017).
pub struct DriveSecret(String);

impl DriveSecret {
    pub fn new(secret: String) -> Self {
        Self(secret)
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

/// Why a drive action failed (contracts/commands.md). Messages come from the system and never hold a
/// password.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DriveError {
    /// The system's prompt was cancelled or refused (FR-007).
    Cancelled,
    /// Another program holds the drive (FR-012).
    Busy,
    /// The drive is not there (any more).
    Gone,
    Failed(String),
    UnlockFailed(String),
    /// Unlocked, but no file system haex can open (FR-020).
    NoFilesystem,
    /// This platform or backend cannot do it.
    Unsupported,
    NoVault,
    UrlTaken,
}

impl From<DriveError> for FilesError {
    fn from(error: DriveError) -> Self {
        let (code, message) = match error {
            DriveError::Cancelled => (FilesErrorCode::DriveCancelled, "cancelled".to_owned()),
            DriveError::Busy => (FilesErrorCode::DriveBusy, "the drive is in use".to_owned()),
            DriveError::Gone => (FilesErrorCode::DriveGone, "the drive is gone".to_owned()),
            DriveError::Failed(message) => (FilesErrorCode::DriveFailed, message),
            DriveError::UnlockFailed(message) => (FilesErrorCode::UnlockFailed, message),
            DriveError::NoFilesystem => (
                FilesErrorCode::DriveNoFilesystem,
                "no file system that can be opened".to_owned(),
            ),
            DriveError::Unsupported => (
                FilesErrorCode::DriveUnsupported,
                "not available on this device".to_owned(),
            ),
            DriveError::NoVault => (
                FilesErrorCode::NoVault,
                "the password manager needs an open vault".to_owned(),
            ),
            DriveError::UrlTaken => (
                FilesErrorCode::UrlTaken,
                "the entry already has another address".to_owned(),
            ),
        };
        FilesError::new(code, message)
    }
}

/// What a platform can do with its drives. Only `list` is required; the rest answer
/// [`DriveError::Unsupported`] unless a backend can do it.
pub trait DriveBackend: Send + Sync {
    fn list(&self) -> Result<Vec<Drive>, DriveError>;

    /// Mounts the drive and returns its mount path.
    fn mount(&self, id: &str) -> Result<String, DriveError> {
        let _ = id;
        Err(DriveError::Unsupported)
    }

    /// Unlocks and mounts the drive and returns its mount path.
    fn unlock(&self, id: &str, secret: &DriveSecret) -> Result<String, DriveError> {
        let _ = (id, secret);
        Err(DriveError::Unsupported)
    }

    /// Unmounts the drive (and locks, powers off or ejects it); whether it may be pulled now.
    fn release(&self, id: &str) -> Result<bool, DriveError> {
        let _ = id;
        Err(DriveError::Unsupported)
    }

    fn unlock_mode(&self) -> UnlockMode {
        UnlockMode::None
    }

    fn open_system_unlock(&self, id: &str) -> Result<(), DriveError> {
        let _ = id;
        Err(DriveError::Unsupported)
    }
}

/// The backend of this platform, chosen once.
pub fn backend() -> Arc<dyn DriveBackend> {
    static BACKEND: OnceLock<Arc<dyn DriveBackend>> = OnceLock::new();
    BACKEND.get_or_init(choose).clone()
}

#[cfg(all(target_os = "linux", not(target_os = "android")))]
fn choose() -> Arc<dyn DriveBackend> {
    match super::udisks::UdisksBackend::connect() {
        Ok(backend) => Arc::new(backend),
        Err(error) => {
            log::warn!("files: no system bus for udisks2, drives come from mountinfo: {error:?}");
            Arc::new(super::fallback::FallbackBackend)
        }
    }
}

#[cfg(target_os = "android")]
fn choose() -> Arc<dyn DriveBackend> {
    Arc::new(super::android::AndroidBackend)
}

#[cfg(not(any(target_os = "linux", target_os = "android")))]
fn choose() -> Arc<dyn DriveBackend> {
    Arc::new(super::fallback::FallbackBackend)
}

/// A scripted backend for tests: a list to answer and a record of the calls.
#[cfg(test)]
pub mod fake {
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    pub struct FakeBackend {
        pub drives: Mutex<Vec<Drive>>,
        pub calls: Mutex<Vec<String>>,
        pub mount_result: Mutex<Option<Result<String, DriveError>>>,
        pub list_count: Mutex<usize>,
    }

    impl FakeBackend {
        pub fn with(drives: Vec<Drive>) -> Self {
            Self {
                drives: Mutex::new(drives),
                ..Self::default()
            }
        }

        pub fn calls(&self) -> Vec<String> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl DriveBackend for FakeBackend {
        fn list(&self) -> Result<Vec<Drive>, DriveError> {
            *self.list_count.lock().unwrap() += 1;
            Ok(self.drives.lock().unwrap().clone())
        }

        fn mount(&self, id: &str) -> Result<String, DriveError> {
            self.calls.lock().unwrap().push(format!("mount {id}"));
            self.mount_result
                .lock()
                .unwrap()
                .clone()
                .unwrap_or_else(|| Ok(format!("/run/media/test/{id}")))
        }
    }
}
