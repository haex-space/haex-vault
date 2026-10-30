//! Tests for finding the agent browser (spec 065, T019) against a fake file system.

use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use super::locate::{candidates, find, major_version, new_enough, Os, Probe};

#[derive(Default)]
struct Fake {
    files: HashSet<PathBuf>,
    vars: HashMap<&'static str, OsString>,
}

impl Fake {
    fn with_var(mut self, name: &'static str, value: &str) -> Self {
        self.vars.insert(name, value.into());
        self
    }

    fn with_file(mut self, path: &str) -> Self {
        self.files.insert(PathBuf::from(path));
        self
    }
}

impl Probe for Fake {
    fn is_file(&self, path: &Path) -> bool {
        self.files.contains(path)
    }

    fn var(&self, name: &str) -> Option<OsString> {
        self.vars.get(name).cloned()
    }
}

fn found(os: Os, fake: &Fake) -> Option<String> {
    find(os, None, fake).map(|path| path.to_string_lossy().into_owned())
}

#[cfg(unix)]
#[test]
fn linux_takes_the_first_name_of_the_list_on_path() {
    let fake = Fake::default()
        .with_var("PATH", "/usr/local/bin:/usr/bin")
        .with_file("/usr/bin/google-chrome")
        .with_file("/usr/bin/chromium");
    assert_eq!(
        found(Os::Linux, &fake).as_deref(),
        Some("/usr/bin/chromium")
    );
    let only_brave = Fake::default()
        .with_var("PATH", "/usr/bin")
        .with_file("/usr/bin/brave-browser");
    assert_eq!(
        found(Os::Linux, &only_brave).as_deref(),
        Some("/usr/bin/brave-browser")
    );
}

#[cfg(unix)]
#[test]
fn macos_looks_in_both_application_folders() {
    let fake = Fake::default()
        .with_var("HOME", "/Users/anna")
        .with_file("/Users/anna/Applications/Chromium.app/Contents/MacOS/Chromium")
        .with_file("/Applications/Brave Browser.app/Contents/MacOS/Brave Browser");
    assert_eq!(
        found(Os::Macos, &fake).as_deref(),
        Some("/Users/anna/Applications/Chromium.app/Contents/MacOS/Chromium"),
        "Chromium comes before Brave in the list"
    );
    let order = candidates(Os::Macos, &fake);
    assert_eq!(
        order[0],
        PathBuf::from("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome")
    );
}

#[test]
fn windows_looks_in_the_program_folders() {
    let fake = Fake::default()
        .with_var("ProgramFiles", r"C:\Program Files")
        .with_var("ProgramFiles(x86)", r"C:\Program Files (x86)")
        .with_var("LOCALAPPDATA", r"C:\Users\anna\AppData\Local\")
        .with_file(r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe")
        .with_file(r"C:\Users\anna\AppData\Local\Google\Chrome\Application\chrome.exe");
    assert_eq!(
        found(Os::Windows, &fake).as_deref(),
        Some(r"C:\Users\anna\AppData\Local\Google\Chrome\Application\chrome.exe"),
        "Chrome comes before Edge, in any of the folders"
    );
}

#[test]
fn nothing_found_is_none() {
    let fake = Fake::default().with_var("PATH", "/usr/bin");
    for os in [Os::Linux, Os::Macos, Os::Windows] {
        assert_eq!(found(os, &fake), None);
    }
}

#[test]
fn a_chosen_path_wins_and_must_exist() {
    let fake = Fake::default()
        .with_var("PATH", "/usr/bin")
        .with_file("/usr/bin/chromium")
        .with_file("/opt/thorium/thorium");
    let chosen = PathBuf::from("/opt/thorium/thorium");
    assert_eq!(find(Os::Linux, Some(&chosen), &fake), Some(chosen));
    let gone = PathBuf::from("/opt/gone/chrome");
    assert_eq!(
        find(Os::Linux, Some(&gone), &fake),
        None,
        "a chosen browser that is gone is not replaced by another one"
    );
    assert_eq!(
        find(Os::Linux, Some(Path::new("")), &fake),
        Some(PathBuf::from("/usr/bin/chromium")),
        "an empty choice means: search"
    );
}

#[test]
fn a_version_below_120_counts_as_not_found() {
    assert_eq!(major_version("Chrome/156.0.8078.73"), Some(156));
    assert_eq!(major_version("HeadlessChrome/120.0.6099.5"), Some(120));
    assert_eq!(major_version("garbage"), None);
    assert!(new_enough("Chrome/120.0.0.0"));
    assert!(new_enough("Edg/131.0.2903.70"));
    assert!(!new_enough("Chrome/119.0.6045.105"));
    assert!(!new_enough("Chrome/"));
}
