//! Downloads and uploads of the agent browser (spec 065 US5, FR-031, research R11): the rules,
//! without a browser, so they have unit tests. The browser saves a download under its GUID in the
//! working directory; holzi moves it, under a safe and unique name, into the folder
//! "Agenten-Downloads" of the vault inside the system's downloads folder, where the agent's file
//! rights (044) reach it like any file of the device. A download over [`LIMIT`] is kept only after
//! the user allowed it. An upload takes only a file the agent's file rights reach, and says the same
//! for a file that is not there as for one it may not take.

use std::fs::OpenOptions;
use std::io;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::cdp::Event;
use crate::caller::Caller;
use crate::files::access::{check, AgentGrants, Target, Verdict, Want};
use crate::files::local::{resolve, OwnPlaces};

/// From this size on, a download asks the user (FR-031).
pub const LIMIT: u64 = 100 * 1000 * 1000;

/// The folder in the system's downloads folder; one folder per vault inside it.
pub const FOLDER: &str = "Agenten-Downloads";

/// The name of a download that brings none.
const FALLBACK_NAME: &str = "download";

/// The most characters of a file name.
const MAX_NAME_CHARS: usize = 150;

/// Names Windows keeps for devices, with any extension.
const RESERVED: &[&str] = &[
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// The folder of the vault `vault` in `downloads`, the system's downloads folder.
pub fn folder(downloads: &Path, vault: &str) -> PathBuf {
    downloads.join(FOLDER).join(safe_name(vault))
}

/// A file name every operating system accepts, from the name a website suggested: no folders, no
/// characters a system forbids, no device name of Windows, at most [`MAX_NAME_CHARS`].
pub fn safe_name(suggested: &str) -> String {
    let last = suggested.rsplit(['/', '\\']).next().unwrap_or_default();
    let cleaned: String = last
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*') {
                '_'
            } else {
                c
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches(|c: char| c.is_whitespace() || c == '.');
    let mut name = if trimmed.is_empty() {
        FALLBACK_NAME.to_owned()
    } else {
        trimmed.to_owned()
    };
    let stem = split(&name).0.split('.').next().unwrap_or_default();
    if RESERVED.contains(&stem.to_lowercase().as_str()) {
        name = format!("_{name}");
    }
    if name.chars().count() > MAX_NAME_CHARS {
        let (stem, extension) = split(&name);
        let keep = MAX_NAME_CHARS.saturating_sub(extension.chars().count());
        name = stem.chars().take(keep).collect::<String>() + extension;
    }
    name
}

/// The name before its extension, and the extension with its dot (`""` without one).
fn split(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(dot) if dot > 0 && name.len() - dot <= 16 => (&name[..dot], &name[dot..]),
        _ => (name, ""),
    }
}

/// The `n`-th name for `name` when the first ones are taken: `Rechnung (2).pdf`.
pub fn numbered(name: &str, n: u32) -> String {
    if n <= 1 {
        return name.to_owned();
    }
    let (stem, extension) = split(name);
    format!("{stem} ({n}){extension}")
}

/// Moves the finished download `source` into `dir` under `name`, or a numbered name when that is
/// taken; returns where it went and its size. The name is taken with an empty file first, so two
/// downloads of the same name at once never overwrite each other.
pub fn move_into(source: &Path, dir: &Path, name: &str) -> io::Result<(PathBuf, u64)> {
    std::fs::create_dir_all(dir)?;
    let target = (1..=10_000)
        .map(|n| dir.join(numbered(name, n)))
        .find(|candidate| {
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(candidate)
                .is_ok()
        })
        .ok_or_else(|| io::Error::other("no free name for the download"))?;
    if std::fs::rename(source, &target).is_err() {
        // Another file system: copy, then remove the original.
        let copied = std::fs::copy(source, &target);
        if let Err(error) = copied {
            let _ = std::fs::remove_file(&target);
            return Err(error);
        }
        let _ = std::fs::remove_file(source);
    }
    let size = std::fs::metadata(&target)?.len();
    Ok((target, size))
}

/// What the browser reports about a download (`Browser.downloadWillBegin`,
/// `Browser.downloadProgress`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Report {
    /// A download began in one of the frames holzi watches.
    Began { guid: String, name: String },
    Progress {
        guid: String,
        /// `0` when the website did not say.
        total: u64,
        received: u64,
        state: State,
        /// Where the browser saved it, when it says.
        file: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    InProgress,
    Completed,
    Canceled,
}

/// The report in `event`; a download that began in a frame not in `frames` is none.
pub fn report_of(event: &Event, frames: &[String]) -> Option<Report> {
    let params = &event.params;
    let text = |key: &str| params[key].as_str().map(str::to_owned);
    let bytes = |key: &str| {
        params[key]
            .as_f64()
            .map_or(0, |value| value.max(0.0) as u64)
    };
    match event.method.as_str() {
        "Browser.downloadWillBegin" => {
            let frame = text("frameId")?;
            if !frames.contains(&frame) {
                return None;
            }
            Some(Report::Began {
                guid: text("guid")?,
                name: text("suggestedFilename").unwrap_or_default(),
            })
        }
        "Browser.downloadProgress" => Some(Report::Progress {
            guid: text("guid")?,
            total: bytes("totalBytes"),
            received: bytes("receivedBytes"),
            state: match params["state"].as_str() {
                Some("completed") => State::Completed,
                Some("canceled") => State::Canceled,
                _ => State::InProgress,
            },
            file: text("filePath").filter(|path| !path.is_empty()),
        }),
        _ => None,
    }
}

/// What to do after a report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Not about this download; wait for the next report.
    Wait,
    /// The download goes on.
    Progress,
    /// The download began.
    Began,
    /// It is larger than [`LIMIT`]: ask the user, with its size as far as known.
    Ask(Option<u64>),
    /// It is complete.
    Completed,
    /// The browser or the website ended it.
    Canceled,
}

/// Follows the one download a tool set off.
#[derive(Debug, Default)]
pub struct Tracker {
    pub guid: Option<String>,
    pub name: String,
    pub total: u64,
    pub received: u64,
    pub asked: bool,
    pub completed: bool,
    pub file: Option<String>,
}

impl Tracker {
    /// Takes in `report`; the first download that begins is the one, any other is ignored.
    pub fn feed(&mut self, report: Report) -> Step {
        match report {
            Report::Began { guid, name } => {
                if self.guid.is_some() {
                    return Step::Wait;
                }
                self.guid = Some(guid);
                self.name = name;
                Step::Began
            }
            Report::Progress {
                guid,
                total,
                received,
                state,
                file,
            } => {
                if self.guid.as_deref() != Some(guid.as_str()) || self.completed {
                    return Step::Wait;
                }
                self.total = self.total.max(total);
                self.received = self.received.max(received);
                if file.is_some() {
                    self.file = file;
                }
                if state == State::Canceled {
                    return Step::Canceled;
                }
                self.completed = state == State::Completed;
                if !self.asked && (self.total > LIMIT || self.received > LIMIT) {
                    self.asked = true;
                    return Step::Ask(Some(self.total.max(self.received)));
                }
                if self.completed {
                    Step::Completed
                } else {
                    Step::Progress
                }
            }
        }
    }
}

/// Why an upload does not take a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UploadRefusal {
    /// Not an absolute path of this device.
    Invalid,
    /// The agent's file rights do not reach it, or it is not there: the agent cannot tell which.
    NotAllowed,
}

/// The file at `path` an upload of `caller` may take: the resolved path, once the agent's file
/// rights (044) reach it and it is a file.
pub fn upload_path(
    caller: &Caller,
    path: &str,
    own: &OwnPlaces,
    grants: &AgentGrants,
) -> Result<PathBuf, UploadRefusal> {
    let spelled = Path::new(path);
    if !spelled.is_absolute() {
        return Err(UploadRefusal::Invalid);
    }
    let real = resolve(spelled).map_err(|_| UploadRefusal::NotAllowed)?;
    match check(
        caller,
        &Target::Device(real.clone()),
        Want::Read,
        own,
        grants,
    ) {
        Verdict::Allowed if real.is_file() => Ok(real),
        _ => Err(UploadRefusal::NotAllowed),
    }
}

/// The frames of a tab from `Page.getFrameTree`: its own and those inside it.
pub fn frames_of(tree: &Value) -> Vec<String> {
    let mut frames = Vec::new();
    let mut open = vec![&tree["frameTree"]];
    while let Some(node) = open.pop() {
        if let Some(id) = node["frame"]["id"].as_str() {
            frames.push(id.to_owned());
        }
        if let Some(children) = node["childFrames"].as_array() {
            open.extend(children);
        }
    }
    frames
}
