//! The drives of a Mac (spec 064 research R5): `diskutil` lists them as plists, read by
//! [`super::macos_plist`], and mounts, unlocks, unmounts and ejects them. A password reaches
//! `diskutil` through its stdin, never its arguments, which every process can read (FR-017).
//! Listing runs `diskutil` again only when the disks in `/dev` or the entries of `/Volumes` changed;
//! otherwise the list stays and only the free space is read again.

use std::collections::HashMap;
use std::sync::Mutex;

use super::backend::{DriveBackend, DriveError, DriveSecret, UnlockMode};
use super::macos_plist::{self, ApfsList, DiskList, VolumeInfo};
use super::{space, Drive, DriveRelease, DriveState};

/// One call of `diskutil`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action<'a> {
    List,
    ApfsList,
    Info(&'a str),
    Mount(&'a str),
    Unmount(&'a str),
    Eject(&'a str),
    /// The password follows on stdin.
    Unlock(&'a str),
}

/// The arguments of `diskutil` for `action`.
pub fn command_for(action: Action<'_>) -> Vec<&str> {
    match action {
        Action::List => vec!["list", "-plist"],
        Action::ApfsList => vec!["apfs", "list", "-plist"],
        Action::Info(id) => vec!["info", "-plist", id],
        Action::Mount(id) => vec!["mount", id],
        Action::Unmount(id) => vec!["unmount", id],
        Action::Eject(id) => vec!["eject", id],
        Action::Unlock(id) => vec!["apfs", "unlockVolume", id, "-stdinpassphrase"],
    }
}

/// What a finished `diskutil` call printed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Output {
    pub success: bool,
    pub stdout: Vec<u8>,
    pub stderr: String,
}

/// How the backend reaches `diskutil` and the folders it watches; the tests stand in for the Mac.
pub trait Diskutil: Send + Sync {
    /// Runs `diskutil` with `args`, writing `stdin` to it and closing it; `Err` when it cannot start.
    fn run(&self, args: &[&str], stdin: Option<&str>) -> Result<Output, DriveError>;

    /// The names of the disks in `/dev` and the entries of `/Volumes`, sorted.
    fn fingerprint(&self) -> Vec<String>;
}

/// `diskutil`'s text of a failed call to what the window shows. Its messages are English on every
/// system language (they go to stderr, the progress to stdout).
pub fn error_from(output: &Output) -> DriveError {
    let text = if output.stderr.trim().is_empty() {
        String::from_utf8_lossy(&output.stdout).into_owned()
    } else {
        output.stderr.clone()
    };
    let text = text.trim();
    if text.contains("dissented by") {
        DriveError::Busy
    } else if text.contains("Passphrase incorrect") {
        DriveError::UnlockFailed(text.to_owned())
    } else if text.contains("Could not find") || text.contains("Unable to find disk") {
        DriveError::Gone
    } else if text.is_empty() {
        DriveError::Failed("diskutil failed".to_owned())
    } else {
        DriveError::Failed(text.to_owned())
    }
}

pub struct MacBackend<D> {
    diskutil: D,
    /// The fingerprint the cached list was made at.
    cache: Mutex<Option<(Vec<String>, Vec<Drive>)>>,
}

impl<D: Diskutil> MacBackend<D> {
    pub fn new(diskutil: D) -> Self {
        Self {
            diskutil,
            cache: Mutex::new(None),
        }
    }

    /// The output of a call that succeeded, else the error it means.
    fn call(&self, action: Action<'_>, stdin: Option<&str>) -> Result<Vec<u8>, DriveError> {
        let output = self.diskutil.run(&command_for(action), stdin)?;
        if output.success {
            Ok(output.stdout)
        } else {
            Err(error_from(&output))
        }
    }

    fn info(&self, id: &str) -> Result<VolumeInfo, DriveError> {
        macos_plist::parse(&self.call(Action::Info(id), None)?)
            .map_err(|error| DriveError::Failed(error.to_string()))
    }

    /// Asks `diskutil` for every volume; an `info` that fails leaves that volume unknown.
    fn read(&self) -> Result<Vec<Drive>, DriveError> {
        let list: DiskList = macos_plist::parse(&self.call(Action::List, None)?)
            .map_err(|error| DriveError::Failed(error.to_string()))?;
        // Without APFS (no container at all) the list still stands.
        let apfs: ApfsList = self
            .call(Action::ApfsList, None)
            .ok()
            .and_then(|bytes| macos_plist::parse(&bytes).ok())
            .unwrap_or_default();
        let infos: HashMap<String, VolumeInfo> = macos_plist::volume_ids(&list)
            .into_iter()
            .filter_map(|id| Some((id.to_owned(), self.info(id).ok()?)))
            .collect();
        Ok(macos_plist::classify(&list, &apfs, &infos))
    }

    /// The listed drive with `id`, so a window can only act on what the list showed.
    fn listed(&self, id: &str) -> Result<Drive, DriveError> {
        self.list()?
            .into_iter()
            .find(|drive| drive.id == id)
            .ok_or(DriveError::Gone)
    }

    /// Where `id` is mounted now; after a call that mounts, the next list reads `diskutil` again.
    fn mount_point(&self, id: &str) -> Result<String, DriveError> {
        self.forget();
        self.info(id)?
            .mount_point
            .filter(|point| !point.is_empty())
            .ok_or_else(|| DriveError::Failed(format!("{id} is not mounted")))
    }

    fn forget(&self) {
        *self.cache.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}

impl<D: Diskutil> DriveBackend for MacBackend<D> {
    fn list(&self) -> Result<Vec<Drive>, DriveError> {
        let fingerprint = self.diskutil.fingerprint();
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        let mut drives = match cache.as_ref() {
            Some((seen, drives)) if *seen == fingerprint => drives.clone(),
            _ => {
                let drives = self.read()?;
                *cache = Some((fingerprint, drives.clone()));
                drives
            }
        };
        drop(cache);
        for drive in drives.iter_mut().filter(|d| d.state == DriveState::Mounted) {
            let (total, free) = drive
                .path
                .as_deref()
                .and_then(|path| space(path.as_ref()))
                .unzip();
            drive.total = total;
            drive.free = free;
        }
        Ok(drives)
    }

    fn mount(&self, id: &str) -> Result<String, DriveError> {
        let drive = self.listed(id)?;
        match drive.state {
            DriveState::Mounted => drive.path.ok_or(DriveError::Gone),
            DriveState::Locked => Err(DriveError::Unsupported),
            DriveState::Unmounted => {
                self.call(Action::Mount(id), None)?;
                self.mount_point(id)
            }
        }
    }

    /// `diskutil apfs unlockVolume` unlocks and mounts in one go.
    fn unlock(&self, id: &str, secret: &DriveSecret) -> Result<String, DriveError> {
        let drive = self.listed(id)?;
        if drive.state != DriveState::Locked {
            return self.mount(id);
        }
        self.call(Action::Unlock(id), Some(secret.expose()))?;
        match self.mount_point(id) {
            Ok(path) => Ok(path),
            // Unlocked but left unmounted: mount it as any other volume.
            Err(_) => {
                self.call(Action::Mount(id), None)?;
                self.mount_point(id)
            }
        }
    }

    /// Unmounts the volume (an encrypted APFS volume locks with it), then ejects its whole disk when
    /// the drive is removable. Unmounted, it may be pulled even when ejecting failed; the window then
    /// says it is unmounted instead.
    fn release(&self, id: &str) -> Result<bool, DriveError> {
        let drive = self.listed(id)?;
        if drive.state != DriveState::Mounted || drive.release == DriveRelease::None {
            return Err(DriveError::Unsupported);
        }
        let whole = self.info(id).ok().and_then(|info| info.whole_disk);
        self.call(Action::Unmount(id), None)?;
        self.forget();
        if drive.release != DriveRelease::Eject {
            return Ok(false);
        }
        let Some(whole) = whole else {
            return Ok(false);
        };
        Ok(self
            .call(Action::Eject(&whole), None)
            .inspect_err(|error| log::warn!("files: ejecting a drive failed: {error:?}"))
            .is_ok())
    }

    fn unlock_mode(&self) -> UnlockMode {
        UnlockMode::Passphrase
    }
}

/// The real `diskutil` of the Mac.
#[cfg(target_os = "macos")]
pub struct SystemDiskutil;

#[cfg(target_os = "macos")]
impl Diskutil for SystemDiskutil {
    fn run(&self, args: &[&str], stdin: Option<&str>) -> Result<Output, DriveError> {
        use std::io::Write;
        use std::process::{Command, Stdio};

        let mut child = Command::new("/usr/sbin/diskutil")
            .args(args)
            .stdin(if stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| match error.kind() {
                std::io::ErrorKind::NotFound => DriveError::Unsupported,
                _ => DriveError::Failed(error.to_string()),
            })?;
        if let Some(secret) = stdin {
            // Taken out of the child, the pipe closes when it drops: `diskutil` reads to its end.
            if let Some(mut pipe) = child.stdin.take() {
                pipe.write_all(secret.as_bytes())
                    .map_err(|error| DriveError::Failed(error.to_string()))?;
            }
        }
        let output = child
            .wait_with_output()
            .map_err(|error| DriveError::Failed(error.to_string()))?;
        Ok(Output {
            success: output.status.success(),
            stdout: output.stdout,
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }

    fn fingerprint(&self) -> Vec<String> {
        let names = |dir: &str, keep: fn(&str) -> bool| -> Vec<String> {
            std::fs::read_dir(dir)
                .map(|entries| {
                    entries
                        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
                        .filter(|name| keep(name))
                        .collect()
                })
                .unwrap_or_default()
        };
        let mut seen = names("/dev", |name| name.starts_with("disk"));
        seen.extend(
            names("/Volumes", |_| true)
                .into_iter()
                .map(|name| format!("/Volumes/{name}")),
        );
        seen.sort();
        seen
    }
}

#[cfg(test)]
#[path = "macos_tests.rs"]
mod tests;
