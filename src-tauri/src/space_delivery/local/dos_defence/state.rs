//! Persistence + one-shot notification for the Phase 3 flood-mode state
//! machine.
//!
//! The pure logic lives in [`super::flood_mode`]; this module owns the side
//! effects: read/write the `haex_dos_defence_state_no_sync` singleton row
//! and emit a `FLOOD_DDOS` row into `haex_critical_notifications_no_sync`
//! exactly once per DDoS episode.
//!
//! The state machine does not touch the system clock — the row stores
//! `ddos_expires_at` as RFC3339 for forensic readability while the running
//! state machine uses monotonic [`Instant`]s; we re-derive the `Instant`
//! deadline from the parsed timestamp on load.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

use serde_json::Value as JsonValue;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::critical::sink::CriticalNotificationSink;
use crate::critical::CriticalFailureCode;
use crate::database::core::{execute, select};
use crate::database::DbConnection;

use super::contacts::ContactResolver;
use super::flood_mode::{apply, evaluate, FloodMode, FloodObservation, FloodThresholds};
use super::tracker::RejectRateTracker;

const TABLE: &str = "haex_dos_defence_state_no_sync";
const FLOOD_DDOS_LOCATION: &str = "dos_defence::flood_mode::ddos";

/// Owns all Phase 3 mutable state attached to the accept loop. Created once
/// per vault session by the wiring layer and dropped on session teardown
/// (the runtime's `Mutex`es never outlive the process).
pub struct DosDefenceRuntime {
    current: Mutex<FloodMode>,
    resolver: ContactResolver,
    ddos_notified: Mutex<bool>,
    db: DbConnection,
    sink: CriticalNotificationSink,
}

impl DosDefenceRuntime {
    /// Construct from persisted state. Loads the singleton row (or seeds it
    /// with a `quiet` row on first run). `sink` is cloned via its internal
    /// `Arc<Mutex<Connection>>`, so callers can hand in a snapshot taken
    /// under `AppState::critical_sink` without keeping the slot lock held.
    pub fn load(db: DbConnection, sink: CriticalNotificationSink) -> Self {
        let current = load_state(&db).unwrap_or_else(|e| {
            eprintln!("[DosDefence Phase 3] load failed, defaulting to Quiet: {e}");
            FloodMode::Quiet
        });
        Self {
            current: Mutex::new(current),
            resolver: ContactResolver::new(),
            ddos_notified: Mutex::new(false),
            db,
            sink,
        }
    }

    /// Returns the current FloodMode snapshot. Cheap clone; callers use the
    /// snapshot to dispatch the L1 Ddos-Mode check.
    pub fn snapshot(&self) -> FloodMode {
        self.current
            .lock()
            .map(|m| m.clone())
            .unwrap_or(FloodMode::Quiet)
    }

    pub fn contacts(&self) -> &ContactResolver {
        &self.resolver
    }

    /// Borrow the DB handle for the contacts resolver. Cheap clone of an
    /// `Arc<Mutex<_>>`; exposed so the accept-loop hot path can call
    /// `contacts().is_contact(&db, …)` without going back to PeerState.
    pub fn db(&self) -> DbConnection {
        DbConnection(self.db.0.clone())
    }

    /// Re-evaluate against fresh tracker observations and apply any
    /// transition (persistence + one-shot notification). Idempotent: returns
    /// the new state for diagnostics.
    pub fn evaluate_and_persist(
        &self,
        tracker: &RejectRateTracker,
        thresholds: FloodThresholds,
        now: Instant,
        accept_tracker_global_key: &str,
    ) -> FloodMode {
        let obs = FloodObservation {
            global_count: tracker.count_within_window(accept_tracker_global_key, now),
            // distinct_keys_count includes the global bucket itself when it
            // is non-empty; the caller's per-source decision wants only the
            // actual source-key buckets. Subtract one when the global is
            // counted, but never go below zero.
            distinct_sources: tracker.distinct_keys_count(now).saturating_sub(usize::from(
                tracker.count_within_window(accept_tracker_global_key, now) > 0,
            )),
        };

        let prev = self.snapshot();
        let transition = evaluate(&prev, obs, thresholds, now);
        let next = apply(prev.clone(), &transition);

        if let Ok(mut current) = self.current.lock() {
            *current = next.clone();
        }

        match &transition {
            super::flood_mode::FloodTransition::NoChange => {}
            super::flood_mode::FloodTransition::EnteredDdos {
                source_count,
                expires_at,
            } => {
                let expires_rfc = instant_to_rfc3339_estimate(now, *expires_at);
                if let Err(e) = persist_ddos(&self.db, *source_count, &expires_rfc) {
                    eprintln!("[DosDefence Phase 3] persist ddos failed: {e}");
                }
                self.emit_ddos_notification(*source_count, &expires_rfc);
            }
            super::flood_mode::FloodTransition::Expired => {
                if let Err(e) = persist_quiet(&self.db) {
                    eprintln!("[DosDefence Phase 3] persist quiet failed: {e}");
                }
                if let Ok(mut flag) = self.ddos_notified.lock() {
                    *flag = false;
                }
            }
        }

        next
    }

    /// Force the FloodMode back to Quiet — invoked by the "Eskalation früher
    /// beenden" UI action.
    pub fn end_escalation(&self) {
        if let Ok(mut current) = self.current.lock() {
            *current = FloodMode::Quiet;
        }
        if let Ok(mut flag) = self.ddos_notified.lock() {
            *flag = false;
        }
        if let Err(e) = persist_quiet(&self.db) {
            eprintln!("[DosDefence Phase 3] persist quiet on end_escalation failed: {e}");
        }
    }

    fn emit_ddos_notification(&self, source_count: usize, expires_at: &str) {
        let mut params = HashMap::new();
        params.insert(
            "source_count".to_string(),
            JsonValue::from(source_count as u64),
        );
        params.insert(
            "expires_at".to_string(),
            JsonValue::String(expires_at.to_string()),
        );
        let value = JsonValue::Object(params.into_iter().collect());

        if let Ok(mut flag) = self.ddos_notified.lock() {
            if !*flag {
                *flag = true;
                if let Err(e) =
                    self.sink
                        .emit(CriticalFailureCode::FloodDdos, FLOOD_DDOS_LOCATION, value)
                {
                    eprintln!("[DosDefence Phase 3] sink.emit FLOOD_DDOS failed: {e}");
                }
            }
        }
    }
}

fn load_state(db: &DbConnection) -> Result<FloodMode, String> {
    let sql =
        format!("SELECT flood_mode, flood_mode_source, ddos_expires_at FROM {TABLE} WHERE id = 1");
    let rows = select(sql, vec![], db).map_err(|e| e.to_string())?;
    let Some(first) = rows.into_iter().next() else {
        // Seed the singleton row so subsequent UPSERTs hit a CONFLICT path.
        seed_singleton(db)?;
        return Ok(FloodMode::Quiet);
    };
    let mut it = first.into_iter();
    let mode = it.next().and_then(|v| v.as_str().map(|s| s.to_string()));
    let source = it.next().and_then(|v| v.as_str().map(|s| s.to_string()));
    let expires = it.next().and_then(|v| v.as_str().map(|s| s.to_string()));

    Ok(match mode.as_deref() {
        Some("single_source") => match source {
            Some(did) => FloodMode::SingleSource { did },
            None => FloodMode::Quiet,
        },
        Some("ddos") => match expires
            .as_deref()
            .and_then(|s| OffsetDateTime::parse(s, &Rfc3339).ok())
        {
            Some(exp_dt) => {
                let now_dt = OffsetDateTime::now_utc();
                let delta_secs = (exp_dt - now_dt).whole_seconds();
                let expires_at = if delta_secs <= 0 {
                    Instant::now()
                } else {
                    Instant::now() + std::time::Duration::from_secs(delta_secs as u64)
                };
                FloodMode::Ddos {
                    source_count: 0,
                    expires_at,
                }
            }
            None => FloodMode::Quiet,
        },
        _ => FloodMode::Quiet,
    })
}

fn seed_singleton(db: &DbConnection) -> Result<(), String> {
    let now = OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|e| e.to_string())?;
    let sql = format!(
        "INSERT INTO {TABLE} (id, flood_mode, flood_mode_source, ddos_expires_at, updated_at) \
         VALUES (1, 'quiet', NULL, NULL, ?1) \
         ON CONFLICT(id) DO NOTHING"
    );
    execute(sql, vec![JsonValue::String(now)], db).map_err(|e| e.to_string())?;
    Ok(())
}

fn persist_ddos(db: &DbConnection, _source_count: usize, expires_rfc: &str) -> Result<(), String> {
    let now = OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|e| e.to_string())?;
    let sql = format!(
        "INSERT INTO {TABLE} (id, flood_mode, flood_mode_source, ddos_expires_at, updated_at) \
         VALUES (1, 'ddos', NULL, ?1, ?2) \
         ON CONFLICT(id) DO UPDATE SET \
            flood_mode = excluded.flood_mode, \
            flood_mode_source = excluded.flood_mode_source, \
            ddos_expires_at = excluded.ddos_expires_at, \
            updated_at = excluded.updated_at"
    );
    execute(
        sql,
        vec![
            JsonValue::String(expires_rfc.to_string()),
            JsonValue::String(now),
        ],
        db,
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn persist_quiet(db: &DbConnection) -> Result<(), String> {
    let now = OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|e| e.to_string())?;
    let sql = format!(
        "INSERT INTO {TABLE} (id, flood_mode, flood_mode_source, ddos_expires_at, updated_at) \
         VALUES (1, 'quiet', NULL, NULL, ?1) \
         ON CONFLICT(id) DO UPDATE SET \
            flood_mode = excluded.flood_mode, \
            flood_mode_source = excluded.flood_mode_source, \
            ddos_expires_at = excluded.ddos_expires_at, \
            updated_at = excluded.updated_at"
    );
    execute(sql, vec![JsonValue::String(now)], db).map_err(|e| e.to_string())?;
    Ok(())
}

/// Best-effort conversion from a monotonic-clock deadline back to a
/// human-facing RFC3339 timestamp. We compute `now → deadline` as a duration
/// and add it to wall-clock `now`. Drift over the auto-expiry window
/// (≤30 min) is well within the user-visible precision (minutes), so this
/// approximation is good enough for the banner.
fn instant_to_rfc3339_estimate(now: Instant, deadline: Instant) -> String {
    let delta = deadline.saturating_duration_since(now);
    let wall = OffsetDateTime::now_utc() + delta;
    wall.format(&Rfc3339).unwrap_or_else(|_| "unknown".into())
}
