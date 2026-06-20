//! L4 reject-decision: pure functions that map a reject-rate count into a
//! [`LoggingMode`] (full / warning / sampled) and decide whether *this*
//! specific reject should be persisted to `haex_logs`.
//!
//! Decoupled from the AuthGate wiring so the policy is unit-testable
//! without a DB or LeaderState.

use super::config::DosDefenceConfig;

/// Sampling fraction once the sample-mode is active. Hardcoded for Phase
/// 1 — could become a `haex_vault_settings` knob later if operators want
/// finer control over log volume during a flood.
pub const SAMPLE_LOG_EVERY_N: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoggingMode {
    /// Per-peer reject rate is below the warning threshold — log every
    /// reject normally.
    Normal,
    /// Reject rate has crossed the warning threshold — still log every
    /// reject, but emit a one-shot single-source-flood notification to
    /// the user.
    Warning,
    /// Rate has crossed the sample threshold — log only 1 of
    /// [`SAMPLE_LOG_EVERY_N`] rejects to keep `haex_logs` from
    /// exploding. Notification stays active.
    Sampled,
}

pub fn classify(count: usize, cfg: &DosDefenceConfig) -> LoggingMode {
    if count > cfg.l4_sample_threshold_per_sec as usize {
        LoggingMode::Sampled
    } else if count > cfg.l4_reject_rate_threshold_per_sec as usize {
        LoggingMode::Warning
    } else {
        LoggingMode::Normal
    }
}

pub fn should_log_this_reject(count: usize, mode: LoggingMode) -> bool {
    match mode {
        LoggingMode::Normal | LoggingMode::Warning => true,
        LoggingMode::Sampled => count % SAMPLE_LOG_EVERY_N == 0,
    }
}
