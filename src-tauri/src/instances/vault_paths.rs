//! The one place that knows where the files of a vault lie (spec 084, FR-003).
//!
//! Every vault has a folder, `<AppLocalData>/vaults/<name>/`. Its root holds the SQLite files
//! (`vault.db`, `-wal`, `-shm`) and small bookkeeping files (`.lock`, `.vault-id`, `.pending`);
//! everything else goes into subfolders. The agent browser profile and the chat files still lie in
//! `<AppLocalData>/agent-browser/<name>/` and `<AppLocalData>/chat-files/<name>/` until stage 3.
//!
//! The `legacy_*` functions name where the database of a vault lay before the folders; only the
//! move at start (`layout_move`) uses them.
//!
//! `VaultPaths` is plain path arithmetic over the data folder: it checks no name and touches no
//! disk, so a test builds one for a temporary folder. A caller that takes a name from outside
//! validates it first (`paths::validate_instance_name`).

use std::path::{Path, PathBuf};

const VAULTS: &str = "vaults";
const LEGACY_INSTANCES: &str = "instances";
const LEGACY_EXTENSION: &str = "db";
/// The name of the database inside a vault folder. The folder carries the vault's name.
pub const DATABASE_FILE: &str = "vault.db";
/// Folders in `vaults/` that start like this hold a vault that is being moved; no vault name
/// starts with a dot, so the list and every lookup by name ignore them.
pub const STAGING_PREFIX: &str = ".moving-";
const DEVICE_LOCK: &str = "vaults.lock";

const AGENT_BROWSER: &str = "agent-browser";
const CHAT_FILES: &str = "chat-files";

/// The paths of the vaults of one data folder.
#[derive(Debug, Clone)]
pub struct VaultPaths {
    root: PathBuf,
}

impl VaultPaths {
    /// The vaults of the data folder `app_local_data`.
    pub fn new(app_local_data: impl Into<PathBuf>) -> Self {
        Self {
            root: app_local_data.into(),
        }
    }

    /// The data folder itself.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The folder that holds one folder per vault.
    pub fn vaults_dir(&self) -> PathBuf {
        self.root.join(VAULTS)
    }

    /// The folder of the vault `name`.
    pub fn folder(&self, name: &str) -> PathBuf {
        self.vaults_dir().join(name)
    }

    /// The database file of the vault `name`.
    pub fn database(&self, name: &str) -> PathBuf {
        self.folder(name).join(DATABASE_FILE)
    }

    /// Where the files of the vault `name` are gathered while they move into `folder(name)`.
    pub fn staging(&self, name: &str) -> PathBuf {
        self.vaults_dir().join(format!("{STAGING_PREFIX}{name}"))
    }

    /// The lock that keeps two processes from moving vaults at the same time.
    pub fn device_lock(&self) -> PathBuf {
        self.root.join(DEVICE_LOCK)
    }

    /// The folder the databases lay in before the vault folders.
    pub fn legacy_dir(&self) -> PathBuf {
        self.root.join(LEGACY_INSTANCES)
    }

    /// The database file of the vault `name` before the vault folders.
    pub fn legacy_database(&self, name: &str) -> PathBuf {
        self.legacy_dir().join(format!("{name}.{LEGACY_EXTENSION}"))
    }

    /// The folder of the agent browser on this device. Its own files (the filter lists, the DNS
    /// choice) and the profile folders of the vaults lie directly in it.
    pub fn agent_browser_root(&self) -> PathBuf {
        self.root.join(AGENT_BROWSER)
    }

    /// The agent browser profile folder of the vault `name`.
    pub fn agent_browser(&self, name: &str) -> PathBuf {
        self.agent_browser_root().join(name)
    }

    /// The chat files folder of the vault `name`.
    pub fn chat_files(&self, name: &str) -> PathBuf {
        self.root.join(CHAT_FILES).join(name)
    }
}

#[cfg(test)]
#[path = "vault_paths_tests.rs"]
mod tests;
