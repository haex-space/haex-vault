//! Where a window path of a request leads on the serving device (spec 056 research R3): a root
//! folder and a key below it, which [`crate::remote_storage::local::LocalStore`] resolves without
//! letting `..` or a link leave the root (FR-006). An own device's paths start at the file system
//! root (on Windows the first part is the drive, `/C:/…`); a contact's paths start at the root of
//! its share.

use std::path::{Path, PathBuf};

use crate::files::peer::wire::FailCode;
use crate::files::share::store::FileShareRow;
use crate::files::share::FileShareItemKind;
use crate::files::SourceRef;

/// A request's target: the folder it may not leave, and the key below it (`""` is the folder).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Located {
    pub root: PathBuf,
    pub key: String,
    /// For a share of a single file: its name, the only entry below `root` the contact sees.
    pub only: Option<String>,
}

impl Located {
    /// The path the request is about (not resolved; links are resolved by the caller).
    pub fn path(&self) -> PathBuf {
        let mut path = self.root.clone();
        for part in self.key.split('/').filter(|part| !part.is_empty()) {
            path.push(part);
        }
        path
    }

    /// The key as the prefix of a folder listing: `""` or ending in `/`.
    pub fn folder_prefix(&self) -> String {
        if self.key.is_empty() {
            String::new()
        } else {
            format!("{}/", self.key)
        }
    }
}

/// The parts of the window path `path`: it starts with `/`; no part is `.`, `..`, holds a `\` or a
/// NUL. Empty parts (`//`, a trailing `/`) are skipped.
pub fn parts(path: &str) -> Result<Vec<&str>, FailCode> {
    let rest = path.strip_prefix('/').ok_or(FailCode::Refused)?;
    let parts: Vec<&str> = rest.split('/').filter(|part| !part.is_empty()).collect();
    let bad = |part: &&str| *part == "." || *part == ".." || part.contains(['\\', '\0']);
    if parts.iter().any(bad) {
        return Err(FailCode::Refused);
    }
    Ok(parts)
}

/// The path of an own device for the window path `path`.
pub fn on_device(path: &str) -> Result<Located, FailCode> {
    let (root, rest) = device_root(&parts(path)?)?;
    check_names(&rest)?;
    Ok(Located {
        root,
        key: rest.join("/"),
        only: None,
    })
}

/// The path a contact's `path` names in `share`: below the shared folder, or the shared file.
pub fn in_share(share: &FileShareRow, path: &str) -> Result<Located, FailCode> {
    if share.source != SourceRef::Device {
        // Shares of a storage or an encrypted folder come with the delivery for contacts.
        return Err(FailCode::Unsupported);
    }
    let shared = on_device(&share.path)?.path();
    let rest = parts(path)?;
    check_names(&rest)?;
    match share.item_kind {
        FileShareItemKind::Folder => Ok(Located {
            root: shared,
            key: rest.join("/"),
            only: None,
        }),
        FileShareItemKind::File => {
            let (Some(parent), Some(name)) = (shared.parent(), shared.file_name()) else {
                return Err(FailCode::NotFound);
            };
            let name = name.to_string_lossy().into_owned();
            let key = match rest.as_slice() {
                [] => String::new(),
                [only] if *only == name => name.clone(),
                _ => return Err(FailCode::NotFound),
            };
            Ok(Located {
                root: parent.to_path_buf(),
                key,
                only: Some(name),
            })
        }
    }
}

/// The root folder and the parts below it.
#[cfg(not(windows))]
fn device_root<'a>(parts: &[&'a str]) -> Result<(PathBuf, Vec<&'a str>), FailCode> {
    Ok((PathBuf::from("/"), parts.to_vec()))
}

/// The drive and the parts below it; the list of drives itself comes with the delivery for own
/// devices.
#[cfg(windows)]
fn device_root<'a>(parts: &[&'a str]) -> Result<(PathBuf, Vec<&'a str>), FailCode> {
    let Some((drive, rest)) = parts.split_first() else {
        return Err(FailCode::Unsupported);
    };
    let letter = drive
        .strip_suffix(':')
        .filter(|letter| letter.len() == 1 && letter.chars().all(|c| c.is_ascii_alphabetic()))
        .ok_or(FailCode::Refused)?;
    Ok((
        PathBuf::from(format!("{}:\\", letter.to_ascii_uppercase())),
        rest.to_vec(),
    ))
}

/// On Windows a name with `:` would name another stream of a file.
fn check_names(parts: &[&str]) -> Result<(), FailCode> {
    if cfg!(windows) && parts.iter().any(|part| part.contains(':')) {
        return Err(FailCode::Refused);
    }
    Ok(())
}

/// The window path of the device path `path`: the same on Unix, `/C:/…` on Windows.
pub fn window_of(path: &Path) -> String {
    if cfg!(windows) {
        let slashed = path.to_string_lossy().replace('\\', "/");
        format!("/{}", slashed.trim_end_matches('/'))
    } else {
        path.to_string_lossy().into_owned()
    }
}

#[cfg(test)]
#[path = "roots_tests.rs"]
mod tests;
