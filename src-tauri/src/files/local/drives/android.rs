//! The drives of an Android device (spec 064 FR-023, research R13): the shared storage and every SD
//! card or USB drive under `/storage`. Apps may not mount or eject there, so the list is only kept
//! live.

/// `XXXX-XXXX` in hex, the name Android gives an SD card or USB drive.
pub fn is_volume_id(name: &str) -> bool {
    let bytes = name.as_bytes();
    bytes.len() == 9
        && bytes[4] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(i, b)| i == 4 || b.is_ascii_hexdigit())
}

#[cfg(target_os = "android")]
pub use platform::AndroidBackend;

#[cfg(target_os = "android")]
mod platform {
    use std::path::{Path, PathBuf};

    use super::super::backend::{DriveBackend, DriveError};
    use super::super::{Drive, DriveKind};
    use super::is_volume_id;

    const SHARED: &str = "/storage/emulated/0";

    pub struct AndroidBackend;

    impl DriveBackend for AndroidBackend {
        fn list(&self) -> Result<Vec<Drive>, DriveError> {
            let mut drives = vec![Drive::mounted(
                SHARED.to_owned(),
                Path::new(SHARED),
                DriveKind::Internal,
            )];
            let Ok(read) = std::fs::read_dir("/storage") else {
                return Ok(drives);
            };
            let mut cards: Vec<(String, PathBuf)> = read
                .filter_map(Result::ok)
                .filter_map(|item| {
                    let name = item.file_name().to_str()?.to_owned();
                    is_volume_id(&name).then(|| (name, item.path()))
                })
                .collect();
            cards.sort();
            drives.extend(cards.into_iter().map(|(name, path)| Drive {
                key: Some(name.to_lowercase()),
                label: Some(name),
                ..Drive::mounted(path.to_string_lossy().into_owned(), &path, DriveKind::Removable)
            }));
            Ok(drives)
        }
    }
}

#[cfg(test)]
#[path = "android_tests.rs"]
mod tests;
