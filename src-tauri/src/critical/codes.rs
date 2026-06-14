//! Critical-failure discriminator + severity. See module-level docs.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Semantic discriminator for a critical failure. One variant per distinct
/// failure class — never reused across call sites that have semantically
/// different recovery paths. Frontend uses this as the i18n key for the
/// banner's title/description/risk/action quad.
///
/// **Adding a new code:**
/// 1. Add the variant here.
/// 2. Wire its severity below in [`Self::severity`].
/// 3. Add the four i18n strings (title/description/risk/action) under
///    `critical_failures.<VariantName>` in `src/locales/{de,en}.yml`.
/// 4. The ts-rs export regenerates the TypeScript union automatically;
///    Vue-i18n's `missing-keys` check catches forgotten translations at
///    compile time.
///
/// **Why string discriminator (the default `Serialize` shape) instead of a
/// numeric tag:** the DB row stores the variant name as text, so an
/// operator can grep `haex_critical_notifications_no_sync` directly. A
/// numeric tag would require a separate code → name map.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum CriticalFailureCode {
    /// The HLC service's mutex was poisoned. Subsequent CRDT writes would
    /// produce inconsistent timestamps; data corruption possible on next
    /// sync. Restart pflicht.
    HlcMutexPoisoned,
    /// The main DB connection's mutex was poisoned. Every subsequent
    /// SQL operation will fail. Restart pflicht.
    DbMutexPoisoned,
    /// A SQL operation discovered a schema state that the running
    /// migration version cannot read or write safely (e.g. missing
    /// column, foreign-key target removed). Restart pflicht — the vault
    /// might need a backup-restore.
    DbSchemaDrift,
    /// `log_to_db` failed to persist an audit row (e.g. DB locked, disk
    /// full, mutex contention with HLC). Observability hole, but the
    /// operation that triggered the audit attempt still ran. No restart
    /// needed — Warning severity.
    AuditLogWriteFailed,
    /// The CRDT transformer rejected a SELECT/UPDATE/DELETE statement.
    /// Some queries will return wrong results until the underlying schema
    /// issue is fixed. No restart needed — Warning severity.
    CrdtTransformFailed,
}

/// Severity is a property of the code (see Q2 in the plan), so the
/// frontend can branch on `Critical` vs `Warning` without consulting an
/// external table. Critical → red banner with "Restart vault" CTA;
/// Warning → orange banner with "Verstanden" only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum Severity {
    /// Restart-pflicht — continuing to use the vault risks data
    /// corruption.
    Critical,
    /// Hinweis only — operation degraded but no corruption risk.
    Warning,
}

impl CriticalFailureCode {
    /// Map this code to its banner severity. Each arm must match the
    /// severity documented in the variant doc-comment above; the tests
    /// `severity_critical_for_data_corruption_codes` and
    /// `severity_warning_for_observability_codes` (in
    /// `crate::critical::tests`) pin every variant to its severity.
    pub fn severity(&self) -> Severity {
        match self {
            Self::HlcMutexPoisoned | Self::DbMutexPoisoned | Self::DbSchemaDrift => Severity::Critical,
            Self::AuditLogWriteFailed | Self::CrdtTransformFailed => Severity::Warning,
        }
    }

    /// Stable string used as the DB row's `code` column. Matches the
    /// variant identifier (e.g. `HlcMutexPoisoned`) so a row found in
    /// `haex_critical_notifications_no_sync` can be grep'd back to this
    /// enum without parsing.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::HlcMutexPoisoned => "HlcMutexPoisoned",
            Self::DbMutexPoisoned => "DbMutexPoisoned",
            Self::DbSchemaDrift => "DbSchemaDrift",
            Self::AuditLogWriteFailed => "AuditLogWriteFailed",
            Self::CrdtTransformFailed => "CrdtTransformFailed",
        }
    }
}
