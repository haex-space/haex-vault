//! The probe's step [`STEP`] (spec 044 US7, T090): what holzi reaches of the shared storage on
//! Android. Without "Zugriff auf alle Dateien" it reports `granted: false` and passes; the CI runs
//! the probe once like that. With the grant (`adb shell appops set … MANAGE_EXTERNAL_STORAGE allow`)
//! it must list [`ROOT`] and see the file the CI creates in [`WATCHED`] through the file browser's
//! own watch.

use std::path::Path;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::files::local::watch::watch_folder;
use crate::platform_probe::Step;

pub const STEP: &str = "all-files";

/// The shared storage of the first user.
pub const ROOT: &str = "/storage/emulated/0";

/// The folder below [`ROOT`] the step watches.
pub const WATCHED: &str = "Download";

/// The file the CI creates there once the watch stands.
pub const TOUCHED: &str = "probe.txt";

/// The note in holzi's cache directory that tells the CI the watch stands.
pub const WATCHING_FILE: &str = "platform-probe-watching.txt";

/// How long the step waits for the CI's file.
pub const WAIT: Duration = Duration::from_secs(60);

fn step(ok: bool, detail: String) -> Step {
    Step {
        name: STEP.to_owned(),
        ok,
        detail: Some(detail),
    }
}

/// Runs the step on `root`: `granted` is the platform's answer, `watching` runs once the watch of
/// `root/`[`WATCHED`] stands, and the step waits up to `wait` for a change of [`TOUCHED`] there.
pub fn check(
    granted: Result<bool, String>,
    root: &Path,
    watching: impl FnOnce(),
    wait: Duration,
) -> Step {
    match granted {
        Err(reason) => return step(false, format!("access unknown: {reason}")),
        Ok(false) => return step(true, "granted: false".to_owned()),
        Ok(true) => {}
    }
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries.count(),
        Err(error) => return step(false, format!("granted: true, listing failed: {error}")),
    };
    let (sender, changes) = mpsc::channel();
    let watched = watch_folder(&root.join(WATCHED), false, move |batch| {
        let _ = sender.send(batch);
    });
    let _watch = match watched {
        Ok(watch) => watch,
        Err(error) => {
            return step(
                false,
                format!("granted: true, {entries} entries, watch failed: {error}"),
            )
        }
    };
    watching();
    let deadline = Instant::now() + wait;
    while let Some(left) = deadline.checked_duration_since(Instant::now()) {
        let Ok(batch) = changes.recv_timeout(left) else {
            break;
        };
        if batch
            .iter()
            .any(|change| change.path.file_name().is_some_and(|name| name == TOUCHED))
        {
            return step(
                true,
                format!("granted: true, {entries} entries, watch event seen"),
            );
        }
    }
    step(
        false,
        format!(
            "granted: true, {entries} entries, no watch event within {} s",
            wait.as_secs()
        ),
    )
}

#[cfg(test)]
#[path = "platform_probe_files_tests.rs"]
mod tests;
