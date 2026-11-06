//! The one place that knows where the files of a vault lie (spec 084, FR-003).
//!
//! Stage 1 describes the layout as it is today, so nothing moves: the database in
//! `<AppLocalData>/instances/<name>.db`, the agent browser profile in
//! `<AppLocalData>/agent-browser/<name>/` and the chat files in `<AppLocalData>/chat-files/<name>/`.
//! The later stages change the answers of these functions, not their callers.
//!
//! `VaultPaths` is plain path arithmetic over the data folder: it checks no name and touches no
//! disk, so a test builds one for a temporary folder. A caller that takes a name from outside
//! validates it first (`paths::validate_instance_name`).

use std::path::{Path, PathBuf};

use super::paths::INSTANCE_EXTENSION;

const INSTANCES: &str = "instances";
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

    /// The folder that holds the vault databases.
    pub fn instances_dir(&self) -> PathBuf {
        self.root.join(INSTANCES)
    }

    /// The database file of the vault `name`.
    pub fn database(&self, name: &str) -> PathBuf {
        self.instances_dir()
            .join(format!("{name}.{INSTANCE_EXTENSION}"))
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
