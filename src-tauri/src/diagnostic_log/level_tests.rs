use log::{Level, LevelFilter};

use super::*;
use crate::diagnostic_log::TEST_LOCK;

#[test]
fn standard_lets_information_through_and_verbose_adds_detail() {
    assert_eq!(LogLevel::Standard.max_level(), LevelFilter::Info);
    assert_eq!(LogLevel::Verbose.max_level(), LevelFilter::Debug);
    assert!(LogLevel::Standard.allows("haex_lib::sync", Level::Info));
    assert!(!LogLevel::Standard.allows("haex_lib::sync", Level::Debug));
    assert!(LogLevel::Verbose.allows("haex_lib::sync", Level::Debug));
    assert!(!LogLevel::Verbose.allows("haex_lib::sync", Level::Trace));
}

#[test]
fn the_damped_crates_stay_at_warn_in_both_levels() {
    for level in [LogLevel::Standard, LogLevel::Verbose] {
        for target in [
            "iroh",
            "iroh::magicsock",
            "tracing::span",
            "zbus::connection",
        ] {
            assert!(level.allows(target, Level::Warn), "{target}");
            assert!(!level.allows(target, Level::Info), "{target}");
            assert!(!level.allows(target, Level::Debug), "{target}");
        }
    }
}

#[test]
fn a_crate_that_only_starts_with_a_damped_name_is_not_damped() {
    assert!(LogLevel::Verbose.allows("iroh_extra", Level::Debug));
    assert!(LogLevel::Verbose.allows("zbusiness", Level::Debug));
}

#[test]
fn the_app_s_own_modules_pass_at_the_level_s_own() {
    assert!(LogLevel::Verbose.allows("haex_lib::passwords::import", Level::Debug));
    assert!(LogLevel::Standard.allows("haex_lib::passwords::import", Level::Error));
}

#[test]
fn applying_a_level_changes_the_global_max_level() {
    let _guard = TEST_LOCK.blocking_lock();
    LogLevel::Verbose.apply();
    assert_eq!(log::max_level(), LevelFilter::Debug);
    LogLevel::Standard.apply();
    assert_eq!(log::max_level(), LevelFilter::Info);
}

#[test]
fn the_stored_value_round_trips_and_an_unknown_one_is_none() {
    for level in [LogLevel::Standard, LogLevel::Verbose] {
        assert_eq!(LogLevel::parse(level.as_str()), Some(level));
    }
    assert_eq!(LogLevel::parse("trace"), None);
    assert_eq!(LogLevel::parse(""), None);
}
