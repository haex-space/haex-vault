use std::collections::HashMap;
use std::time::Duration;

use super::config::{DosDefenceConfig, EscalationPolicy};

#[test]
fn defaults_match_plan_doc_phase_1() {
    // Defaults per docs/plans/2026-06-13-leader-reject-rate-limit.md.
    // Test cements them so future tuning is explicit.
    let cfg = DosDefenceConfig::defaults();
    assert_eq!(cfg.l1_global_rate_per_sec, 100);
    assert_eq!(cfg.l1_per_source_rate_per_sec, 10);
    assert_eq!(cfg.l2_max_streams_per_conn, 8);
    assert_eq!(cfg.l3_handshake_timeout, Duration::from_secs(5));
    assert_eq!(cfg.l4_reject_rate_threshold_per_sec, 20);
    assert_eq!(cfg.l4_sample_threshold_per_sec, 100);
    assert_eq!(cfg.ddos_distinct_sources_threshold, 10);
    assert_eq!(cfg.ddos_escalation_policy, EscalationPolicy::ContactsOnly);
    assert_eq!(cfg.ddos_auto_expiry, Duration::from_secs(1800));
}

#[test]
fn from_rows_returns_defaults_when_empty() {
    let cfg = DosDefenceConfig::from_rows(HashMap::new());
    assert_eq!(cfg, DosDefenceConfig::defaults());
}

#[test]
fn from_rows_overrides_l4_threshold_with_parsed_value() {
    let mut rows = HashMap::new();
    rows.insert(
        "dosDefence.l4.rejectRateThresholdPerSec".to_string(),
        "42".to_string(),
    );
    let cfg = DosDefenceConfig::from_rows(rows);
    assert_eq!(cfg.l4_reject_rate_threshold_per_sec, 42);
    assert_eq!(cfg.l1_global_rate_per_sec, 100); // other defaults intact
}

#[test]
fn from_rows_falls_back_to_default_on_unparseable_value() {
    let mut rows = HashMap::new();
    rows.insert(
        "dosDefence.l4.rejectRateThresholdPerSec".to_string(),
        "not a number".to_string(),
    );
    let cfg = DosDefenceConfig::from_rows(rows);
    assert_eq!(cfg.l4_reject_rate_threshold_per_sec, 20);
}

#[test]
fn from_rows_overrides_all_known_keys() {
    let rows: HashMap<String, String> = [
        ("dosDefence.l1.globalRatePerSec", "200"),
        ("dosDefence.l1.perSourceRatePerSec", "20"),
        ("dosDefence.l2.maxStreamsPerConn", "16"),
        ("dosDefence.l3.handshakeTimeoutSecs", "10"),
        ("dosDefence.l4.rejectRateThresholdPerSec", "30"),
        ("dosDefence.l4.sampleThresholdPerSec", "150"),
        ("dosDefence.ddos.distinctSourcesThreshold", "20"),
        ("dosDefence.ddos.escalationPolicy", "off"),
        ("dosDefence.ddos.autoExpirySecs", "3600"),
    ]
    .iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect();
    let cfg = DosDefenceConfig::from_rows(rows);
    assert_eq!(cfg.l1_global_rate_per_sec, 200);
    assert_eq!(cfg.l1_per_source_rate_per_sec, 20);
    assert_eq!(cfg.l2_max_streams_per_conn, 16);
    assert_eq!(cfg.l3_handshake_timeout, Duration::from_secs(10));
    assert_eq!(cfg.l4_reject_rate_threshold_per_sec, 30);
    assert_eq!(cfg.l4_sample_threshold_per_sec, 150);
    assert_eq!(cfg.ddos_distinct_sources_threshold, 20);
    assert_eq!(cfg.ddos_escalation_policy, EscalationPolicy::Off);
    assert_eq!(cfg.ddos_auto_expiry, Duration::from_secs(3600));
}

#[test]
fn from_rows_ignores_unknown_keys() {
    let mut rows = HashMap::new();
    rows.insert("dosDefence.bogus.key".to_string(), "value".to_string());
    rows.insert("totally.unrelated".to_string(), "junk".to_string());
    let cfg = DosDefenceConfig::from_rows(rows);
    assert_eq!(cfg, DosDefenceConfig::defaults());
}
