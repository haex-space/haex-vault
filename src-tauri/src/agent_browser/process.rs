//! Starting the agent browser over `--remote-debugging-pipe` (spec 065, research R2,
//! contracts/agent-browser.md "Start"): the browser reads holzi's commands on descriptor 3 and
//! writes on descriptor 4. No debugging port is ever opened, so no other program reaches it
//! (FR-004). On Linux and macOS the descriptors are set in the child before `exec`; Windows passes
//! them through the C runtime's handle inheritance ([`super::process_windows`]).
//!
//! The browser leads its own process group, so ending it ends its helper processes too
//! ([`crate::vault_gate::kill_process_tree`]).

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::watch;

use super::cdp::{browser, target, Connection};
use super::error::{BrowserError, Result};
use super::locate::{self, Os};

/// How long the browser gets to answer `Browser.getVersion` after its start.
pub const START_TIMEOUT: Duration = Duration::from_secs(30);

/// What a start needs.
#[derive(Debug, Clone)]
pub struct Launch {
    pub executable: PathBuf,
    /// The profile's working directory (`--user-data-dir`).
    pub workdir: PathBuf,
    /// Where downloads land until holzi moves them (research R11).
    pub downloads: PathBuf,
    /// Without a window: only for tests on machines without a display.
    pub headless: bool,
}

/// The start flags of contracts/agent-browser.md for `os`, the working directory and `about:blank`.
pub fn args(os: Os, workdir: &Path, headless: bool) -> Vec<OsString> {
    let mut args: Vec<OsString> = vec![
        "--remote-debugging-pipe".into(),
        {
            let mut flag = OsString::from("--user-data-dir=");
            flag.push(workdir);
            flag
        },
        "--no-first-run".into(),
        "--no-default-browser-check".into(),
        "--disable-sync".into(),
        "--disable-extensions".into(),
        "--disable-background-networking".into(),
        "--disable-component-update".into(),
        // Without the back/forward cache every step through the history loads again, through the
        // website check and with a load event the tools wait for.
        "--disable-features=AutofillServerCommunication,Translate,BackForwardCache".into(),
    ];
    match os {
        // Chromium would otherwise ask the system keyring (research R2).
        Os::Linux => args.push("--password-store=basic".into()),
        Os::Macos => args.push("--use-mock-keychain".into()),
        Os::Windows => {}
    }
    if headless {
        args.push("--headless=new".into());
        // Headless Linux integration tests run under CI's AppArmor profile, which blocks
        // unprivileged user namespaces. Production launches are not headless and keep Chromium's
        // sandbox enabled.
        if matches!(os, Os::Linux) {
            args.push("--no-sandbox".into());
        }
    }
    args.push("about:blank".into());
    args
}

/// A running browser process.
pub struct BrowserProcess {
    pid: u32,
    /// `None` while it runs, then whether it ended successfully.
    exit: watch::Receiver<Option<bool>>,
}

impl BrowserProcess {
    /// A process whose end `exit` reports.
    pub(super) fn new(pid: u32, exit: watch::Receiver<Option<bool>>) -> Self {
        Self { pid, exit }
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// Whether it has ended, and if so, successfully.
    pub fn ended(&self) -> Option<bool> {
        *self.exit.borrow()
    }

    /// Waits until it has ended; whether that was successful.
    pub async fn wait(&self) -> bool {
        let mut exit = self.exit.clone();
        loop {
            if let Some(success) = *exit.borrow_and_update() {
                return success;
            }
            if exit.changed().await.is_err() {
                return false;
            }
        }
    }

    /// Ends the browser and its helper processes at once.
    pub fn kill(&self) {
        if self.ended().is_none() {
            crate::vault_gate::kill_process_tree(self.pid);
        }
    }
}

/// A started browser that answered.
pub struct Launched {
    pub connection: Arc<Connection>,
    pub process: BrowserProcess,
    /// `Chrome/156.0.8078.73`.
    pub product: String,
}

/// Starts the browser of `launch`, checks its version and sets it up (contracts/agent-browser.md
/// "Start"): auto-attach in flat mode, page permissions denied, downloads into their folder.
pub async fn launch(os: Os, launch: &Launch) -> Result<Launched> {
    let (connection, process) = spawn(os, launch)?;
    let connection = Arc::new(connection);
    let started = async {
        let product = tokio::time::timeout(START_TIMEOUT, browser::product(&connection))
            .await
            .map_err(|_| {
                BrowserError::Unavailable("no answer within 30 seconds of the start".to_owned())
            })??;
        if !locate::new_enough(&product) {
            return Err(BrowserError::NotFound(format!(
                "{product} is older than Chromium {}",
                locate::MIN_MAJOR
            )));
        }
        let configure = async {
            target::auto_attach(&connection, None).await?;
            browser::deny_permissions(&connection).await?;
            browser::download_into(&connection, &launch.downloads.to_string_lossy()).await?;
            Ok::<(), BrowserError>(())
        };
        tokio::time::timeout(START_TIMEOUT, configure)
            .await
            .map_err(|_| {
                BrowserError::Unavailable(
                    "browser setup did not finish within 30 seconds".to_owned(),
                )
            })??;
        Ok(product)
    }
    .await;
    match started {
        Ok(product) => Ok(Launched {
            connection,
            process,
            product,
        }),
        Err(error) => {
            connection.close();
            process.kill();
            Err(error)
        }
    }
}

#[cfg(unix)]
fn spawn(os: Os, launch: &Launch) -> Result<(Connection, BrowserProcess)> {
    use std::os::fd::AsRawFd;
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};

    let (child_reads, holzi_writes) = std::io::pipe().map_err(unavailable)?;
    let (holzi_reads, child_writes) = std::io::pipe().map_err(unavailable)?;
    let (read_fd, write_fd) = (child_reads.as_raw_fd(), child_writes.as_raw_fd());
    let mut command = Command::new(&launch.executable);
    command
        .args(args(os, &launch.workdir, launch.headless))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0);
    // Headless integration runs have no desktop session and do not need its bus address. In CI,
    // the inherited value can be malformed and Chromium reports it while starting.
    if launch.headless && matches!(os, Os::Linux) {
        command.env_remove("DBUS_SESSION_BUS_ADDRESS");
    }
    // SAFETY: the closure runs in the child between `fork` and `exec` and calls only `fcntl` and
    // `dup2`, which are async-signal-safe. Both descriptors are first moved above 9, so placing
    // one on 3 cannot overwrite the other; `dup2` clears close-on-exec on 3 and 4.
    unsafe {
        command.pre_exec(move || {
            let high_read = libc::fcntl(read_fd, libc::F_DUPFD_CLOEXEC, 10);
            let high_write = libc::fcntl(write_fd, libc::F_DUPFD_CLOEXEC, 10);
            if high_read < 0
                || high_write < 0
                || libc::dup2(high_read, 3) < 0
                || libc::dup2(high_write, 4) < 0
            {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = command.spawn().map_err(|error| {
        BrowserError::Unavailable(format!(
            "{} did not start: {error}",
            launch.executable.display()
        ))
    })?;
    // holzi keeps only its own ends; the browser's are in the child now.
    drop((child_reads, child_writes));
    let pid = child.id();
    let (report, exit) = watch::channel(None);
    std::thread::Builder::new()
        .name("agent-browser-wait".to_owned())
        .spawn(move || {
            let success = child.wait().is_ok_and(|status| status.success());
            let _ = report.send(Some(success));
        })
        .map_err(unavailable)?;
    Ok((
        Connection::new(holzi_reads, holzi_writes),
        BrowserProcess::new(pid, exit),
    ))
}

#[cfg(windows)]
fn spawn(os: Os, launch: &Launch) -> Result<(Connection, BrowserProcess)> {
    super::process_windows::spawn(
        &launch.executable,
        &args(os, &launch.workdir, launch.headless),
    )
}

#[cfg(not(any(unix, windows)))]
fn spawn(_os: Os, _launch: &Launch) -> Result<(Connection, BrowserProcess)> {
    Err(BrowserError::Unsupported)
}

fn unavailable(error: std::io::Error) -> BrowserError {
    BrowserError::Unavailable(error.to_string())
}
