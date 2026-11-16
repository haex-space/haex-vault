//! What a delegate needs to start its tool: the installed program, the vault's home, the folder
//! for one run and the containment to start it under (spec 094, `data-model.md`).
//!
//! The adapters receive a [`DelegateRuntime`] instead of a binary name. Everything in it lies in
//! the vault's folder; there is no `PATH` lookup and no host fallback (spec FR-003).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use uuid::Uuid;

use super::catalog::{Catalog, VendorDefinition};
use super::containment::{Containment, EnvContainment, IsolationLevel};
use crate::instances::vault_paths::VaultPaths;

/// A folder for one delegate run or connect flow, below `<vault>/home/run/`: the tool's
/// configuration folder, its working folder, its temp folder and holzi's own files for the run
/// (attachments, system prompt, MCP config, the approval socket). It is removed when the value
/// is dropped; whatever a killed process leaves behind is swept at the next vault open.
///
/// The folder name is 8 hex characters so that `<run>/approval.sock` stays as short as it can
/// be (a Unix socket address holds about 104 bytes).
#[derive(Debug)]
pub struct RunFolder {
    dir: PathBuf,
    tmp: PathBuf,
    runtime: PathBuf,
    home: PathBuf,
}

const TMP: &str = "tmp";
const RUNTIME: &str = "rt";
const ATTEMPTS: usize = 8;

impl RunFolder {
    /// Creates a new run folder below `run_root`, for a tool whose home is `home`. Both come
    /// from `VaultPaths`; `run_root` is created when it is missing.
    pub fn create(run_root: &Path, home: &Path) -> io::Result<Self> {
        fs::create_dir_all(run_root)?;
        for _ in 0..ATTEMPTS {
            let id = Uuid::new_v4().simple().to_string();
            let dir = run_root.join(&id[..8]);
            match fs::create_dir(&dir) {
                Ok(()) => return Self::fill(dir, home),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "no free run folder name",
        ))
    }

    fn fill(dir: PathBuf, home: &Path) -> io::Result<Self> {
        let folder = Self {
            tmp: dir.join(TMP),
            runtime: dir.join(RUNTIME),
            home: home.to_path_buf(),
            dir,
        };
        // On any failure below, dropping `folder` removes what was created.
        fs::create_dir(&folder.tmp)?;
        fs::create_dir(&folder.runtime)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            // `XDG_RUNTIME_DIR` must be private to its user.
            fs::set_permissions(&folder.runtime, fs::Permissions::from_mode(0o700))?;
        }
        Ok(folder)
    }

    /// The run folder itself: the tool's configuration folder and working folder.
    pub fn path(&self) -> &Path {
        &self.dir
    }

    /// The run's temp folder.
    pub fn tmp(&self) -> &Path {
        &self.tmp
    }

    /// The folder `XDG_RUNTIME_DIR` points at where the host defines that variable.
    pub fn runtime(&self) -> &Path {
        &self.runtime
    }

    /// The vault's home folder, the same for every run.
    pub fn home(&self) -> &Path {
        &self.home
    }
}

impl Drop for RunFolder {
    fn drop(&mut self) {
        // Best effort: a failure leaves a folder the sweep at the next vault open removes.
        let _ = fs::remove_dir_all(&self.dir);
    }
}

/// Everything an adapter needs to start one vendor's tool of one vault.
#[derive(Clone)]
pub struct DelegateRuntime {
    /// The vendor's definition: the switches and variables the tool always gets.
    pub vendor: Arc<VendorDefinition>,
    /// Absolute path of the installed program.
    pub executable: PathBuf,
    /// The vault's home folder, `<vault folder>/home/`.
    pub home: PathBuf,
    /// The folder of the per-run folders, `<vault folder>/home/run/`.
    pub run_root: PathBuf,
    /// How the tool's processes are contained.
    pub containment: Arc<dyn Containment>,
}

impl std::fmt::Debug for DelegateRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DelegateRuntime")
            .field("vendor", &self.vendor.vendor)
            .field("executable", &self.executable)
            .field("run_root", &self.run_root)
            .finish_non_exhaustive()
    }
}

impl DelegateRuntime {
    /// A runtime for a test: the shipped definition of `vendor`, a stub program at
    /// `executable`, a home and run folder of a vault named `stub` below `dir`, and the base containment.
    ///
    /// # Panics
    ///
    /// When `vendor` is not in the shipped catalog.
    pub fn for_stub(vendor: &str, executable: impl Into<PathBuf>, dir: &Path) -> Self {
        let definition = Catalog::shipped()
            .vendor(vendor)
            .unwrap_or_else(|| panic!("stub vendor {vendor} is not in the catalog"))
            .clone();
        // The same arithmetic as a real vault, for a vault named `stub` below `dir`.
        let paths = VaultPaths::new(dir);
        Self {
            vendor: Arc::new(definition),
            executable: executable.into(),
            home: paths.home("stub"),
            run_root: paths.run_root("stub"),
            containment: Arc::new(EnvContainment),
        }
    }

    /// A new folder for one run or connect flow.
    pub fn run_folder(&self) -> io::Result<RunFolder> {
        RunFolder::create(&self.run_root, &self.home)
    }

    /// Prepares `command` (environment and, for a sandbox, the wrapper) for a run in `run`.
    pub fn contain(&self, command: &mut Command, run: &RunFolder) -> IsolationLevel {
        self.containment.contain(command, run, &self.vendor)
    }

    /// The level the tool's processes run under.
    pub fn isolation(&self) -> IsolationLevel {
        self.containment.level()
    }

    /// Called when a request or connect flow reached its regular end, so the version it used
    /// counts as proven (spec FR-022). The deletion of older versions follows with US3.
    pub fn confirm(&self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;
