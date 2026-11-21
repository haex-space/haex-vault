//! The two levels of the diagnostic log and what each lets through (spec 098 FR-001, FR-002,
//! FR-012). "Standard" is warnings, errors and information; "Ausführlich" adds detail lines.

use log::{Level, LevelFilter};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::rotation::Limits;

/// Third-party crates that flood a verbose log. They stay at `Warn` at both levels, whatever the
/// person chooses (spec FR-012; research R2). The list starts with what the dev log showed and
/// grows only after a measured session (task T038).
pub const DAMPED: [&str; 3] = ["iroh", "tracing::span", "zbus"];

/// How much the log of a vault records.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(
    export,
    export_to = "../../src/types/bindings/",
    rename = "DiagnosticLogLevel"
)]
pub enum LogLevel {
    /// Warnings, errors and information.
    Standard,
    /// Adds detail lines; ends by itself (see `expiry`).
    Verbose,
}

impl LogLevel {
    /// The value stored in the level table.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::Verbose => "verbose",
        }
    }

    /// The level a stored value names; `None` for anything else.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "standard" => Some(Self::Standard),
            "verbose" => Some(Self::Verbose),
            _ => None,
        }
    }

    /// The global maximum level of the `log` facade: a `debug!` call above it costs one comparison.
    pub const fn max_level(self) -> LevelFilter {
        match self {
            Self::Standard => LevelFilter::Info,
            Self::Verbose => LevelFilter::Debug,
        }
    }

    /// The size the log of this level may take.
    pub const fn limits(self) -> Limits {
        match self {
            Self::Standard => Limits::STANDARD,
            Self::Verbose => Limits::VERBOSE,
        }
    }

    /// Whether a record of `level` from `target` is written at this level.
    pub fn allows(self, target: &str, level: Level) -> bool {
        let ceiling = if is_damped(target) {
            LevelFilter::Warn
        } else {
            self.max_level()
        };
        level <= ceiling
    }

    /// Makes this level the one the `log` macros check first, at once and without a restart.
    pub fn apply(self) {
        log::set_max_level(self.max_level());
    }
}

/// Whether `target` is a damped crate or a module of one.
fn is_damped(target: &str) -> bool {
    DAMPED.iter().any(|module| {
        target
            .strip_prefix(module)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with("::"))
    })
}

#[cfg(test)]
#[path = "level_tests.rs"]
mod tests;
