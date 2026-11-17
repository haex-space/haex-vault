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
use super::installed::{self, Misfit, Resolution};
use super::platform::PlatformKey;
use super::record::Record;
use crate::instances::vault_paths::VaultPaths;
#[cfg(unix)]
use crate::mcp_server::endpoint::MAX_SOCKET_PATH_BYTES;

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
    /// Files outside the run folder that this run made and removes with it (a residual, see
    /// [`RunFolder::approval_socket`]).
    outside: Vec<PathBuf>,
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
            match create_private_run_dir(&dir) {
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
            outside: Vec::new(),
            dir,
        };
        // On any failure below, dropping `folder` removes what was created.
        fs::create_dir(&folder.tmp)?;
        fs::create_dir(&folder.runtime)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            // This folder holds tool credentials, so only its owner may access it.
            fs::set_permissions(&folder.dir, fs::Permissions::from_mode(0o700))?;
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

    /// Where holzi's approval socket for this run lies: in the run folder, which keeps it inside
    /// the vault. A Unix socket address holds about 104 bytes, so for a vault whose folder path is
    /// too long the socket gets a random name in the system temp folder instead, and is removed
    /// with the run. That is one of the two residuals the spec lists (spec 094 Assumptions): it
    /// carries neither a vault name nor any content.
    #[cfg(unix)]
    pub fn approval_socket(&mut self) -> PathBuf {
        let preferred = self.dir.join("approval.sock");
        if preferred.as_os_str().len() <= MAX_SOCKET_PATH_BYTES {
            return preferred;
        }
        let id = Uuid::new_v4().simple().to_string();
        // residual: see the doc comment above
        let filename = format!("holzi-{}.sock", &id[..12]);
        let temp = std::env::temp_dir().join(&filename);
        let fallback = if temp.as_os_str().len() <= MAX_SOCKET_PATH_BYTES {
            temp
        } else {
            Path::new("/tmp").join(filename)
        };
        self.outside.push(fallback.clone());
        fallback
    }

    #[cfg(not(unix))]
    pub fn approval_socket(&mut self) -> PathBuf {
        self.dir.join("approval.sock")
    }
}

#[cfg(unix)]
fn create_private_run_dir(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;

    let mut builder = fs::DirBuilder::new();
    builder.mode(0o700).create(path)
}

#[cfg(not(unix))]
fn create_private_run_dir(path: &Path) -> io::Result<()> {
    fs::create_dir(path)
}

impl Drop for RunFolder {
    fn drop(&mut self) {
        // Best effort: a failure leaves a folder the sweep at the next vault open removes.
        let _ = fs::remove_dir_all(&self.dir);
        for file in &self.outside {
            let _ = fs::remove_file(file);
        }
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
    /// Where connect and run events of this vault are recorded (`<vault folder>/home/records/`).
    pub record: Record,
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
        let definition = Catalog::active()
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
            record: Record::new(paths.records_dir("stub")),
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

/// Why no runtime could be built for a vendor: a state the person can act on (spec FR-004,
/// FR-013). The reason text is what the adapter reports and the Settings page maps to its
/// install action, so it carries the vendor id and nothing else.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeError {
    /// The vault has no installation of the tool.
    NotInstalled { vendor: String },
    /// An installation exists but is for another device or damaged.
    NeedsReinstall { vendor: String, why: Misfit },
}

impl RuntimeError {
    /// The reason an adapter's `Unavailable` error carries.
    pub fn reason(&self) -> String {
        match self {
            Self::NotInstalled { vendor } => format!("not installed in this vault: {vendor}"),
            Self::NeedsReinstall { vendor, .. } => format!("needs reinstall: {vendor}"),
        }
    }
}

/// Where the tools of the open vault are: enough to build a [`DelegateRuntime`] when a run starts.
/// The version is chosen then, not when an adapter is built, so an install or a removal between
/// two requests is seen by the next one (spec FR-010, FR-022).
#[derive(Clone)]
pub struct DelegateTools {
    paths: VaultPaths,
    vault: String,
    containment: Arc<dyn Containment>,
}

impl DelegateTools {
    /// The tools of the vault `vault`, started under the base containment.
    pub fn new(paths: VaultPaths, vault: impl Into<String>) -> Self {
        Self {
            paths,
            vault: vault.into(),
            containment: Arc::new(EnvContainment),
        }
    }

    /// The paths of the vaults of this device.
    pub fn paths(&self) -> &VaultPaths {
        &self.paths
    }

    /// The name of the vault whose tools these are.
    pub fn vault(&self) -> &str {
        &self.vault
    }

    /// The record of this vault's tool and delegate activity.
    pub fn record(&self) -> Record {
        Record::new(self.paths.records_dir(&self.vault))
    }

    /// The containment every process of these tools is started under.
    pub fn containment(&self) -> &Arc<dyn Containment> {
        &self.containment
    }

    /// The runtime of `vendor` as installed now.
    pub fn runtime(&self, vendor: &str) -> Result<DelegateRuntime, RuntimeError> {
        let not_installed = || RuntimeError::NotInstalled {
            vendor: vendor.to_string(),
        };
        let definition = Catalog::active().vendor(vendor).ok_or_else(not_installed)?;
        let found = installed::resolve(
            &self.paths.cli_tool_dir(&self.vault, vendor),
            definition,
            PlatformKey::host(),
        );
        match found {
            Resolution::Usable(installation) => Ok(DelegateRuntime {
                vendor: Arc::new(definition.clone()),
                executable: installation.executable,
                home: self.paths.home(&self.vault),
                run_root: self.paths.run_root(&self.vault),
                containment: self.containment.clone(),
                record: self.record(),
            }),
            Resolution::NeedsReinstall(why) => Err(RuntimeError::NeedsReinstall {
                vendor: vendor.to_string(),
                why,
            }),
            Resolution::NotInstalled => Err(not_installed()),
        }
    }
}

/// What an adapter starts its tool from.
#[derive(Clone)]
pub enum RuntimeSource {
    /// A fixed runtime: a test with a stub program.
    Fixed(DelegateRuntime),
    /// The installation of the open vault, looked up at each start.
    Vault {
        tools: DelegateTools,
        vendor: &'static str,
    },
    /// An adapter built only to list models; it never starts a tool.
    Unwired { vendor: &'static str },
}

impl RuntimeSource {
    /// The runtime for the next run, or the reason there is none.
    pub fn resolve(&self) -> Result<DelegateRuntime, RuntimeError> {
        match self {
            Self::Fixed(runtime) => Ok(runtime.clone()),
            Self::Vault { tools, vendor } => tools.runtime(vendor),
            Self::Unwired { vendor } => Err(RuntimeError::NotInstalled {
                vendor: (*vendor).to_string(),
            }),
        }
    }
}

impl From<DelegateRuntime> for RuntimeSource {
    fn from(runtime: DelegateRuntime) -> Self {
        Self::Fixed(runtime)
    }
}

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;
