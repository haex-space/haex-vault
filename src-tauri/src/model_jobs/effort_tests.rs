//! Tests for the effort of a model job (spec 055 FR-006).

use super::effort::{effective, Applied, Level};
use crate::model_capabilities::{ModelCapabilities, ReasoningControl, ReasoningOption};

fn presets(ids: &[&str]) -> ModelCapabilities {
    ModelCapabilities {
        reasoning: Some(ReasoningControl::presets(
            ids.iter()
                .map(|id| ReasoningOption {
                    id: id.to_string(),
                    label: id.to_string(),
                })
                .collect(),
        )),
        ..ModelCapabilities::default()
    }
}

fn option(applied: &Applied) -> Option<&str> {
    applied.option_id()
}

#[test]
fn the_wish_is_capped_at_the_ceiling() {
    let caps = presets(&["low", "medium", "high"]);
    assert_eq!(
        option(&effective(Some(Level::High), Level::Medium, Some(&caps))),
        Some("medium")
    );
    assert_eq!(
        option(&effective(Some(Level::Low), Level::High, Some(&caps))),
        Some("low")
    );
    assert_eq!(
        effective(Some(Level::High), Level::Medium, Some(&caps)).reported(),
        "medium"
    );
}

#[test]
fn no_wish_means_none_which_is_the_off_switch_or_else_the_lowest_option() {
    let caps = presets(&["low", "medium", "high"]);
    let applied = effective(None, Level::High, Some(&caps));
    assert_eq!(option(&applied), Some("low"));
    // Something was switched on after all, and the provenance says what.
    assert_eq!(applied.reported(), "low");
    let caps = presets(&["off", "low", "high"]);
    let applied = effective(None, Level::High, Some(&caps));
    assert_eq!(
        (option(&applied), applied.reported()),
        (Some("off"), "none")
    );

    // An unknown provider preset keeps its reported order, but an explicit off switch still wins
    // for a `none` wish even when it is not the first option.
    let caps = presets(&["turbo", "none"]);
    assert_eq!(
        option(&effective(None, Level::High, Some(&caps))),
        Some("none")
    );
    let caps = presets(&["turbo", "off"]);
    assert_eq!(
        option(&effective(Some(Level::None), Level::High, Some(&caps))),
        Some("off")
    );
}

#[test]
fn known_names_never_exceed_the_level() {
    // `high` would exceed `medium`, so the step goes down, not up.
    let two = presets(&["low", "high"]);
    let applied = effective(Some(Level::Medium), Level::High, Some(&two));
    assert_eq!((option(&applied), applied.reported()), (Some("low"), "low"));
    let three = presets(&["low", "medium", "high"]);
    assert_eq!(
        option(&effective(Some(Level::Medium), Level::High, Some(&three))),
        Some("medium")
    );
    let five = presets(&["minimal", "low", "medium", "high", "xhigh"]);
    assert_eq!(
        option(&effective(Some(Level::Low), Level::High, Some(&five))),
        Some("low")
    );
    assert_eq!(
        option(&effective(Some(Level::Medium), Level::High, Some(&five))),
        Some("medium")
    );
    // `xhigh` and `max` lie above the highest level the user can grant.
    assert_eq!(
        option(&effective(Some(Level::High), Level::High, Some(&five))),
        Some("high")
    );
    let anthropic = presets(&["low", "medium", "high", "max"]);
    assert_eq!(
        option(&effective(
            Some(Level::Medium),
            Level::High,
            Some(&anthropic)
        )),
        Some("medium")
    );
    // All options above the level: the lowest is the nearest.
    let upper = presets(&["medium", "high"]);
    assert_eq!(
        option(&effective(Some(Level::Low), Level::High, Some(&upper))),
        Some("medium")
    );
    // The off switch is only for `none`.
    let off = presets(&["off", "low", "medium", "high"]);
    assert_eq!(
        option(&effective(Some(Level::Low), Level::High, Some(&off))),
        Some("low")
    );
    let one = presets(&["only"]);
    for level in [Level::Low, Level::Medium, Level::High] {
        assert_eq!(
            option(&effective(Some(level), Level::High, Some(&one))),
            Some("only")
        );
    }
}

#[test]
fn known_names_are_put_in_order_even_when_reported_out_of_order() {
    let shuffled = presets(&["high", "low", "max", "medium"]);
    assert_eq!(
        option(&effective(Some(Level::Low), Level::High, Some(&shuffled))),
        Some("low")
    );
    assert_eq!(
        option(&effective(Some(Level::High), Level::High, Some(&shuffled))),
        Some("high")
    );
    assert_eq!(
        option(&effective(
            Some(Level::Medium),
            Level::High,
            Some(&shuffled)
        )),
        Some("medium")
    );
}

#[test]
fn the_reported_order_stays_when_a_name_is_the_providers_own() {
    let own = presets(&["turbo", "low", "deep"]);
    // An option named like the level wins over its position.
    assert_eq!(
        option(&effective(Some(Level::Low), Level::High, Some(&own))),
        Some("low")
    );
    let own = presets(&["turbo", "fast", "deep"]);
    assert_eq!(
        option(&effective(Some(Level::Low), Level::High, Some(&own))),
        Some("turbo")
    );
    assert_eq!(
        option(&effective(Some(Level::Medium), Level::High, Some(&own))),
        Some("fast")
    );
    let two = presets(&["fast", "deep"]);
    assert_eq!(
        option(&effective(Some(Level::Medium), Level::High, Some(&two))),
        Some("deep"),
        "the upper of two"
    );
    assert_eq!(
        option(&effective(Some(Level::High), Level::High, Some(&own))),
        Some("deep")
    );
}

#[test]
fn a_model_that_regulates_itself_or_has_no_effort_gets_nothing_sent() {
    let managed = ModelCapabilities {
        reasoning: Some(ReasoningControl::ModelManaged),
        ..ModelCapabilities::default()
    };
    let managed = effective(Some(Level::High), Level::High, Some(&managed));
    assert_eq!(managed, Applied::Managed);
    assert_eq!((managed.reported(), managed.option_id()), ("managed", None));

    let unavailable = ModelCapabilities {
        reasoning: Some(ReasoningControl::Unavailable),
        ..ModelCapabilities::default()
    };
    let unavailable = effective(Some(Level::High), Level::High, Some(&unavailable));
    assert_eq!(
        (unavailable.reported(), unavailable.option_id()),
        ("none", None)
    );

    // Nothing is known about the model: the same, nothing is guessed.
    assert_eq!(
        effective(Some(Level::High), Level::High, None),
        Applied::NotSet
    );
    assert_eq!(
        effective(
            Some(Level::High),
            Level::High,
            Some(&ModelCapabilities::default())
        ),
        Applied::NotSet
    );
}

#[test]
fn a_level_is_parsed_and_printed_by_its_name() {
    for (text, level) in [
        ("none", Level::None),
        ("low", Level::Low),
        ("medium", Level::Medium),
        ("high", Level::High),
    ] {
        assert_eq!(Level::parse(text), Some(level));
        assert_eq!(level.as_str(), text);
    }
    assert_eq!(Level::parse("max"), None);
    assert!(Level::None < Level::Low && Level::Low < Level::Medium && Level::Medium < Level::High);
}
