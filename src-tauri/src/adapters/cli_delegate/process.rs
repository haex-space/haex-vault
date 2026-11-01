use tokio::process::{Child, Command};

use crate::adapters::AdapterError;
use crate::vault_gate::{ChildGuard, ChildRegistry};

use super::DelegateVendor;

/// CLI delegates exist only where the device runs them (spec 043 FR-016, FR-026). Everywhere else
/// a start ends here, before any binary is looked for, with the reason `platform`, so the person
/// does not read "not installed" where installing is impossible.
pub(super) fn ensure_available(available: bool) -> Result<(), AdapterError> {
    if available {
        Ok(())
    } else {
        Err(AdapterError::Unavailable {
            reason: "platform: CLI delegates are not available on this device".to_string(),
        })
    }
}

/// Maps a `Command::spawn` failure to a distinct "backend not installed"
/// error (spec 007-cli-delegate FR-008/SC-006) when the binary itself
/// isn't found, separate from any other spawn failure (permissions,
/// resource limits, ...) or from `AdapterError::InvalidCredentials`
/// (tasks.md T019/T026 cover different failure causes on purpose). Carries
/// `vendor`'s install hint so the operator sees a next step rather than a
/// bare PATH failure.
pub(super) fn map_spawn_error(
    binary: &str,
    vendor: DelegateVendor,
    error: std::io::Error,
) -> AdapterError {
    if error.kind() == std::io::ErrorKind::NotFound {
        AdapterError::Unavailable {
            reason: format!(
                "\"{binary}\" is not installed or not on PATH — {}",
                vendor.install_hint()
            ),
        }
    } else {
        AdapterError::Http {
            reason: format!("failed to spawn \"{binary}\": {error}"),
        }
    }
}

/// Owns a delegate child and makes the process-group boundary explicit.
/// Normal and cancellation paths call `terminate_and_reap`; `Drop` remains a
/// last-resort guard for task abortion.
pub(super) struct ChildLifecycle {
    child: Child,
    /// Keeps the child registered with the vault gate while it can run, so the drain ladder and
    /// the forced end reach its process group even though they skip the `Drop` kill below.
    registration: Option<ChildGuard>,
}

impl ChildLifecycle {
    pub(super) fn spawn(command: &mut Command, children: &ChildRegistry) -> std::io::Result<Self> {
        command.kill_on_drop(true);
        let child = command.spawn()?;
        let registration = child.id().map(|pid| children.register(pid));
        Ok(Self {
            child,
            registration,
        })
    }

    pub(super) fn child_mut(&mut self) -> &mut Child {
        &mut self.child
    }

    pub(super) async fn terminate_and_reap(&mut self) {
        kill_process_group(self.child.id());
        let _ = self.child.kill().await;
        let _ = self.child.wait().await;
        // Reaped: the process id may be handed on, so it must not stay registered.
        self.registration = None;
    }
}

impl Drop for ChildLifecycle {
    fn drop(&mut self) {
        kill_process_group(self.child.id());
    }
}

pub(super) fn configure_process_group(command: &mut Command) {
    #[cfg(unix)]
    command.process_group(0);
    #[cfg(not(unix))]
    let _ = command;
}

fn kill_process_group(pid: Option<u32>) {
    #[cfg(unix)]
    if let Some(pid) = pid {
        // SAFETY: the negative pid targets the process group created by
        // `process_group(0)`; no pointers or borrowed memory are involved.
        unsafe {
            libc::kill(-(pid as libc::pid_t), libc::SIGKILL);
        }
    }
    #[cfg(not(unix))]
    let _ = pid;
}
