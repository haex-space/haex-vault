//! DoS-defence configuration: keys + typed defaults + parser.
//!
//! Settings persist in `haex_vault_settings` (synced) under the
//! `dosDefence.*` key prefix. This module owns the keys (so renames are
//! grep-able) and the typed defaults (so callers don't sprinkle magic
//! numbers).
//!
//! See `docs/plans/2026-06-13-leader-reject-rate-limit.md` §Konfigurierbarkeit.

use std::collections::HashMap;
use std::time::Duration;

use serde_json::Value as JsonValue;

use crate::database::core::select_with_crdt;
use crate::database::DbConnection;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EscalationPolicy {
    /// On DDoS detection, drop non-contact connection attempts at L1
    /// until either the auto-expiry elapses or the user acknowledges.
    ContactsOnly,
    /// No automatic escalation. Notification is still emitted.
    Off,
}

/// Per-DID per-request-type rate limits for the L5 "valid-but-evil" layer.
///
/// L5 protects against authenticated peers that hammer expensive handlers
/// (SyncPull scanning big spaces, SyncPush with column-sig-verify pressure,
/// MLS fetch handlers with amplifying responses). Cheap handlers like
/// `Announce` / `MlsAckCommit` / `MlsKeyPackageCount` are once-per-session
/// or trivially cheap and are not rate-limited here (L4 already covers
/// per-DID reject-amplification).
///
/// Limits are in units of "allowed events per L5 window per verified DID".
/// The L5 window is [`DosDefenceConfig::l5_window`] (default 1 second),
/// so `sync_pull = 5` reads as "at most 5 SyncPull requests per DID per
/// second". `limit_for_op` returns `None` for handlers not enumerated
/// here — the L5 gate then treats them as `NoLimit`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandlerRateLimits {
    pub sync_pull: u32,
    pub sync_push: u32,
    pub mls_fetch_messages: u32,
    pub mls_fetch_welcomes: u32,
    pub mls_send_message: u32,
    pub submit_external_commit: u32,
}

impl HandlerRateLimits {
    pub const fn defaults() -> Self {
        Self {
            sync_pull: 5,
            sync_push: 10,
            mls_fetch_messages: 20,
            mls_fetch_welcomes: 20,
            mls_send_message: 30,
            submit_external_commit: 5,
        }
    }

    /// Look up the per-DID per-window limit for an [`super::super::protocol::Request`]
    /// variant's [`super::super::protocol::Request::op_name`]. Returns
    /// `None` for handlers not covered by the L5 layer (cheap, one-shot,
    /// or session-lifecycle requests).
    pub fn limit_for_op(&self, op_name: &str) -> Option<u32> {
        match op_name {
            "SyncPull" => Some(self.sync_pull),
            "SyncPush" => Some(self.sync_push),
            "MlsFetchMessages" => Some(self.mls_fetch_messages),
            "MlsFetchWelcomes" => Some(self.mls_fetch_welcomes),
            "MlsSendMessage" => Some(self.mls_send_message),
            "SubmitExternalCommit" => Some(self.submit_external_commit),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DosDefenceConfig {
    pub l1_global_rate_per_sec: u32,
    pub l1_per_source_rate_per_sec: u32,
    pub l2_max_streams_per_conn: u32,
    pub l3_handshake_timeout: Duration,
    pub l4_reject_rate_threshold_per_sec: u32,
    pub l4_sample_threshold_per_sec: u32,
    pub ddos_distinct_sources_threshold: u32,
    pub ddos_escalation_policy: EscalationPolicy,
    pub ddos_auto_expiry: Duration,
    /// L5 per-handler per-DID rate limits.
    pub l5_handler_limits: HandlerRateLimits,
    /// Sliding window for L5 rate accounting. Kept small (1 s) so a burst
    /// beyond the limit is rejected quickly and a well-behaved client
    /// recovers within one window.
    pub l5_window: Duration,
}

impl DosDefenceConfig {
    pub fn defaults() -> Self {
        Self {
            l1_global_rate_per_sec: 100,
            l1_per_source_rate_per_sec: 10,
            l2_max_streams_per_conn: 8,
            l3_handshake_timeout: Duration::from_secs(5),
            l4_reject_rate_threshold_per_sec: 20,
            l4_sample_threshold_per_sec: 100,
            ddos_distinct_sources_threshold: 10,
            ddos_escalation_policy: EscalationPolicy::ContactsOnly,
            ddos_auto_expiry: Duration::from_secs(1800),
            l5_handler_limits: HandlerRateLimits::defaults(),
            l5_window: Duration::from_secs(1),
        }
    }

    /// Load all `dosDefence.*` settings from `haex_vault_settings` and
    /// return a typed config. Unknown keys, missing rows, parse failures,
    /// and DB errors all fall back silently to defaults — Phase 1 favours
    /// availability over strict validation. Bad config values still leave
    /// the Leader operating with safe defaults.
    ///
    /// DB-integration coverage is deferred to the L4 wiring step (and the
    /// Phase 1 e2e tests) — this thin wrapper does not warrant the heavy
    /// `setup_test_db` machinery on its own.
    pub fn load(db: &DbConnection) -> Self {
        let sql =
            "SELECT key, value FROM haex_vault_settings WHERE key LIKE 'dosDefence.%'".to_string();
        match select_with_crdt(sql, vec![], db) {
            Ok(rows) => {
                let map = rows_to_string_map(rows);
                Self::from_rows(map)
            }
            Err(e) => {
                eprintln!("[DosDefence] config load failed, using defaults: {e}");
                Self::defaults()
            }
        }
    }

    pub fn from_rows(rows: HashMap<String, String>) -> Self {
        let mut cfg = Self::defaults();
        for (key, value) in &rows {
            match key.as_str() {
                KEY_L1_GLOBAL_RATE_PER_SEC => assign_u32(&mut cfg.l1_global_rate_per_sec, value),
                KEY_L1_PER_SOURCE_RATE_PER_SEC => {
                    assign_u32(&mut cfg.l1_per_source_rate_per_sec, value)
                }
                KEY_L2_MAX_STREAMS_PER_CONN => assign_u32(&mut cfg.l2_max_streams_per_conn, value),
                KEY_L3_HANDSHAKE_TIMEOUT_SECS => {
                    assign_duration_secs(&mut cfg.l3_handshake_timeout, value)
                }
                KEY_L4_REJECT_RATE_THRESHOLD_PER_SEC => {
                    assign_u32(&mut cfg.l4_reject_rate_threshold_per_sec, value)
                }
                KEY_L4_SAMPLE_THRESHOLD_PER_SEC => {
                    assign_u32(&mut cfg.l4_sample_threshold_per_sec, value)
                }
                KEY_DDOS_DISTINCT_SOURCES_THRESHOLD => {
                    assign_u32(&mut cfg.ddos_distinct_sources_threshold, value)
                }
                KEY_DDOS_ESCALATION_POLICY => {
                    if let Some(p) = parse_escalation_policy(value) {
                        cfg.ddos_escalation_policy = p;
                    }
                }
                KEY_DDOS_AUTO_EXPIRY_SECS => assign_duration_secs(&mut cfg.ddos_auto_expiry, value),
                KEY_L5_SYNC_PULL_PER_DID_PER_SEC => {
                    assign_u32(&mut cfg.l5_handler_limits.sync_pull, value)
                }
                KEY_L5_SYNC_PUSH_PER_DID_PER_SEC => {
                    assign_u32(&mut cfg.l5_handler_limits.sync_push, value)
                }
                KEY_L5_MLS_FETCH_MESSAGES_PER_DID_PER_SEC => {
                    assign_u32(&mut cfg.l5_handler_limits.mls_fetch_messages, value)
                }
                KEY_L5_MLS_FETCH_WELCOMES_PER_DID_PER_SEC => {
                    assign_u32(&mut cfg.l5_handler_limits.mls_fetch_welcomes, value)
                }
                KEY_L5_MLS_SEND_MESSAGE_PER_DID_PER_SEC => {
                    assign_u32(&mut cfg.l5_handler_limits.mls_send_message, value)
                }
                KEY_L5_SUBMIT_EXTERNAL_COMMIT_PER_DID_PER_SEC => {
                    assign_u32(&mut cfg.l5_handler_limits.submit_external_commit, value)
                }
                _ => {}
            }
        }
        cfg
    }
}

pub const KEY_L1_GLOBAL_RATE_PER_SEC: &str = "dosDefence.l1.globalRatePerSec";
pub const KEY_L1_PER_SOURCE_RATE_PER_SEC: &str = "dosDefence.l1.perSourceRatePerSec";
pub const KEY_L2_MAX_STREAMS_PER_CONN: &str = "dosDefence.l2.maxStreamsPerConn";
pub const KEY_L3_HANDSHAKE_TIMEOUT_SECS: &str = "dosDefence.l3.handshakeTimeoutSecs";
pub const KEY_L4_REJECT_RATE_THRESHOLD_PER_SEC: &str = "dosDefence.l4.rejectRateThresholdPerSec";
pub const KEY_L4_SAMPLE_THRESHOLD_PER_SEC: &str = "dosDefence.l4.sampleThresholdPerSec";
pub const KEY_DDOS_DISTINCT_SOURCES_THRESHOLD: &str = "dosDefence.ddos.distinctSourcesThreshold";
pub const KEY_DDOS_ESCALATION_POLICY: &str = "dosDefence.ddos.escalationPolicy";
pub const KEY_DDOS_AUTO_EXPIRY_SECS: &str = "dosDefence.ddos.autoExpirySecs";
pub const KEY_L5_SYNC_PULL_PER_DID_PER_SEC: &str = "dosDefence.l5.syncPull.perDidPerSec";
pub const KEY_L5_SYNC_PUSH_PER_DID_PER_SEC: &str = "dosDefence.l5.syncPush.perDidPerSec";
pub const KEY_L5_MLS_FETCH_MESSAGES_PER_DID_PER_SEC: &str =
    "dosDefence.l5.mlsFetchMessages.perDidPerSec";
pub const KEY_L5_MLS_FETCH_WELCOMES_PER_DID_PER_SEC: &str =
    "dosDefence.l5.mlsFetchWelcomes.perDidPerSec";
pub const KEY_L5_MLS_SEND_MESSAGE_PER_DID_PER_SEC: &str =
    "dosDefence.l5.mlsSendMessage.perDidPerSec";
pub const KEY_L5_SUBMIT_EXTERNAL_COMMIT_PER_DID_PER_SEC: &str =
    "dosDefence.l5.submitExternalCommit.perDidPerSec";

fn assign_u32(target: &mut u32, raw: &str) {
    if let Ok(n) = raw.parse() {
        *target = n;
    }
}

fn assign_duration_secs(target: &mut Duration, raw: &str) {
    if let Ok(n) = raw.parse::<u64>() {
        *target = Duration::from_secs(n);
    }
}

fn parse_escalation_policy(raw: &str) -> Option<EscalationPolicy> {
    match raw {
        "contacts_only" => Some(EscalationPolicy::ContactsOnly),
        "off" => Some(EscalationPolicy::Off),
        _ => None,
    }
}

fn rows_to_string_map(rows: Vec<Vec<JsonValue>>) -> HashMap<String, String> {
    rows.into_iter()
        .filter_map(|row| {
            let mut it = row.into_iter();
            let key = it.next()?.as_str()?.to_string();
            let value = it.next()?.as_str()?.to_string();
            Some((key, value))
        })
        .collect()
}
