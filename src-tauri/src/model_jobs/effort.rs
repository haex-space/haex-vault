//! The effort of a model job (spec 055 FR-003, FR-036, `contracts/job-format.md` §Aufwand): the
//! user chooses it for each model, the extension has no say; holzi maps it onto what the model offers.

use serde::{Deserialize, Serialize};

use crate::model_capabilities::{ModelCapabilities, ReasoningControl};

/// An effort the user may choose for a model: the well-known names of the providers, in ascending order. A model offers the ones it has (`offered`); `minimal` counts as
/// `low`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    None,
    Low,
    Medium,
    High,
    #[serde(rename = "xhigh")]
    XHigh,
    Max,
}

impl Level {
    /// Every level, lowest first.
    pub const ALL: [Level; 6] = [
        Level::None,
        Level::Low,
        Level::Medium,
        Level::High,
        Level::XHigh,
        Level::Max,
    ];

    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|level| level.as_str() == text)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Level::None => "none",
            Level::Low => "low",
            Level::Medium => "medium",
            Level::High => "high",
            Level::XHigh => "xhigh",
            Level::Max => "max",
        }
    }
}

/// The levels a model offers, lowest first: the ones its own options name. A model whose options
/// have names of the provider's own offers `low`, `medium` and `high` (and `none` with an off
/// switch), the scale `effective` maps onto them. A model without a choice of effort offers none.
pub fn offered(capabilities: Option<&ModelCapabilities>) -> Vec<Level> {
    let Some(ReasoningControl::Presets { options }) =
        capabilities.and_then(|c| c.reasoning.as_ref())
    else {
        return Vec::new();
    };
    let ids: Vec<&str> = options.iter().map(|o| o.id.as_str()).collect();
    let mut levels: Vec<Level> = if ids.iter().all(|id| rank(id).is_some()) {
        ids.iter().filter_map(|id| level_of(id)).collect()
    } else {
        let mut own = vec![Level::Low, Level::Medium, Level::High];
        if ids.iter().any(|id| is_off(id)) {
            own.push(Level::None);
        }
        own
    };
    levels.sort();
    levels.dedup();
    levels
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
    /// The text of the provenance: `none`, `managed`, `low`, `medium`, `high`, `xhigh` or `max`.
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

/// The effort the user chose for a model, mapped onto the model's options.
pub fn applied_effort(chosen: Level, capabilities: Option<&ModelCapabilities>) -> Applied {
    effective(Some(chosen), chosen, capabilities)
}

/// Caps the wish (none when absent) at `ceiling` and maps it onto the model's options.
///
/// `none` takes the model's off switch (`none`/`off`) when it has one; the other levels never take
/// it. When every name is a known one, the options are put in order and the level takes the
/// highest option that does not exceed it (`minimal` counts as `low`), or the lowest one when all
/// exceed it. Otherwise the order the provider reported is
/// kept (it is not promised to be ascending): an option named like the level wins, else `low`
/// takes the first, `high` the last and `medium` the middle one (the upper of two).
pub fn effective(
    wish: Option<Level>,
    ceiling: Level,
    capabilities: Option<&ModelCapabilities>,
) -> Applied {
    let level = wish.unwrap_or(Level::None).min(ceiling);
    let options = match capabilities.and_then(|c| c.reasoning.as_ref()) {
        None | Some(ReasoningControl::Unavailable) => return Applied::NotSet,
        Some(ReasoningControl::ModelManaged) => return Applied::Managed,
        Some(ReasoningControl::Presets { options }) => options,
    };
    let mut ids: Vec<&str> = options.iter().map(|o| o.id.as_str()).collect();
    let all_known = ids.iter().all(|id| rank(id).is_some());
    if all_known {
        ids.sort_by_key(|id| rank(id));
    }
    let (off, on): (Vec<&str>, Vec<&str>) = ids.into_iter().partition(|id| is_off(id));
    let chosen = match (level, off.first(), on.first()) {
        (Level::None, Some(&off), _) | (_, Some(&off), None) => off,
        (_, None, None) => return Applied::NotSet,
        (_, _, Some(&lowest)) if all_known => on
            .iter()
            .rev()
            .copied()
            .find(|id| rank(id).is_some_and(|r| r <= rank_of(level)))
            .unwrap_or(lowest),
        (_, _, Some(&lowest)) => match on.iter().copied().find(|id| *id == level.as_str()) {
            Some(named) => named,
            None => match level {
                Level::None | Level::Low => lowest,
                Level::Medium => on[on.len() / 2],
                Level::High | Level::XHigh | Level::Max => on[on.len() - 1],
            },
        },
    };
    // The provenance names what was sent: the level of a known name, else the capped level (a
    // `none` wish on a model without an off switch still switches something on).
    let reported = level_of(chosen).unwrap_or(match level {
        Level::None => Level::Low,
        other => other,
    });
    Applied::Option {
        level: reported,
        option_id: chosen.to_string(),
    }
}

fn is_off(id: &str) -> bool {
    matches!(id, "none" | "off")
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

/// The highest rank a level may take.
fn rank_of(level: Level) -> u8 {
    match level {
        Level::None => 0,
        Level::Low => 2,
        Level::Medium => 3,
        Level::High => 4,
        Level::XHigh => 5,
        Level::Max => 6,
    }
}

/// The level a known name stands for (`minimal` counts as `low`).
fn level_of(id: &str) -> Option<Level> {
    Some(match rank(id)? {
        0 => Level::None,
        1 | 2 => Level::Low,
        3 => Level::Medium,
        4 => Level::High,
        5 => Level::XHigh,
        _ => Level::Max,
    })
}
