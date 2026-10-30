//! Finding the agent browser (spec 065, contracts/agent-browser.md "Finden"): a path chosen in
//! "KI → Browser" wins, otherwise the first Chromium-based browser of a fixed list per operating
//! system. The version is checked once the browser answers (`Browser.getVersion`), since only the
//! running browser says it reliably; below [`MIN_MAJOR`] it counts as not found.
//!
//! The lists take the operating system as a value, so the Windows and macOS lists are tested on
//! Linux too.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// The oldest Chromium holzi steers (the CDP methods of research R1 exist from there on).
pub const MIN_MAJOR: u32 = 120;

/// The operating systems with an agent browser (research R13).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Linux,
    Macos,
    Windows,
}

impl Os {
    /// The system holzi runs on; `None` where there is no agent browser.
    pub fn current() -> Option<Self> {
        if cfg!(target_os = "linux") {
            Some(Self::Linux)
        } else if cfg!(target_os = "macos") {
            Some(Self::Macos)
        } else if cfg!(target_os = "windows") {
            Some(Self::Windows)
        } else {
            None
        }
    }
}

/// What the search may look at: files and environment variables.
pub trait Probe {
    fn is_file(&self, path: &Path) -> bool;
    fn var(&self, name: &str) -> Option<OsString>;
}

/// The real file system and environment.
pub struct System;

impl Probe for System {
    fn is_file(&self, path: &Path) -> bool {
        path.is_file()
    }

    fn var(&self, name: &str) -> Option<OsString> {
        std::env::var_os(name)
    }
}

/// Program names on `PATH` (Linux). Flatpak and Snap wrappers break the pipe and are not listed.
const LINUX_NAMES: [&str; 6] = [
    "chromium",
    "chromium-browser",
    "google-chrome",
    "google-chrome-stable",
    "microsoft-edge",
    "brave-browser",
];

/// App bundles and the program inside them (macOS).
const MACOS_APPS: [(&str, &str); 4] = [
    ("Google Chrome.app", "Google Chrome"),
    ("Chromium.app", "Chromium"),
    ("Microsoft Edge.app", "Microsoft Edge"),
    ("Brave Browser.app", "Brave Browser"),
];

/// Programs below the program folders (Windows).
const WINDOWS_PROGRAMS: [&str; 4] = [
    r"Google\Chrome\Application\chrome.exe",
    r"Microsoft\Edge\Application\msedge.exe",
    r"BraveSoftware\Brave-Browser\Application\brave.exe",
    r"Chromium\Application\chrome.exe",
];

/// Every place the browser is looked for on `os`, in order.
pub fn candidates(os: Os, probe: &dyn Probe) -> Vec<PathBuf> {
    match os {
        Os::Linux => {
            let dirs: Vec<PathBuf> = probe
                .var("PATH")
                .map(|path| {
                    std::env::split_paths(&path)
                        .filter(|dir| {
                            !dir.starts_with("/snap") && !dir.starts_with("/var/lib/snapd")
                        })
                        .collect()
                })
                .unwrap_or_default();
            LINUX_NAMES
                .iter()
                .flat_map(|name| dirs.iter().map(move |dir| dir.join(name)))
                .collect()
        }
        Os::Macos => {
            let mut roots = vec![PathBuf::from("/Applications")];
            if let Some(home) = probe.var("HOME") {
                roots.push(PathBuf::from(home).join("Applications"));
            }
            MACOS_APPS
                .iter()
                .flat_map(|(app, program)| {
                    roots.iter().map(move |root| {
                        root.join(app).join("Contents").join("MacOS").join(program)
                    })
                })
                .collect()
        }
        Os::Windows => {
            let roots: Vec<PathBuf> = ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"]
                .iter()
                .filter_map(|name| probe.var(name).map(PathBuf::from))
                .collect();
            WINDOWS_PROGRAMS
                .iter()
                .flat_map(|program| roots.iter().map(move |root| windows_join(root, program)))
                .collect()
        }
    }
}

/// `root\program` with backslashes, also when the list is built on another system (tests).
fn windows_join(root: &Path, program: &str) -> PathBuf {
    let root = root.to_string_lossy();
    PathBuf::from(format!("{}\\{program}", root.trim_end_matches(['\\', '/'])))
}

/// The browser to start: `chosen` when it exists, else the first candidate that exists.
pub fn find(os: Os, chosen: Option<&Path>, probe: &dyn Probe) -> Option<PathBuf> {
    if let Some(chosen) = chosen.filter(|path| !path.as_os_str().is_empty()) {
        return probe.is_file(chosen).then(|| chosen.to_path_buf());
    }
    candidates(os, probe)
        .into_iter()
        .find(|path| probe.is_file(path))
}

/// The major version in a `Browser.getVersion` product (`Chrome/156.0.8078.73`,
/// `HeadlessChrome/…`, `Edg/…`); `None` when there is none.
pub fn major_version(product: &str) -> Option<u32> {
    let (_, version) = product.rsplit_once('/')?;
    version.split('.').next()?.parse().ok()
}

/// Whether the browser that reported `product` is new enough.
pub fn new_enough(product: &str) -> bool {
    major_version(product).is_some_and(|major| major >= MIN_MAJOR)
}
