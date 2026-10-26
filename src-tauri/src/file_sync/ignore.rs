//! Exclusions of a rule (FR-037 to FR-042, contracts/ignore-file.md): the fixed ones and the
//! patterns of the `.haex-ignore` file of each place, written as in `.gitignore`. A path is left out
//! when the fixed exclusions or the patterns of either side match it or a folder above it.

use std::path::Path;

use ignore::gitignore::{Gitignore, GitignoreBuilder};

use super::model::SyncRuleSide;

/// The name of the exclusion file in the root of a place.
pub const IGNORE_FILE: &str = ".haex-ignore";
/// The largest exclusion file in bytes.
pub const MAX_BYTES: usize = 64 * 1024;
/// The most lines of an exclusion file.
pub const MAX_LINES: usize = 2_000;
/// What every rule leaves out (contracts/ignore-file.md, point 4).
pub const FIXED: [&str; 6] = [
    ".holzi-trash/",
    ".*.holzi-part-*",
    ".*.holzi-old-*",
    ".DS_Store",
    "Thumbs.db",
    "desktop.ini",
];

/// A line of an exclusion file that is left out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineError {
    pub side: SyncRuleSide,
    /// Counted from 1.
    pub line: usize,
    pub text: String,
}

/// An exclusion file over the limits; the run stops until it is fixed (FR-038).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TooLarge {
    pub side: SyncRuleSide,
}

/// The exclusions of one run.
pub struct Exclusions {
    fixed: Gitignore,
    sides: Vec<Gitignore>,
    /// Lines that were left out, for the rule status.
    pub errors: Vec<LineError>,
}

impl Exclusions {
    /// The exclusions from the exclusion files of both places (`None` where there is none).
    pub fn new(a: Option<&str>, b: Option<&str>) -> Result<Self, TooLarge> {
        let mut errors = Vec::new();
        let mut sides = Vec::new();
        for (side, text) in [(SyncRuleSide::A, a), (SyncRuleSide::B, b)] {
            let Some(text) = text else { continue };
            check(text).map_err(|()| TooLarge { side })?;
            let mut builder = GitignoreBuilder::new("");
            for (index, line) in text.lines().enumerate() {
                if builder.add_line(None, line).is_err() {
                    errors.push(LineError {
                        side,
                        line: index + 1,
                        text: line.to_owned(),
                    });
                }
            }
            sides.push(builder.build().unwrap_or_else(|_| Gitignore::empty()));
        }
        let mut fixed = GitignoreBuilder::new("");
        for line in FIXED {
            fixed
                .add_line(None, line)
                .expect("the fixed exclusions are valid patterns");
        }
        Ok(Self {
            fixed: fixed.build().expect("the fixed exclusions build"),
            sides,
            errors,
        })
    }

    /// Whether `path` (relative, `/` between names) is left out. The exclusion file itself never is.
    pub fn excludes(&self, path: &str, is_dir: bool) -> bool {
        if path == IGNORE_FILE {
            return false;
        }
        let path = Path::new(path);
        std::iter::once(&self.fixed)
            .chain(&self.sides)
            .any(|matcher| {
                matcher
                    .matched_path_or_any_parents(path, is_dir)
                    .is_ignore()
            })
    }
}

/// Whether an exclusion file keeps to the limits; the editor refuses one that does not.
pub fn fits(text: &str) -> bool {
    check(text).is_ok()
}

fn check(text: &str) -> Result<(), ()> {
    if text.len() > MAX_BYTES || text.lines().count() > MAX_LINES {
        return Err(());
    }
    Ok(())
}

#[cfg(test)]
#[path = "ignore_tests.rs"]
mod tests;
