use std::collections::HashMap;
use std::time::Duration;

use super::config::{DosDefenceConfig, EscalationPolicy, HandlerRateLimits};

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
    // L5 (Phase 4) — same "cement the defaults" rationale as above.
    assert_eq!(cfg.l5_handler_limits, HandlerRateLimits::defaults());
    assert_eq!(cfg.l5_handler_limits.sync_pull, 5);
    assert_eq!(cfg.l5_handler_limits.sync_push, 10);
    assert_eq!(cfg.l5_handler_limits.mls_fetch_messages, 20);
    assert_eq!(cfg.l5_handler_limits.mls_fetch_welcomes, 20);
    assert_eq!(cfg.l5_handler_limits.mls_send_message, 30);
    assert_eq!(cfg.l5_handler_limits.submit_external_commit, 5);
    assert_eq!(cfg.l5_handler_limits.mls_fetch_key_package, 10);
    assert_eq!(cfg.l5_handler_limits.request_rejoin, 2);
    assert_eq!(cfg.l5_handler_limits.default_per_op, 10);
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
        ("dosDefence.l5.syncPull.perDidPerSec", "1"),
        ("dosDefence.l5.syncPush.perDidPerSec", "2"),
        ("dosDefence.l5.mlsFetchMessages.perDidPerSec", "3"),
        ("dosDefence.l5.mlsFetchWelcomes.perDidPerSec", "4"),
        ("dosDefence.l5.mlsSendMessage.perDidPerSec", "5"),
        ("dosDefence.l5.submitExternalCommit.perDidPerSec", "6"),
        ("dosDefence.l5.mlsFetchKeyPackage.perDidPerSec", "7"),
        ("dosDefence.l5.requestRejoin.perDidPerSec", "8"),
        ("dosDefence.l5.default.perDidPerSec", "9"),
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
    // Distinct values per key so a copy-pasted key→field mapping in
    // `from_rows` cannot pass by assigning the right number to the wrong
    // handler.
    assert_eq!(cfg.l5_handler_limits.sync_pull, 1);
    assert_eq!(cfg.l5_handler_limits.sync_push, 2);
    assert_eq!(cfg.l5_handler_limits.mls_fetch_messages, 3);
    assert_eq!(cfg.l5_handler_limits.mls_fetch_welcomes, 4);
    assert_eq!(cfg.l5_handler_limits.mls_send_message, 5);
    assert_eq!(cfg.l5_handler_limits.submit_external_commit, 6);
    assert_eq!(cfg.l5_handler_limits.mls_fetch_key_package, 7);
    assert_eq!(cfg.l5_handler_limits.request_rejoin, 8);
    assert_eq!(cfg.l5_handler_limits.default_per_op, 9);
}

#[test]
fn from_rows_ignores_unknown_keys() {
    let mut rows = HashMap::new();
    rows.insert("dosDefence.bogus.key".to_string(), "value".to_string());
    rows.insert("totally.unrelated".to_string(), "junk".to_string());
    let cfg = DosDefenceConfig::from_rows(rows);
    assert_eq!(cfg, DosDefenceConfig::defaults());
}

// ---------------------------------------------------------------------------
// `limit_for_op` — default-deny coverage
// ---------------------------------------------------------------------------

/// Every `Request::op_name()` the protocol can emit, duplicated here on
/// purpose (same double-bookkeeping rationale as the sync whitelists): adding
/// a `Request` variant without deciding its L5 policy has to fail a test
/// rather than ship an unbounded per-DID vector.
const ALL_OP_NAMES: &[&str] = &[
    "MlsUploadKeyPackages",
    "MlsFetchKeyPackage",
    "MlsSendMessage",
    "MlsFetchMessages",
    "MlsSendWelcome",
    "MlsFetchWelcomes",
    "MlsAckCommit",
    "MlsKeyPackageCount",
    "RequestRejoin",
    "SubmitExternalCommit",
    "SyncPush",
    "SyncPull",
    "SyncPullColumns",
    "Announce",
    "ClaimInvite",
    "PushInvite",
];

#[test]
fn all_op_names_are_in_sync_with_the_protocol_enum() {
    // Guards the duplication above against `Request::op_name` growing a new
    // arm: the string list must name every variant, and only those.
    let src = include_str!("../protocol.rs");
    let body = src
        .split_once("pub fn op_name(&self) -> &'static str {")
        .expect("op_name signature must exist")
        .1
        .split_once("\n    }")
        .expect("op_name body must terminate")
        .0;
    let mut from_source: Vec<&str> = body
        .lines()
        .filter_map(|l| l.rsplit_once("=> \""))
        .filter_map(|(_, rest)| rest.split_once('"'))
        .map(|(name, _)| name)
        .collect();
    from_source.sort_unstable();
    let mut expected: Vec<&str> = ALL_OP_NAMES.to_vec();
    expected.sort_unstable();
    assert_eq!(
        from_source, expected,
        "`Request::op_name` and ALL_OP_NAMES disagree. A new request variant \
         must be given an explicit L5 limit or added to \
         `HandlerRateLimits::EXEMPT_OPS`, then listed here."
    );
}

#[test]
fn every_non_exempt_op_is_capped() {
    let limits = HandlerRateLimits::defaults();
    for op in ALL_OP_NAMES {
        let limit = limits.limit_for_op(op);
        if HandlerRateLimits::EXEMPT_OPS.contains(op) {
            assert_eq!(limit, None, "{op} is exempt, so it must not be capped");
        } else {
            let cap = limit.unwrap_or_else(|| {
                panic!(
                    "{op} is neither exempt nor capped — an unbounded per-DID \
                     vector. Give it a limit or add it to EXEMPT_OPS."
                )
            });
            assert!(cap > 0, "{op} has a zero cap, which blocks it entirely");
        }
    }
}

#[test]
fn destructive_and_expensive_handlers_have_their_own_cap() {
    // These four were the concrete vectors the default-deny flip closed;
    // `MlsFetchKeyPackage` consumes a victim's KeyPackage and `RequestRejoin`
    // exports the ratchet tree, so both carry a dedicated row rather than
    // riding on `default_per_op`.
    let limits = HandlerRateLimits::defaults();
    assert_eq!(limits.limit_for_op("MlsFetchKeyPackage"), Some(10));
    assert_eq!(limits.limit_for_op("RequestRejoin"), Some(2));
    // These two ride the default, but must be capped all the same.
    assert_eq!(
        limits.limit_for_op("MlsUploadKeyPackages"),
        Some(limits.default_per_op)
    );
    assert_eq!(
        limits.limit_for_op("MlsSendWelcome"),
        Some(limits.default_per_op)
    );
}

#[test]
fn an_unknown_op_falls_back_to_the_default_cap() {
    // A handler added later — before anyone gives it a row — is capped from
    // the first commit instead of being silently unlimited.
    let limits = HandlerRateLimits::defaults();
    assert_eq!(
        limits.limit_for_op("SomeFutureHandler"),
        Some(limits.default_per_op)
    );
}
