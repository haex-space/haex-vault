//! The browser that agents drive (spec 065, ADR-0019): holzi starts a Chromium-based browser of its
//! own and steers it over a pipe; agents reach it through the native catalog actions `browser.*`
//! within the rights group `browser`.
//!
//! [`AgentBrowser`] is the managed state (T033): it starts the browser on the first call
//! (FR-003), keeps its profile between runs ([`profile_store`]) and ends it after
//! [`IDLE_LIMIT`] without a lease, when the vault closes, or when the user closes its window.

pub mod cdp;
pub mod error;
pub mod executor;
pub mod locate;
pub mod process;
pub mod process_windows;
pub mod profile;
pub mod profile_store;
pub mod prompts;
pub mod read;
pub mod session;
pub mod sites;
pub mod tab;

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use self::cdp::{browser, Connection};
use self::error::{BrowserError, Result};
use self::locate::Os;
use self::process::{BrowserProcess, Launch};
use self::profile::Workdir;
use self::profile_store::ProfileStore;
use crate::vault_gate::{ChildGuard, VaultGate};

/// How long the browser runs on without a session (FR-003).
pub const IDLE_LIMIT: Duration = Duration::from_secs(15 * 60);

/// How long the browser gets to end in order before holzi ends its process.
pub const CLOSE_GRACE: Duration = Duration::from_secs(5);

/// What a start needs; built by the caller from the open vault.
pub struct StartConfig {
    pub os: Os,
    pub executable: std::path::PathBuf,
    pub workdir: Workdir,
    pub store: ProfileStore,
    /// The vault session the browser belongs to: its close ends the browser, and the browser's
    /// process is one of its children. `None` in tests that run without a vault session.
    pub gate: Option<VaultGate>,
    /// Without a window: only for tests on machines without a display.
    pub headless: bool,
}

/// One run of the browser.
struct Running {
    connection: Arc<Connection>,
    process: Arc<BrowserProcess>,
    workdir: Workdir,
    store: Arc<ProfileStore>,
    product: String,
    /// Set when holzi ends the browser itself, so the watcher does not save a second time.
    ending: Arc<AtomicBool>,
    _child: Option<ChildGuard>,
}

#[derive(Default)]
struct Shared {
    running: tokio::sync::Mutex<Option<Running>>,
    leases: AtomicUsize,
    /// Counts lease changes, so an idle timer knows whether it is still the latest.
    lease_epoch: AtomicU64,
    /// Why the last profile could not be read, for "KI → Browser".
    profile_problem: Mutex<Option<String>>,
}

/// The agent browser of this app. Cheap to clone; every clone is the same browser.
#[derive(Clone, Default)]
pub struct AgentBrowser {
    shared: Arc<Shared>,
}

/// Keeps the browser from ending for being idle; sessions hold one each (FR-003).
pub struct Lease {
    browser: AgentBrowser,
}

impl Drop for Lease {
    fn drop(&mut self) {
        let shared = &self.browser.shared;
        let epoch = shared.lease_epoch.fetch_add(1, Ordering::SeqCst) + 1;
        if shared.leases.fetch_sub(1, Ordering::SeqCst) != 1 {
            return;
        }
        let browser = self.browser.clone();
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                tokio::time::sleep(IDLE_LIMIT).await;
                let shared = &browser.shared;
                if shared.leases.load(Ordering::SeqCst) == 0
                    && shared.lease_epoch.load(Ordering::SeqCst) == epoch
                {
                    if let Err(error) = browser.stop().await {
                        log::warn!("agent browser: ending after idle: {error}");
                    }
                }
            });
        }
    }
}

impl AgentBrowser {
    /// A lease: the browser does not end for being idle while one is held.
    pub fn lease(&self) -> Lease {
        self.shared.lease_epoch.fetch_add(1, Ordering::SeqCst);
        self.shared.leases.fetch_add(1, Ordering::SeqCst);
        Lease {
            browser: self.clone(),
        }
    }

    /// The connection of the running browser, starting it with `config` first when none runs.
    /// Callers hold a [`Lease`] while they use it; the idle end starts when the last one goes.
    pub async fn ensure(
        &self,
        config: impl FnOnce() -> Result<StartConfig>,
    ) -> Result<Arc<Connection>> {
        let mut running = self.shared.running.lock().await;
        if let Some(run) = running.as_ref() {
            if run.process.ended().is_none() && !run.connection.is_closed() {
                return Ok(Arc::clone(&run.connection));
            }
        }
        // A dead pipe does not prove the browser process ended. Retire an unusable run before
        // reusing its profile directory, or the old browser can outlive its state and race the new
        // run's cleanup. The watcher uses this same lock before saving or removing the directory.
        if let Some(run) = running.take() {
            run.ending.store(true, Ordering::SeqCst);
            let ended_cleanly = run.process.ended() == Some(true);
            if !ended_cleanly {
                run.process.kill();
                let _ = tokio::time::timeout(CLOSE_GRACE, run.process.wait()).await;
            }
            run.connection.close();
            if ended_cleanly {
                if let Err(error) = save(&run.workdir, &run.store).await {
                    log::warn!("agent browser: saving the profile before restart: {error}");
                }
            }
            let workdir = run.workdir.clone();
            let _ = tokio::task::spawn_blocking(move || workdir.remove()).await;
        }
        let run = self.start(config()?).await?;
        let connection = Arc::clone(&run.connection);
        *running = Some(run);
        Ok(connection)
    }

    /// The running browser's product, `Chrome/156.0.8078.73`; `None` when none runs.
    pub async fn product(&self) -> Option<String> {
        let running = self.shared.running.lock().await;
        running
            .as_ref()
            .filter(|run| run.process.ended().is_none())
            .map(|run| run.product.clone())
    }

    /// Why the last saved profile could not be read, if it could not.
    pub fn profile_problem(&self) -> Option<String> {
        self.shared
            .profile_problem
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    async fn start(&self, config: StartConfig) -> Result<Running> {
        if config.gate.as_ref().is_some_and(VaultGate::is_closing) {
            return Err(BrowserError::VaultClosed);
        }
        let zip = match config.store.load().await {
            Ok(zip) => {
                self.set_profile_problem(None);
                zip
            }
            Err(error) => {
                log::warn!("agent browser: starting with an empty profile: {error}");
                self.set_profile_problem(Some(error.to_string()));
                None
            }
        };
        let workdir = config.workdir.clone();
        let prepared = tokio::task::spawn_blocking(move || workdir.prepare(zip.as_deref()))
            .await
            .map_err(|e| BrowserError::Profile(e.to_string()))?;
        prepared?;
        let launch = Launch {
            executable: config.executable.clone(),
            workdir: config.workdir.profile(),
            downloads: config.workdir.downloads(),
            headless: config.headless,
        };
        let launched = match process::launch(config.os, &launch).await {
            Ok(launched) => launched,
            Err(error) => {
                let _ = config.workdir.remove();
                return Err(error);
            }
        };
        let process = Arc::new(launched.process);
        let child = config
            .gate
            .as_ref()
            .map(|gate| gate.children().register(process.pid()));
        let run = Running {
            connection: launched.connection,
            process,
            workdir: config.workdir,
            store: Arc::new(config.store),
            product: launched.product,
            ending: Arc::new(AtomicBool::new(false)),
            _child: child,
        };
        self.watch(&run);
        // ponytail: the profile is saved inside the close window of the vault (1 s before tasks
        // are aborted, spec 013). Ceiling: a large profile that takes longer is not saved on this
        // close; the one of the last orderly end (idle end, window closed) stays. Upgrade path:
        // save after each session as well.
        if let Some(gate) = config.gate {
            let browser = self.clone();
            let token = gate.token();
            let _ = gate.spawn(async move {
                token.cancelled().await;
                if let Err(error) = browser.stop().await {
                    log::warn!("agent browser: ending with the vault: {error}");
                }
            });
        }
        Ok(run)
    }

    /// Saves the profile when the browser ends by itself in order (the user closed its window),
    /// and removes the working directory however it ended (contracts/agent-browser.md "Ende").
    fn watch(&self, run: &Running) {
        let (process, ending, workdir, store, browser) = (
            Arc::clone(&run.process),
            Arc::clone(&run.ending),
            run.workdir.clone(),
            Arc::clone(&run.store),
            self.clone(),
        );
        tokio::spawn(async move {
            let success = process.wait().await;
            if ending.load(Ordering::SeqCst) {
                return;
            }
            // Serialize profile persistence and cleanup with `ensure`: after an unexpected exit it
            // may be starting a replacement that uses the same working directory.
            let mut running = browser.shared.running.lock().await;
            if !running
                .as_ref()
                .is_some_and(|run| Arc::ptr_eq(&run.process, &process))
                || ending.load(Ordering::SeqCst)
            {
                return;
            }
            if success {
                if let Err(error) = save(&workdir, &store).await {
                    log::warn!("agent browser: saving the profile: {error}");
                }
            } else {
                log::warn!("agent browser: the browser ended unexpectedly");
            }
            let _ = tokio::task::spawn_blocking(move || workdir.remove()).await;
            *running = None;
        });
    }

    /// Ends the browser in order: `Browser.close`, after [`CLOSE_GRACE`] the process, then the
    /// profile is saved and the working directory removed. Nothing runs: nothing to do.
    pub async fn stop(&self) -> Result<()> {
        let Some(run) = self.shared.running.lock().await.take() else {
            return Ok(());
        };
        run.ending.store(true, Ordering::SeqCst);
        let closed = tokio::time::timeout(CLOSE_GRACE, async {
            let _ = browser::close(&run.connection).await;
            run.process.wait().await
        })
        .await;
        if closed.is_err() {
            run.process.kill();
        }
        run.connection.close();
        let saved = if closed.is_ok() {
            save(&run.workdir, &run.store).await
        } else {
            // A browser that had to be killed may have left its files half written; the profile of
            // the last orderly end stays.
            Ok(())
        };
        let workdir = run.workdir.clone();
        let _ = tokio::task::spawn_blocking(move || workdir.remove()).await;
        saved
    }

    fn set_profile_problem(&self, problem: Option<String>) {
        *self
            .shared
            .profile_problem
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = problem;
    }
}

/// Packs the lasting parts of the profile in `workdir` and writes them to `store`.
async fn save(workdir: &Workdir, store: &ProfileStore) -> Result<()> {
    let profile = workdir.profile();
    let zip = tokio::task::spawn_blocking(move || profile::pack(&profile))
        .await
        .map_err(|e| BrowserError::Profile(e.to_string()))??;
    store.save(&zip).await
}

#[cfg(test)]
mod locate_tests;
#[cfg(test)]
mod process_windows_tests;
#[cfg(test)]
mod profile_store_tests;
#[cfg(test)]
mod profile_tests;
#[cfg(test)]
mod read_tests;
#[cfg(test)]
mod sites_tests;
