//! The agent browser's profile on disk (spec 065, research R3, contracts/agent-browser.md
//! "Profil"). While the browser runs, the profile is an ordinary directory, because Chromium writes
//! files: owner-only, under `$XDG_RUNTIME_DIR` on Linux (in memory) or the user's temporary
//! folder elsewhere. holzi removes it at the end of every run and before every start, so leftovers
//! of a crash live only until the next start. Between runs only the lasting parts are kept, as a
//! ZIP that [`super::profile_store`] puts into an encrypted folder.
//!
//! The browser saves no passwords and fills no forms itself (FR-002): credentials come from the
//! password manager only.

use std::io::{Cursor, Read, Write};
use std::path::{Component, Path, PathBuf};

use serde_json::{json, Value};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use super::error::{BrowserError, Result};
use super::locate::Os;

/// The parts of a Chromium profile that last between runs; caches and everything else are dropped.
/// A directory keeps everything below it; a file keeps its SQLite journal too.
pub const LASTING: [&str; 8] = [
    "Local State",
    "Default/Preferences",
    "Default/Network/Cookies",
    // Before Chromium 96 the cookies were here.
    "Default/Cookies",
    "Default/Local Storage",
    "Default/IndexedDB",
    "Default/Session Storage",
    "Default/Network/TransportSecurity",
];

/// The most a profile ZIP may unpack to; a bigger one is refused rather than filling the disk.
const MAX_UNPACKED: u64 = 2 * 1024 * 1024 * 1024;

/// The folder all working directories of the agent browser live in.
pub fn root(os: Os) -> Result<PathBuf> {
    match os {
        Os::Linux => linux_root(),
        Os::Macos | Os::Windows => Ok(std::env::temp_dir().join("holzi-agent-browser")),
    }
}

#[cfg(unix)]
fn linux_root() -> Result<PathBuf> {
    if let Some(runtime_dir) = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|dir| dir.is_absolute())
    {
        let root = runtime_dir.join("holzi-agent-browser");
        if ensure_private_root(&root).is_ok() {
            return Ok(root);
        }
    }

    // `/tmp/holzi-agent-browser` would be shared by every local account. Use a per-user directory
    // and refuse a pre-existing symlink, foreign owner, or loose mode.
    let root = std::env::temp_dir().join(format!("holzi-agent-browser-{}", unsafe {
        libc::geteuid()
    }));
    ensure_private_root(&root)?;
    Ok(root)
}

#[cfg(not(unix))]
fn linux_root() -> Result<PathBuf> {
    Err(BrowserError::Unsupported)
}

#[cfg(unix)]
fn ensure_private_root(dir: &Path) -> Result<()> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt};

    match std::fs::symlink_metadata(dir) {
        Ok(metadata) => {
            if !metadata.file_type().is_dir()
                || metadata.uid() != unsafe { libc::geteuid() }
                || metadata.mode() & 0o777 != 0o700
            {
                return Err(BrowserError::Profile(format!(
                    "unsafe agent browser profile root: {}",
                    dir.display()
                )));
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            std::fs::DirBuilder::new()
                .mode(0o700)
                .create(dir)
                .map_err(profile_error)?;
            let metadata = std::fs::symlink_metadata(dir).map_err(profile_error)?;
            if !metadata.file_type().is_dir()
                || metadata.uid() != unsafe { libc::geteuid() }
                || metadata.mode() & 0o777 != 0o700
            {
                return Err(BrowserError::Profile(format!(
                    "unsafe agent browser profile root: {}",
                    dir.display()
                )));
            }
        }
        Err(error) => return Err(profile_error(error)),
    }
    Ok(())
}

/// The working directory of one vault instance for one run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workdir {
    pub dir: PathBuf,
}

impl Workdir {
    /// The working directory of `instance` below `root`; nothing is created yet.
    pub fn of(root: &Path, instance: &str) -> Self {
        Self {
            dir: root.join(instance),
        }
    }

    /// The browser's `--user-data-dir`.
    pub fn profile(&self) -> PathBuf {
        self.dir.join("profile")
    }

    /// Where downloads land (research R11).
    pub fn downloads(&self) -> PathBuf {
        self.dir.join("downloads")
    }

    /// Removes what a former run left, then creates the directory owner-only with an unpacked
    /// `zip` (or an empty profile) and the browser's own password saving switched off.
    pub fn prepare(&self, zip: Option<&[u8]>) -> Result<()> {
        self.remove()?;
        create_private(&self.dir)?;
        create_private(&self.profile())?;
        create_private(&self.downloads())?;
        if let Some(zip) = zip {
            unpack(zip, &self.profile())?;
        }
        no_saved_credentials(&self.profile())
    }

    /// Removes the directory with everything in it; a directory that is not there is fine.
    pub fn remove(&self) -> Result<()> {
        match std::fs::remove_dir_all(&self.dir) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(profile_error(error)),
        }
    }
}

#[cfg(unix)]
fn create_private(dir: &Path) -> Result<()> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
        .map_err(profile_error)?;
    // `mode` does not apply to a directory that existed (the shared root of earlier runs).
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700)).map_err(profile_error)
}

// ponytail: on Windows the folder inherits the access list of the user's temporary folder, which
// only the user and the system can read. Upgrade path: an explicit owner-only access list.
#[cfg(not(unix))]
fn create_private(dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir).map_err(profile_error)
}

fn profile_error(error: impl std::fmt::Display) -> BrowserError {
    BrowserError::Profile(error.to_string())
}

/// Whether `rel` (with `/`) is one of the lasting parts or inside one.
pub fn is_lasting(rel: &str) -> bool {
    LASTING.iter().any(|part| {
        rel == *part
            || rel.strip_prefix(part).is_some_and(|rest| {
                rest.starts_with('/') || rest == "-journal" || rest == "-wal" || rest == "-shm"
            })
    })
}

/// The lasting parts of the profile in `profile` as a ZIP.
pub fn pack(profile: &Path) -> Result<Vec<u8>> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .large_file(true);
    let mut pending = vec![profile.to_path_buf()];
    let mut files = Vec::new();
    while let Some(dir) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                pending.push(path);
            } else if kind.is_file() {
                let rel = relative(profile, &path);
                if is_lasting(&rel) {
                    files.push((rel, path));
                }
            }
        }
    }
    files.sort();
    for (rel, path) in files {
        let mut data = Vec::new();
        // A file the browser removed meanwhile is simply not kept.
        if std::fs::File::open(&path)
            .and_then(|mut file| file.read_to_end(&mut data))
            .is_err()
        {
            continue;
        }
        writer.start_file(rel, options).map_err(profile_error)?;
        writer.write_all(&data).map_err(profile_error)?;
    }
    Ok(writer.finish().map_err(profile_error)?.into_inner())
}

/// `path` below `base`, with `/` as separator.
fn relative(base: &Path, path: &Path) -> String {
    path.strip_prefix(base)
        .unwrap_or(path)
        .components()
        .filter_map(|component| match component {
            Component::Normal(name) => Some(name.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// Unpacks a profile ZIP into `profile`. Only lasting parts are written, each strictly inside
/// `profile`; anything else in the archive is ignored.
pub fn unpack(zip: &[u8], profile: &Path) -> Result<()> {
    let mut archive = ZipArchive::new(Cursor::new(zip)).map_err(profile_error)?;
    let mut total = 0u64;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(profile_error)?;
        let Some(rel) = entry.enclosed_name() else {
            continue;
        };
        let rel_text = relative(Path::new(""), &rel);
        if !entry.is_file() || entry.is_symlink() || !is_lasting(&rel_text) {
            continue;
        }
        total = total.saturating_add(entry.size());
        if total > MAX_UNPACKED {
            return Err(BrowserError::Profile("the profile is too large".to_owned()));
        }
        let target = profile.join(&rel);
        if let Some(parent) = target.parent() {
            create_private(parent)?;
        }
        let mut data = Vec::new();
        entry
            .by_ref()
            .take(MAX_UNPACKED)
            .read_to_end(&mut data)
            .map_err(profile_error)?;
        std::fs::write(&target, data).map_err(profile_error)?;
    }
    Ok(())
}

/// Switches Chromium's own password saving and form filling off in `Default/Preferences`
/// (contracts/agent-browser.md "Profil", FR-002); the rest of the file stays.
pub fn no_saved_credentials(profile: &Path) -> Result<()> {
    let path = profile.join("Default").join("Preferences");
    let mut preferences: Value = std::fs::read(&path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({}));
    set(&mut preferences, &["credentials_enable_service"], false);
    set(&mut preferences, &["credentials_enable_autosignin"], false);
    set(
        &mut preferences,
        &["profile", "password_manager_enabled"],
        false,
    );
    set(&mut preferences, &["autofill", "profile_enabled"], false);
    set(
        &mut preferences,
        &["autofill", "credit_card_enabled"],
        false,
    );
    if let Some(parent) = path.parent() {
        create_private(parent)?;
    }
    let bytes = serde_json::to_vec(&preferences).map_err(profile_error)?;
    std::fs::write(&path, bytes).map_err(profile_error)
}

fn set(value: &mut Value, path: &[&str], to: bool) {
    let Some((last, parents)) = path.split_last() else {
        return;
    };
    let mut at = value;
    for key in parents {
        if !at.get(*key).is_some_and(Value::is_object) {
            at[*key] = json!({});
        }
        at = &mut at[*key];
    }
    at[*last] = Value::Bool(to);
}
