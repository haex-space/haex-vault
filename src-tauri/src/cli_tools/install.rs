//! Installing one version of a tool into a vault (spec 094 FR-005 to FR-007, FR-022).
//!
//! The steps, each of which can end the attempt without touching what is installed:
//! take the install lock of the vendor and vault, check the free space, download into a staging
//! folder, check the SHA-256 against the catalog, unpack, run the program once for its version
//! under the same containment as a real run, write `install.json`, and publish with one rename.
//! An interrupted attempt leaves only a `.staging` folder, which is removed here on any error and
//! by the sweep at the next vault open after a kill. Nothing here names a vendor; everything a
//! vendor needs comes from its catalog definition.
//!
//! Log lines and records carry the vendor id, the version and a fixed code, never a path or the
//! vault's name (spec FR-019).

use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use tokio::process::Command;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::catalog::{Artifact, ArtifactKind, VendorDefinition};
use super::containment::Containment;
use super::installed::{
    self, Found, InstallMarker, Installation, InstalledFile, BACKUP_SUFFIX, STAGING_SUFFIX,
};
use super::platform::PlatformKey;
use super::record::Record;
use super::runtime::DelegateRuntime;
use super::unpack::{place_file, unpack_tar_gz, UnpackError};
use crate::error::HaexError;
use crate::models::{download, download_check, hash};

/// The lock file of a vendor in a vault: one install at a time.
const LOCK_FILE: &str = ".install.lock";
/// Where a package is downloaded to inside its staging folder, until it is unpacked.
const DOWNLOAD_FILE: &str = ".haex-download";
/// How long the start check may take.
const START_CHECK_TIMEOUT: Duration = Duration::from_secs(60);
/// How often the start check tries to start a program that is busy, and the wait between tries.
const BUSY_ATTEMPTS: u32 = 40;
const BUSY_WAIT: Duration = Duration::from_millis(50);

/// Where an install is, for the progress shown in Settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Downloading,
    Verifying,
    Unpacking,
    Publishing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    pub phase: Phase,
    pub bytes_done: u64,
    /// The bytes the download will have; 0 outside the download phase.
    pub bytes_total: u64,
}

/// Why an install, an update or a revert ended without the change the person asked for. The codes are stable and shown to the user
/// through their translations.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InstallError {
    #[error("cli_tools.unknown_vendor: no tool of this name is listed")]
    UnknownVendor,
    #[error("cli_tools.unknown_version: that version is not on the list")]
    UnknownVersion,
    #[error("cli_tools.unsupported_platform: no program of this vendor for this device")]
    UnsupportedPlatform,
    #[error("cli_tools.locked: another install of this tool runs in this vault")]
    Locked,
    #[error("cli_tools.no_space: {needed} bytes needed, {free} free")]
    NoSpace { needed: u64, free: u64 },
    #[error("cli_tools.offline: the release host cannot be reached")]
    Offline,
    #[error("cli_tools.server_error: the release host answered with status {status}")]
    ServerError { status: String },
    #[error("cli_tools.integrity_mismatch: the download does not match the listed checksum")]
    IntegrityMismatch,
    #[error("cli_tools.unpack_refused: {reason}")]
    UnpackRefused { reason: String },
    #[error("cli_tools.smoke_failed: {reason}")]
    SmokeFailed { reason: &'static str },
    #[error("cli_tools.unconfirmed_update: the version in use has not ended a run yet")]
    UnconfirmedUpdate,
    #[error("cli_tools.version_in_use: a run still uses the previous version")]
    VersionInUse,
    #[error("cli_tools.nothing_to_revert: there is no unconfirmed update to go back from")]
    NothingToRevert,
    #[error("cli_tools.in_use: a run of this tool is in progress")]
    InUse,
    #[error("cli_tools.cancelled: the install was cancelled")]
    Cancelled,
    #[error("cli_tools.io_error: {reason}")]
    Io { reason: String },
}

impl InstallError {
    /// The stable machine code, `cli_tools.<name>`.
    pub fn code(&self) -> &'static str {
        match self {
            Self::UnknownVendor => "cli_tools.unknown_vendor",
            Self::UnknownVersion => "cli_tools.unknown_version",
            Self::UnsupportedPlatform => "cli_tools.unsupported_platform",
            Self::Locked => "cli_tools.locked",
            Self::NoSpace { .. } => "cli_tools.no_space",
            Self::Offline => "cli_tools.offline",
            Self::ServerError { .. } => "cli_tools.server_error",
            Self::IntegrityMismatch => "cli_tools.integrity_mismatch",
            Self::UnpackRefused { .. } => "cli_tools.unpack_refused",
            Self::SmokeFailed { .. } => "cli_tools.smoke_failed",
            Self::UnconfirmedUpdate => "cli_tools.unconfirmed_update",
            Self::VersionInUse => "cli_tools.version_in_use",
            Self::NothingToRevert => "cli_tools.nothing_to_revert",
            Self::InUse => "cli_tools.in_use",
            Self::Cancelled => "cli_tools.cancelled",
            Self::Io { .. } => "cli_tools.io_error",
        }
    }
}

fn io_error(context: &str, error: impl std::fmt::Display) -> InstallError {
    InstallError::Io {
        reason: format!("{context}: {error}"),
    }
}

/// Everything one install needs. The catalog entry is resolved by the caller, so this takes the
/// artifact as it is (a test points it at a local server).
pub struct InstallContext {
    pub vendor: Arc<VendorDefinition>,
    pub version: String,
    pub artifact: Artifact,
    pub platform: PlatformKey,
    /// `<vault>/cli-tools/<vendor>/`.
    pub vendor_dir: PathBuf,
    /// `<vault>/home/` and `<vault>/home/run/`, for the start check.
    pub home: PathBuf,
    pub run_root: PathBuf,
    pub containment: Arc<dyn Containment>,
    pub record: Record,
}

/// Held while an install of one vendor runs in one vault; the file lock ends with it.
struct InstallLock(#[allow(dead_code)] File);

fn lock(vendor_dir: &Path) -> Result<InstallLock, InstallError> {
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(vendor_dir.join(LOCK_FILE))
        .map_err(|e| io_error("open the install lock", e))?;
    match file.try_lock() {
        Ok(()) => Ok(InstallLock(file)),
        Err(fs::TryLockError::WouldBlock) => Err(InstallError::Locked),
        Err(fs::TryLockError::Error(e)) => Err(io_error("take the install lock", e)),
    }
}

/// Installs `ctx.version` of the vendor's tool. Returns the installation that is now in place, or
/// the reason it is not; in either case no staging folder of this attempt is left behind.
pub async fn install<F>(
    ctx: &InstallContext,
    cancel: &CancellationToken,
    mut progress: F,
) -> Result<Installation, InstallError>
where
    F: FnMut(Progress) + Send,
{
    let fields = [
        ("vendor", ctx.vendor.vendor.as_str()),
        ("version", ctx.version.as_str()),
    ];
    ctx.record.append("install_started", &fields);

    let staging = ctx.vendor_dir.join(format!(
        ".{}.{}{STAGING_SUFFIX}",
        ctx.version,
        Uuid::new_v4().simple()
    ));
    let result = attempt(ctx, &staging, cancel, &mut progress).await;

    // The staging folder is gone after every outcome: published (renamed away) or removed here.
    if staging.exists() {
        let doomed = staging.clone();
        let _ = tokio::task::spawn_blocking(move || fs::remove_dir_all(doomed)).await;
    }
    match &result {
        Ok(_) => ctx.record.append("install_finished", &fields),
        Err(InstallError::Cancelled) => ctx.record.append("install_cancelled", &fields),
        Err(error) => ctx.record.append(
            "install_failed",
            &[
                fields[0],
                fields[1],
                ("code", error.code().trim_start_matches("cli_tools.")),
            ],
        ),
    }
    result
}

async fn attempt<F>(
    ctx: &InstallContext,
    staging: &Path,
    cancel: &CancellationToken,
    progress: &mut F,
) -> Result<Installation, InstallError>
where
    F: FnMut(Progress) + Send,
{
    let relative = ctx
        .vendor
        .executable_for(ctx.platform)
        .ok_or(InstallError::UnsupportedPlatform)?;
    fs::create_dir_all(&ctx.vendor_dir).map_err(|e| io_error("create the tool folder", e))?;
    let _lock = lock(&ctx.vendor_dir)?;
    check_cancelled(cancel)?;

    let needed = ctx
        .artifact
        .size
        .saturating_add(ctx.artifact.unpacked_size.unwrap_or(0));
    if let Some(free) = download_check::free_bytes(&ctx.vendor_dir) {
        if !download_check::fits(Some(needed), Some(free)) {
            return Err(InstallError::NoSpace { needed, free });
        }
    }

    fs::create_dir(staging).map_err(|e| io_error("create the staging folder", e))?;
    let executable = staging.join(relative);
    let downloaded = match ctx.artifact.kind {
        ArtifactKind::File => executable.clone(),
        ArtifactKind::TarGz => staging.join(DOWNLOAD_FILE),
    };

    download_artifact(ctx, &downloaded, cancel, progress).await?;

    progress(Progress {
        phase: Phase::Verifying,
        bytes_done: 0,
        bytes_total: 0,
    });
    verify(&downloaded, &ctx.artifact).await?;
    check_cancelled(cancel)?;

    progress(Progress {
        phase: Phase::Unpacking,
        bytes_done: 0,
        bytes_total: 0,
    });
    unpack(ctx, &downloaded, staging, &executable, cancel).await?;

    let executable_size = fs::metadata(&executable)
        .ok()
        .filter(|meta| meta.is_file())
        .map(|meta| meta.len())
        .ok_or_else(|| InstallError::UnpackRefused {
            reason: "the package does not hold its program".into(),
        })?;

    let mut directories = vec![staging.to_path_buf()];
    let mut installed_files = Vec::new();
    while let Some(directory) = directories.pop() {
        for entry in fs::read_dir(directory).map_err(|e| io_error("list installed files", e))? {
            let entry = entry.map_err(|e| io_error("list installed files", e))?;
            let kind = entry
                .file_type()
                .map_err(|e| io_error("inspect an installed file", e))?;
            if kind.is_dir() {
                directories.push(entry.path());
            } else if kind.is_file() {
                let relative = entry
                    .path()
                    .strip_prefix(staging)
                    .map_err(|e| io_error("name an installed file", e))?
                    .to_string_lossy()
                    .replace('\\', "/");
                installed_files.push(InstalledFile {
                    path: relative,
                    size: entry
                        .metadata()
                        .map_err(|e| io_error("inspect an installed file", e))?
                        .len(),
                });
            } else {
                return Err(InstallError::UnpackRefused {
                    reason: "the package contains a non-regular file".into(),
                });
            }
        }
    }

    start_check(ctx, &executable, cancel).await?;
    check_cancelled(cancel)?;

    progress(Progress {
        phase: Phase::Publishing,
        bytes_done: 0,
        bytes_total: 0,
    });
    InstallMarker::new(
        &ctx.vendor.vendor,
        &ctx.version,
        ctx.platform,
        &ctx.artifact.sha256,
        ctx.artifact.size,
        executable_size,
        installed_files,
    )
    .write(staging)
    .map_err(|e| io_error("write the install marker", e))?;
    publish(ctx, staging).await
}

fn check_cancelled(cancel: &CancellationToken) -> Result<(), InstallError> {
    if cancel.is_cancelled() {
        Err(InstallError::Cancelled)
    } else {
        Ok(())
    }
}

async fn download_artifact<F>(
    ctx: &InstallContext,
    destination: &Path,
    cancel: &CancellationToken,
    progress: &mut F,
) -> Result<(), InstallError>
where
    F: FnMut(Progress) + Send,
{
    let total = ctx.artifact.size;
    let transfer = download::download_to_file(&ctx.artifact.url, destination.to_path_buf(), |p| {
        progress(Progress {
            phase: Phase::Downloading,
            bytes_done: p.bytes_downloaded,
            bytes_total: p.bytes_total.unwrap_or(total),
        });
    });
    let outcome = tokio::select! {
        () = cancel.cancelled() => None,
        result = transfer => Some(result),
    };
    match outcome {
        None => {
            download::discard_partial(destination).await;
            Err(InstallError::Cancelled)
        }
        Some(Ok(_)) => Ok(()),
        Some(Err(error)) => Err(classify_download_error(&error)),
    }
}

/// The downloader reports a failure as text; this reads the wording it uses for the cases a user
/// can act on (`models/download.rs`: `GET <url>: <error>` when the host cannot be reached, `GET
/// <url> returned status <code>` for an error answer, `stream:` and `body ended` for a cut
/// connection). The tests of this module run the real downloader against a local server, so a
/// change of that wording fails there.
fn classify_download_error(error: &HaexError) -> InstallError {
    let reason = match error {
        HaexError::ModelDownload { reason } => reason.as_str(),
        other => return io_error("download", other),
    };
    if let Some((_, after)) = reason.split_once(" returned status ") {
        let status = after.split_whitespace().next().unwrap_or("?");
        return InstallError::ServerError {
            status: status.chars().filter(char::is_ascii_digit).collect(),
        };
    }
    if reason.starts_with("GET ")
        || reason.starts_with("stream:")
        || reason.starts_with("body ended")
    {
        return InstallError::Offline;
    }
    // A file system failure of the downloader: keep the kind, drop the path it names.
    InstallError::Io {
        reason: "the download could not be written".into(),
    }
}

async fn verify(downloaded: &Path, artifact: &Artifact) -> Result<(), InstallError> {
    let path = downloaded.to_path_buf();
    let sum = tokio::task::spawn_blocking(move || hash::sha256_file(&path))
        .await
        .map_err(|e| io_error("hash the download", e))?
        .map_err(|e| io_error("hash the download", e))?;
    let size_matches = fs::metadata(downloaded).is_ok_and(|meta| meta.len() == artifact.size);
    if sum == artifact.sha256 && size_matches {
        Ok(())
    } else {
        Err(InstallError::IntegrityMismatch)
    }
}

async fn unpack(
    ctx: &InstallContext,
    downloaded: &Path,
    staging: &Path,
    executable: &Path,
    cancel: &CancellationToken,
) -> Result<(), InstallError> {
    let result = match ctx.artifact.kind {
        ArtifactKind::File => {
            let (from, to) = (downloaded.to_path_buf(), executable.to_path_buf());
            tokio::task::spawn_blocking(move || place_file(&from, &to).map(|()| 0))
                .await
                .map_err(|e| io_error("place the program", e))?
        }
        ArtifactKind::TarGz => {
            let (archive, target) = (downloaded.to_path_buf(), staging.to_path_buf());
            let limit = ctx.artifact.unpacked_size.unwrap_or(0);
            let token = cancel.clone();
            let unpacked = tokio::task::spawn_blocking(move || {
                unpack_tar_gz(&archive, &target, limit, &token)
            })
            .await
            .map_err(|e| io_error("unpack the package", e))?;
            // The archive has served its purpose whatever the outcome.
            let _ = fs::remove_file(downloaded);
            unpacked
        }
    };
    result.map(|_| ()).map_err(|error| match error {
        UnpackError::Refused(reason) => InstallError::UnpackRefused { reason },
        UnpackError::Cancelled => InstallError::Cancelled,
        UnpackError::Io(reason) => InstallError::Io { reason },
    })
}

/// Starts `command`, trying again a few times while the program is "busy".
///
/// Linux refuses to execute a file that some process still holds open for writing. The file was
/// just written by this install, and another thread of the app that forks at that moment
/// inherits the open descriptor until its own exec closes it. The refusal ends within
/// milliseconds, so waiting a little is correct; any other error is returned at once.
async fn spawn_retrying_busy<F, Fut>(
    command: &mut Command,
    mut wait: F,
) -> std::io::Result<tokio::process::Child>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    let mut attempt = 1;
    loop {
        match command.spawn() {
            Err(error)
                if error.kind() == std::io::ErrorKind::ExecutableFileBusy
                    && attempt < BUSY_ATTEMPTS =>
            {
                attempt += 1;
                wait().await;
            }
            other => return other,
        }
    }
}

/// Runs the program once and asks for its version, under the containment a real run has. It must
/// exit successfully and name the version it was installed as (spec FR-022).
async fn start_check(
    ctx: &InstallContext,
    executable: &Path,
    cancel: &CancellationToken,
) -> Result<(), InstallError> {
    let runtime = DelegateRuntime {
        vendor: ctx.vendor.clone(),
        executable: executable.to_path_buf(),
        home: ctx.home.clone(),
        run_root: ctx.run_root.clone(),
        containment: ctx.containment.clone(),
        record: ctx.record.clone(),
        lease: None,
    };
    let run = runtime
        .run_folder()
        .map_err(|e| io_error("create the run folder for the start check", e))?;
    let mut command = Command::new(executable);
    command
        .args(&ctx.vendor.config_args)
        .arg("--version")
        .current_dir(run.path())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    runtime.contain(command.as_std_mut(), &run);

    let child = match spawn_retrying_busy(&mut command, || tokio::time::sleep(BUSY_WAIT)).await {
        Ok(child) => child,
        Err(error) => {
            // Only the kind of the error is recorded: the text of an OS error carries a path.
            let kind = format!("{:?}", error.kind());
            ctx.record.append(
                "start_check_failed",
                &[
                    ("vendor", ctx.vendor.vendor.as_str()),
                    ("version", ctx.version.as_str()),
                    ("kind", kind.as_str()),
                ],
            );
            return Err(InstallError::SmokeFailed {
                reason: "start_failed",
            });
        }
    };
    let finished = tokio::select! {
        () = cancel.cancelled() => return Err(InstallError::Cancelled),
        result = tokio::time::timeout(START_CHECK_TIMEOUT, child.wait_with_output()) => result,
    };
    let output = match finished {
        Err(_) => return Err(InstallError::SmokeFailed { reason: "timeout" }),
        Ok(Err(_)) => {
            return Err(InstallError::SmokeFailed {
                reason: "start_failed",
            })
        }
        Ok(Ok(output)) => output,
    };
    if !output.status.success() {
        return Err(InstallError::SmokeFailed {
            reason: "exit_status",
        });
    }
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let reports_version = said.split_whitespace().any(|part| {
        part.trim_matches(|character: char| {
            !character.is_ascii_alphanumeric() && !matches!(character, '.' | '-' | '+')
        }) == ctx.version
    });
    if reports_version {
        Ok(())
    } else {
        Err(InstallError::SmokeFailed {
            reason: "version_mismatch",
        })
    }
}

/// Publishes staging with one rename and retains any displaced installation until validation.
async fn publish(ctx: &InstallContext, staging: &Path) -> Result<Installation, InstallError> {
    let target = ctx.vendor_dir.join(&ctx.version);
    let aside = ctx.vendor_dir.join(format!(
        ".{}.{}{BACKUP_SUFFIX}",
        ctx.version,
        Uuid::new_v4().simple()
    ));
    let (staging, target_path, aside_path) = (staging.to_path_buf(), target.clone(), aside.clone());
    let moved_aside = tokio::task::spawn_blocking(move || -> std::io::Result<bool> {
        let had = target_path.exists();
        if had {
            fs::rename(&target_path, &aside_path)?;
        }
        if let Err(error) = fs::rename(&staging, &target_path) {
            if had {
                let _ = fs::rename(&aside_path, &target_path);
            }
            return Err(error);
        }
        Ok(had)
    })
    .await
    .map_err(|e| io_error("publish the installation", e))?
    .map_err(|e| io_error("publish the installation", e))?;
    let validated = installed::scan(&ctx.vendor_dir, &ctx.vendor, Some(ctx.platform))
        .into_iter()
        .find_map(|found| match found {
            Found::Usable(installation) if installation.version == ctx.version => {
                Some(*installation)
            }
            _ => None,
        });
    if let Some(installation) = validated {
        if moved_aside {
            let _ = tokio::task::spawn_blocking(move || fs::remove_dir_all(aside)).await;
        }
        return Ok(installation);
    }
    let target_for_restore = target.clone();
    let aside_for_restore = aside.clone();
    let _ = tokio::task::spawn_blocking(move || {
        let _ = fs::remove_dir_all(&target_for_restore);
        if moved_aside {
            let _ = fs::rename(&aside_for_restore, &target_for_restore);
        }
    })
    .await;
    Err(InstallError::Io {
        reason: "the published installation is not usable".into(),
    })
}

#[cfg(test)]
#[path = "install_tests.rs"]
mod tests;
