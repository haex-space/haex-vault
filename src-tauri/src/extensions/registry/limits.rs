//! The limits of an extension as the settings show and change them (spec 017, FR-031, T092,
//! contracts/tauri-commands.md `extension_limits_get`/`_set`). Only holzi's own window changes
//! them; the bridge only reads them (`sql::exec::limits_of`).

use haex_crdt::rusqlite::params;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::error::{HolziError, Result};
use crate::extensions::ids::limits_id;
use crate::extensions::sql::exec::{limits_of, Limits};
use crate::storage::query::Query;
use crate::vault_gate::VaultDb;

/// The limits of one extension (data-model.md `extension_limits`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct ExtensionLimits {
    /// Rows one query may return.
    pub max_rows: u32,
    /// Requests running at the same time.
    pub max_concurrent: u32,
    /// Bytes of one SQL statement with its parameters.
    pub max_sql_bytes: u32,
    /// Run time of one request in milliseconds.
    pub timeout_ms: u32,
    /// Bytes of one answer.
    pub max_response_bytes: u32,
    /// Run time of one model job in milliseconds (spec 055).
    pub model_job_timeout_ms: u32,
}

/// The smallest values the settings accept (contracts/tauri-commands.md §Grenzwerte).
pub const MIN: ExtensionLimits = ExtensionLimits {
    max_rows: 1,
    max_concurrent: 1,
    max_sql_bytes: 1_000,
    timeout_ms: 100,
    max_response_bytes: 64 * 1024,
    model_job_timeout_ms: 10_000,
};

/// The largest values the settings accept.
pub const MAX: ExtensionLimits = ExtensionLimits {
    max_rows: 1_000_000,
    max_concurrent: 100,
    max_sql_bytes: 16 * 1024 * 1024,
    timeout_ms: 60_000,
    max_response_bytes: 256 * 1024 * 1024,
    model_job_timeout_ms: 1_800_000,
};

/// The limits with the bounds the settings show next to them; the bounds are only checked here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct ExtensionLimitsView {
    #[ts(inline)]
    pub values: ExtensionLimits,
    #[ts(inline)]
    pub min: ExtensionLimits,
    #[ts(inline)]
    pub max: ExtensionLimits,
}

fn clamp_u32(value: u64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

impl From<Limits> for ExtensionLimits {
    fn from(limits: Limits) -> Self {
        Self {
            max_rows: clamp_u32(limits.max_rows),
            max_concurrent: clamp_u32(limits.max_concurrent),
            max_sql_bytes: clamp_u32(limits.max_sql_bytes),
            timeout_ms: clamp_u32(limits.timeout_ms),
            max_response_bytes: clamp_u32(limits.max_response_bytes),
            model_job_timeout_ms: clamp_u32(limits.model_job_timeout_ms),
        }
    }
}

impl ExtensionLimits {
    /// Every value with its field name.
    fn fields(&self) -> [(&'static str, u32); 6] {
        [
            ("maxRows", self.max_rows),
            ("maxConcurrent", self.max_concurrent),
            ("maxSqlBytes", self.max_sql_bytes),
            ("timeoutMs", self.timeout_ms),
            ("maxResponseBytes", self.max_response_bytes),
            ("modelJobTimeoutMs", self.model_job_timeout_ms),
        ]
    }

    /// The first value outside [`MIN`]..=[`MAX`], by its field name.
    fn out_of_bounds(&self) -> Option<&'static str> {
        self.fields()
            .into_iter()
            .zip(MIN.fields().into_iter().zip(MAX.fields()))
            .find(|((_, value), ((_, min), (_, max)))| !(min..=max).contains(&value))
            .map(|((name, _), _)| name)
    }
}

/// The limits of an installed extension, without a row the defaults, and their bounds.
pub fn get(db: &VaultDb, extension_id: Uuid) -> Result<ExtensionLimitsView> {
    db.read_blocking(move |q| {
        let installed = q
            .query_row(
                "SELECT COUNT(*) FROM extensions WHERE id = ?1 AND state = 'installed'",
                &[&extension_id.to_string()],
                |r| r.get::<_, i64>(0),
            )?
            .unwrap_or(0)
            > 0;
        if !installed {
            return Err(HolziError::ExtensionNotFound.into());
        }
        Ok(ExtensionLimitsView {
            values: limits_of(q, extension_id)?.into(),
            min: MIN,
            max: MAX,
        })
    })
}

/// Stores the limits of an installed extension for every device; a value outside its bounds is
/// refused and nothing is written.
pub fn set(db: &VaultDb, extension_id: Uuid, limits: ExtensionLimits) -> Result<()> {
    if let Some(field) = limits.out_of_bounds() {
        return Err(HolziError::InvalidInput {
            reason: format!("limit {field} out of bounds"),
        });
    }
    db.write_blocking(move |tx| {
        let ext = extension_id.to_string();
        let installed = tx
            .query_row(
                "SELECT COUNT(*) FROM extensions WHERE id = ?1 AND state = 'installed'",
                params![ext],
                |r| r.get::<_, i64>(0),
            )?
            .unwrap_or(0)
            > 0;
        if !installed {
            return Err(HolziError::ExtensionNotFound.into());
        }
        let id = limits_id(extension_id).to_string();
        let values = params![
            id,
            ext,
            limits.max_rows,
            limits.max_concurrent,
            limits.max_sql_bytes,
            limits.timeout_ms,
            limits.max_response_bytes,
            limits.model_job_timeout_ms
        ];
        let exists = tx
            .query_row(
                "SELECT COUNT(*) FROM extension_limits WHERE id = ?1",
                params![id],
                |r| r.get::<_, i64>(0),
            )?
            .unwrap_or(0)
            > 0;
        if exists {
            tx.execute(
                "UPDATE extension_limits SET extension_id = ?2, max_rows = ?3, \
                 max_concurrent = ?4, max_sql_bytes = ?5, timeout_ms = ?6, \
                 max_response_bytes = ?7, model_job_timeout_ms = ?8 WHERE id = ?1",
                values,
            )?;
        } else {
            tx.execute(
                "INSERT INTO extension_limits (id, extension_id, max_rows, max_concurrent, \
                 max_sql_bytes, timeout_ms, max_response_bytes, model_job_timeout_ms) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                values,
            )?;
        }
        Ok(())
    })
}

#[cfg(test)]
#[path = "limits_tests.rs"]
mod tests;
