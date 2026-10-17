//! The effort of a model job (spec 055 FR-006, `contracts/job-format.md` §Aufwand): the wish of
//! the extension, capped at what the user allowed, mapped onto what the model offers.

use serde::{Deserialize, Serialize};

use crate::model_capabilities::{ModelCapabilities, ReasoningControl};

/// An effort the user may allow and an extension may wish for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    None,
    Low,
    Medium,
    High,
}

impl Level {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "none" => Some(Level::None),
            "low" => Some(Level::Low),
            "medium" => Some(Level::Medium),
            "high" => Some(Level::High),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Level::None => "none",
            Level::Low => "low",
            Level::Medium => "medium",
            Level::High => "high",
        }
    }
}

/// What was actually applied, as the provenance names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Applied {
    /// The model has no effort to set, or nothing is known about it: nothing is sent.
    NotSet,
    /// The model regulates its effort itself: nothing is sent.
    Managed,
    /// One of the model's own options was chosen for this level.
    Option { level: Level, option_id: String },
}

impl Applied {
    /// The text of the provenance: `none`, `managed`, `low`, `medium` or `high`.
    pub fn reported(&self) -> &str {
        match self {
            Applied::NotSet => "none",
            Applied::Managed => "managed",
            Applied::Option { level, .. } => level.as_str(),
        }
    }

    /// The provider-native option to send, if any.
    pub fn option_id(&self) -> Option<&str> {
        match self {
            Applied::Option { option_id, .. } => Some(option_id),
            _ => None,
        }
    }
}

/// Caps the wish (none when absent) at `ceiling` and maps it onto the model's options.
///
/// With several options the lowest level takes the first, the highest the last, `medium` the middle
/// one (the upper of two). The options are put in order by name where all of them are known names;
/// otherwise the order the provider reported is kept, which is not promised to be ascending.
pub fn effective(
    wish: Option<Level>,
    ceiling: Level,
    capabilities: Option<&ModelCapabilities>,
) -> Applied {
    let level = wish.unwrap_or(Level::None).min(ceiling);
    match capabilities.and_then(|c| c.reasoning.as_ref()) {
        None | Some(ReasoningControl::Unavailable) => Applied::NotSet,
        Some(ReasoningControl::ModelManaged) => Applied::Managed,
        Some(ReasoningControl::Presets { options }) => {
            let mut ids: Vec<&str> = options.iter().map(|o| o.id.as_str()).collect();
            if ids.iter().all(|id| rank(id).is_some()) {
                ids.sort_by_key(|id| rank(id));
            }
            let Some(last) = ids.len().checked_sub(1) else {
                return Applied::NotSet;
            };
            let index = match level {
                Level::None | Level::Low => 0,
                Level::Medium => (ids.len() / 2).min(last),
                Level::High => last,
            };
            Applied::Option {
                level,
                option_id: ids[index].to_string(),
            }
        }
    }
}

/// The rank of a well-known effort name; `None` for a name that is the provider's own.
fn rank(id: &str) -> Option<u8> {
    Some(match id {
        "none" | "off" => 0,
        "minimal" => 1,
        "low" => 2,
        "medium" => 3,
        "high" => 4,
        "xhigh" => 5,
        "max" => 6,
        _ => return None,
    })
}
