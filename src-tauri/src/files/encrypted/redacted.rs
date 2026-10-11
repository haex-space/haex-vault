//! Paths for logs (FR-027): a path that can come from an encrypted folder never reaches a log line,
//! not even at debug level. [`Redacted`] stands in for it in `Display` and `Debug`.

use std::fmt;

/// A path or name that is never printed: both formats show `<verschlüsselt>`.
#[derive(Clone, Copy)]
pub struct Redacted<'a>(pub &'a str);

const SHOWN: &str = "<verschlüsselt>";

impl fmt::Display for Redacted<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(SHOWN)
    }
}

impl fmt::Debug for Redacted<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(SHOWN)
    }
}

#[cfg(test)]
#[path = "redacted_tests.rs"]
mod tests;
