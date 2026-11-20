//! The child processes started for the vault (spec 013 FR-003, data-model.md `ChildRegistry`).
//!
//! Ending the process on close skips every `Drop`-based kill, so a shell the agent started would
//! outlive the vault it belonged to. The registry knows each such child, each one the leader of
//! its own process group, and ends them all when the close runs out of patience: at the abort rung
//! of the drain ladder and again right before a forced end.

use std::collections::HashSet;
use std::sync::{Arc, Mutex, MutexGuard};

// ponytail: process groups on Unix, also every group of a session the child leads, and
// `taskkill /T` on Windows. Ceiling: a descendant that left the child's session (`setsid`, or a
// double fork outside a session the child leads) survives; only cgroups or a subreaper could hold
// it, and both would reach all of haex-vault. For an extension's shell this is accepted: its
// permission already lets it do anything the user can, `systemd-run --user` too. Upgrade path: a
// Windows Job Object with kill-on-close, a cgroup per child on Linux.

/// The registered children of one app process. Cheap to clone; every clone is the same registry.
#[derive(Clone, Default)]
pub struct ChildRegistry {
    inner: Arc<Mutex<Inner>>,
}

#[derive(Default)]
struct Inner {
    live: HashSet<u32>,
    /// Set by the first `kill_all`; from then on a registration kills its child at once.
    ended: bool,
}

/// Keeps one child registered. Dropping it removes the entry, so register it for exactly as long
/// as the child can run: a process id the system has handed on must never stay listed.
pub struct ChildGuard {
    registry: ChildRegistry,
    pid: u32,
}

impl ChildRegistry {
    fn lock(&self) -> MutexGuard<'_, Inner> {
        // The state is a set and a flag, so a poisoned lock holds nothing half-written.
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Registers `pid`, the leader of its own process group. After [`ChildRegistry::kill_all`]
    /// the child is killed at once instead.
    pub fn register(&self, pid: u32) -> ChildGuard {
        let mut inner = self.lock();
        if inner.ended {
            drop(inner);
            kill_process_tree(pid);
        } else {
            inner.live.insert(pid);
        }
        ChildGuard {
            registry: self.clone(),
            pid,
        }
    }

    /// Kills every registered group and makes later registrations kill at once. Harmless to
    /// repeat. Never waits: it runs from the drain ladder and from a plain thread at the forced
    /// end.
    pub fn kill_all(&self) {
        let pids = {
            let mut inner = self.lock();
            inner.ended = true;
            std::mem::take(&mut inner.live)
        };
        for pid in pids {
            kill_process_tree(pid);
        }
    }

    /// How many children are registered right now.
    pub fn live(&self) -> usize {
        self.lock().live.len()
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        self.registry.lock().live.remove(&self.pid);
    }
}

/// The value to signal to end the whole group led by `pid`, or `None` for a pid that must never
/// be signalled. A group signal targets `-pid`: pid 1 would give `-1`, which reaches every
/// process the user owns, pid 0 gives the caller's own group, and a value above `i32::MAX` wraps
/// to a negative pid.
fn safe_group_pid(pid: u32) -> Option<i32> {
    if pid <= 1 {
        return None;
    }
    i32::try_from(pid).ok()
}

/// Ends `pid` and everything below it. Returns without waiting for the processes to go.
pub(crate) fn kill_process_tree(pid: u32) {
    let Some(group) = safe_group_pid(pid) else {
        log::error!("refusing to signal process group {pid}");
        return;
    };
    #[cfg(unix)]
    {
        // SAFETY: FFI call with a plain integer group id and a signal constant, no pointers
        // involved; `safe_group_pid` excluded every id that would name more than one group.
        unsafe {
            libc::kill(-group, libc::SIGKILL);
        }
        // A child that leads its own session (a shell on a terminal) puts each job in a group of
        // its own; those groups stay in its session.
        #[cfg(any(target_os = "linux", target_os = "android", target_os = "macos"))]
        for other in session_groups(group) {
            // SAFETY: as above; `session_groups` returns only ids above 1 other than our own group.
            unsafe {
                libc::kill(-other, libc::SIGKILL);
            }
        }
    }
    // Windows has no group signal; `taskkill /T` walks the OS's own parent-child tree instead.
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/T", "/F", "/PID", &group.to_string()])
            .output();
    }
    #[cfg(not(any(unix, windows)))]
    let _ = group;
}

/// The other process groups with a live process in the session `session`. Only a process that
/// leads its session has its id as the session id, and the id stays taken while the session has
/// members, so this never names a stranger's group.
#[cfg(any(target_os = "linux", target_os = "android"))]
fn session_groups(session: i32) -> Vec<i32> {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    // SAFETY: `getpgrp` takes no arguments and cannot fail.
    let own = unsafe { libc::getpgrp() };
    let mut groups: Vec<i32> = entries
        .filter_map(|entry| entry.ok()?.file_name().to_str()?.parse::<u32>().ok())
        .filter_map(|pid| std::fs::read_to_string(format!("/proc/{pid}/stat")).ok())
        .filter_map(|stat| group_in_session(&stat, session))
        .filter(|group| *group > 1 && *group != session && *group != own)
        .collect();
    groups.sort_unstable();
    groups.dedup();
    groups
}

/// The other process groups with a live process in the session `session`, as on Linux; macOS has
/// no `/proc`, so its process list comes from `proc_listallpids`.
#[cfg(target_os = "macos")]
fn session_groups(session: i32) -> Vec<i32> {
    // SAFETY: with no buffer `proc_listallpids` only counts the processes.
    let count = unsafe { libc::proc_listallpids(std::ptr::null_mut(), 0) };
    let Ok(count) = usize::try_from(count) else {
        return Vec::new();
    };
    // Room for processes started since the count.
    let mut pids: Vec<libc::pid_t> = vec![0; count + 64];
    let Ok(bytes) = libc::c_int::try_from(pids.len() * std::mem::size_of::<libc::pid_t>()) else {
        return Vec::new();
    };
    // SAFETY: the buffer is `bytes` long and holds `pid_t`s; the call writes at most that much and
    // returns how many ids it wrote.
    let listed = unsafe { libc::proc_listallpids(pids.as_mut_ptr().cast(), bytes) };
    pids.truncate(usize::try_from(listed).unwrap_or(0).min(pids.len()));
    // SAFETY: `getpgrp` takes no arguments and cannot fail.
    let own = unsafe { libc::getpgrp() };
    let mut groups: Vec<i32> = pids
        .into_iter()
        .filter(|pid| *pid > 1)
        // SAFETY: `getsid` and `getpgid` take a plain id and answer -1 for one that is gone.
        .filter(|pid| unsafe { libc::getsid(*pid) } == session)
        .map(|pid| unsafe { libc::getpgid(pid) })
        .filter(|group| *group > 1 && *group != session && *group != own)
        .collect();
    groups.sort_unstable();
    groups.dedup();
    groups
}

/// The process group of a `/proc/<pid>/stat` line when the process is alive (not a zombie) and
/// in `session`. The command name in parentheses may hold spaces and parentheses itself.
#[cfg(any(target_os = "linux", target_os = "android", test))]
fn group_in_session(stat: &str, session: i32) -> Option<i32> {
    // After the name: state, parent, process group, session.
    let mut fields = stat.get(stat.rfind(')')? + 1..)?.split_whitespace();
    let state = fields.next()?;
    let group = fields.nth(1)?.parse().ok()?;
    let in_session = fields.next()?.parse::<i32>().ok()? == session;
    (in_session && state != "Z" && state != "X").then_some(group)
}

#[cfg(test)]
#[path = "children_tests.rs"]
mod children_tests;
