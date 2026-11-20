//! How a delegate process is kept inside its vault (spec 094 FR-011, FR-020).
//!
//! Every process haex-vault starts for a delegate goes through one [`Containment`]. The portable base
//! layer here, [`EnvContainment`], changes the child's environment and nothing else, so it is
//! honest about what it is: [`IsolationLevel::BestEffort`]. It moves what the measurements
//! showed must move (the home, the temp folders, the tool's own configuration variable), removes
//! only the host variables that decide which account or endpoint the tool talks to, and leaves
//! everything else of the host alone: `PATH`, locale, proxy, certificate and terminal variables
//! reach the tool unchanged. A later OS sandbox implements the same trait and reports
//! [`IsolationLevel::Enforced`] without touching the install, update, removal or run logic.
//!
//! Everything here applies to the child's environment alone; haex-vault's own environment is never
//! written to (an environment variable of the whole process is exactly what a host may already
//! rely on).

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;
use ts_rs::TS;

use super::catalog::VendorDefinition;
use super::runtime::RunFolder;

/// What the containment of one tool's processes guarantees on this platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(
    export,
    export_to = "../../src/types/bindings/",
    rename = "CliToolIsolation"
)]
pub enum IsolationLevel {
    /// The base layer redirects what it can; the residue is measured and listed in the spec.
    BestEffort,
    /// An OS sandbox blocks everything outside the vault folder.
    Enforced,
}

/// Decides how a delegate process is contained and says which level it reached.
pub trait Containment: Send + Sync {
    /// Prepares `command` (environment, and for a sandbox the wrapper) for one run in `run`.
    /// Returns the isolation level this run is under.
    fn contain(
        &self,
        command: &mut Command,
        run: &RunFolder,
        vendor: &VendorDefinition,
    ) -> IsolationLevel;

    /// The level `contain` reaches, for the status shown in Settings.
    fn level(&self) -> IsolationLevel;
}

/// Host variables the base layer never touches, listed so a test can assert they survive. A
/// host may need any of them for the tool to work at all (a proxy, a company certificate).
pub const UNTOUCHED: [&str; 16] = [
    "PATH",
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    "TERM",
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "NO_PROXY",
    "http_proxy",
    "https_proxy",
    "no_proxy",
    "SSL_CERT_FILE",
    "SSL_CERT_DIR",
    "NODE_EXTRA_CA_CERTS",
    "CURL_CA_BUNDLE",
    "COLORTERM",
];

/// The XDG base directories and where below the home each one is pointed, when the host
/// defines it. Where the host leaves one unset it stays unset: the tool derives its default
/// from the new `HOME`.
const XDG_BELOW_HOME: [(&str, &[&str]); 4] = [
    ("XDG_CONFIG_HOME", &[".config"]),
    ("XDG_CACHE_HOME", &[".cache"]),
    ("XDG_DATA_HOME", &[".local", "share"]),
    ("XDG_STATE_HOME", &[".local", "state"]),
];

/// The portable base layer: the child's environment, and nothing else.
#[derive(Debug, Default, Clone, Copy)]
pub struct EnvContainment;

impl Containment for EnvContainment {
    fn contain(
        &self,
        command: &mut Command,
        run: &RunFolder,
        vendor: &VendorDefinition,
    ) -> IsolationLevel {
        let host: Vec<(OsString, OsString)> = std::env::vars_os().collect();
        apply(command, run, vendor, &host);
        IsolationLevel::BestEffort
    }

    fn level(&self) -> IsolationLevel {
        IsolationLevel::BestEffort
    }
}

/// The six steps of the environment contract (`data-model.md`), over a snapshot of the host's
/// environment so a test needs no process-wide variable. `command` inherits the host
/// environment; only the differences are recorded on it.
fn apply(
    command: &mut Command,
    run: &RunFolder,
    vendor: &VendorDefinition,
    host: &[(OsString, OsString)],
) {
    // 2. Remove what decides which account or endpoint the tool talks to.
    for (name, _) in host {
        if vendor
            .env_remove_prefixes
            .iter()
            .any(|prefix| has_prefix(name, prefix))
        {
            command.env_remove(name);
        }
    }

    // 3. Set what must move.
    for (name, value) in home_variables(cfg!(windows), run.home()) {
        command.env(name, value);
    }
    for name in temp_variables(cfg!(windows)) {
        command.env(name, run.tmp());
    }
    command.env(&vendor.config_env, run.path());
    for name in &vendor.tmp_env {
        command.env(name, run.tmp());
    }
    for pair in &vendor.env_set {
        command.env(&pair.name, &pair.value);
    }

    // 4. An XDG variable only where the host defines it.
    for (name, below) in XDG_BELOW_HOME {
        if host_defines(host, name) {
            let mut target = run.home().to_path_buf();
            target.extend(below);
            command.env(name, target);
        }
    }
    if host_defines(host, "XDG_RUNTIME_DIR") {
        command.env("XDG_RUNTIME_DIR", run.runtime());
    }
}

/// The variables a tool derives its home from: `HOME` on unix, the account folders on Windows.
fn home_variables(windows: bool, home: &Path) -> Vec<(&'static str, PathBuf)> {
    let names: &[&'static str] = if windows {
        &["USERPROFILE", "APPDATA", "LOCALAPPDATA"]
    } else {
        &["HOME"]
    };
    names
        .iter()
        .map(|name| (*name, home.to_path_buf()))
        .collect()
}

/// The variables a tool derives its temp folder from.
fn temp_variables(windows: bool) -> &'static [&'static str] {
    if windows {
        &["TEMP", "TMP"]
    } else {
        &["TMPDIR"]
    }
}

fn host_defines(host: &[(OsString, OsString)], name: &str) -> bool {
    host.iter().any(|(key, _)| same_name(key, name))
}

/// Variable names are case-insensitive on Windows and case-sensitive elsewhere.
fn same_name(key: &OsStr, name: &str) -> bool {
    let key = key.to_string_lossy();
    if cfg!(windows) {
        key.eq_ignore_ascii_case(name)
    } else {
        key == name
    }
}

fn has_prefix(key: &OsStr, prefix: &str) -> bool {
    let key = key.to_string_lossy();
    if cfg!(windows) {
        key.len() >= prefix.len()
            && key.is_char_boundary(prefix.len())
            && key[..prefix.len()].eq_ignore_ascii_case(prefix)
    } else {
        key.starts_with(prefix)
    }
}

#[cfg(test)]
#[path = "containment_tests.rs"]
mod tests;
