use super::config::DosDefenceConfig;
use super::decision::{classify, should_log_this_reject, LoggingMode};

fn cfg() -> DosDefenceConfig {
    DosDefenceConfig::defaults()
}

#[test]
fn classify_returns_normal_below_warn_threshold() {
    // Default warn threshold = 20.
    assert_eq!(classify(1, &cfg()), LoggingMode::Normal);
    assert_eq!(classify(19, &cfg()), LoggingMode::Normal);
    assert_eq!(classify(20, &cfg()), LoggingMode::Normal);
}

#[test]
fn classify_returns_warning_between_warn_and_sample_threshold() {
    // Warn fires strictly above the warn threshold, before sample.
    assert_eq!(classify(21, &cfg()), LoggingMode::Warning);
    assert_eq!(classify(50, &cfg()), LoggingMode::Warning);
    assert_eq!(classify(100, &cfg()), LoggingMode::Warning);
}

#[test]
fn classify_returns_sampled_above_sample_threshold() {
    // Default sample threshold = 100.
    assert_eq!(classify(101, &cfg()), LoggingMode::Sampled);
    assert_eq!(classify(5000, &cfg()), LoggingMode::Sampled);
}

#[test]
fn normal_mode_always_logs() {
    assert!(should_log_this_reject(1, LoggingMode::Normal));
    assert!(should_log_this_reject(19, LoggingMode::Normal));
    assert!(should_log_this_reject(20, LoggingMode::Normal));
}

#[test]
fn warning_mode_always_logs() {
    assert!(should_log_this_reject(21, LoggingMode::Warning));
    assert!(should_log_this_reject(99, LoggingMode::Warning));
}

#[test]
fn sampled_mode_logs_every_nth() {
    // SAMPLE_LOG_EVERY_N = 20: log at counts 120, 140, ... skip the rest.
    assert!(should_log_this_reject(120, LoggingMode::Sampled));
    assert!(should_log_this_reject(140, LoggingMode::Sampled));
    assert!(!should_log_this_reject(121, LoggingMode::Sampled));
    assert!(!should_log_this_reject(139, LoggingMode::Sampled));
    assert!(!should_log_this_reject(101, LoggingMode::Sampled));
}
