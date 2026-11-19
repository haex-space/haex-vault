//! Starting an extension on this device before its first frame (FR-003, data-model.md
//! §Zustände): it must be installed and enabled, its effective bundle complete and verifying, and
//! its migrations applied; the outcome becomes its state on this device.

use std::sync::PoisonError;

use uuid::Uuid;

use super::effective::{effective_bundle, retired_above, EffectiveBundle};
use super::status::{self, DeviceStatus};
use crate::error::{HaexError, Result};
use crate::extensions::bundle::store::{verify_stored_bundle, BundleIds, StoredBundleState};
use crate::extensions::bundle::{Manifest, VerifiedBundle};
use crate::extensions::ids::TablePrefix;
use crate::extensions::protocol::{csp, prefix};
use crate::extensions::sql::migrate::{
    apply_pending_locked, applying, check_applied_kept, MigrationError,
};
use crate::storage::query::Query;
use crate::vault_gate::VaultDb;

/// What a frame of a started extension needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Started {
    pub bundle_id: Uuid,
    /// Path of the entry page inside the bundle.
    pub entry: String,
    /// The Content-Security-Policy header of its files.
    pub csp: String,
}

pub(super) enum Registration {
    Missing,
    Disabled,
    Enabled,
}

pub(super) fn registration(q: &mut impl Query, extension_id: Uuid) -> Result<Registration> {
    let row = q.query_row(
        "SELECT enabled, state FROM extensions WHERE id = ?1",
        &[&extension_id.to_string()],
        |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)),
    )?;
    Ok(match row {
        Some((_, state)) if state != "installed" => Registration::Missing,
        None => Registration::Missing,
        Some((0, _)) => Registration::Disabled,
        Some(_) => Registration::Enabled,
    })
}

fn not_ready(status: DeviceStatus) -> HaexError {
    HaexError::ExtensionNotReady {
        status: status.as_str().to_owned(),
    }
}

/// The error kind stored in the device status; no data of the extension.
fn migration_error_kind(error: &MigrationError) -> &'static str {
    match error {
        MigrationError::Changed { .. } => "migration_changed",
        MigrationError::Refused { .. } => "migration_refused",
        MigrationError::Failed { .. } => "migration_failed",
        MigrationError::Missing { .. } => "migration_missing",
        MigrationError::Unavailable => "database_unavailable",
    }
}

/// The effective bundle of an extension after verification, before anything of it runs.
pub(crate) enum Effective {
    /// No bundle, or not every BLOB has arrived.
    Transferring(Option<Uuid>),
    /// The bundle does not verify, or is not the one its row names; the error kind.
    SignatureFailed(Uuid, String),
    Ready(Box<Prepared>),
}

/// A verified effective bundle that belongs to its rows.
pub(crate) struct Prepared {
    pub bundle_id: Uuid,
    pub bundle: VerifiedBundle,
    pub manifest: Manifest,
    pub own: TablePrefix,
    /// The migrations (name, SQL) this device runs, in order: those of the bundle, and after a
    /// confirmed downgrade those of the retired higher bundles it does not know. Only verified SQL
    /// runs (FR-003).
    pub migrations: Vec<(String, String)>,
    /// It stands after a confirmed downgrade (research R11).
    pub downgrade: bool,
}

/// Verifies the effective bundle of `extension_id`. The rows only choose the bundle: it must
/// derive the extension id and its own id, and carry the version the choice was made by, so a
/// changed row cannot put an older or a foreign signed bundle in place. Blocking.
pub(crate) fn effective(db: &VaultDb, extension_id: Uuid) -> Result<Effective> {
    db.read_blocking(move |q| Ok(effective_in(q, extension_id)))?
}

/// A stored bundle that verifies and belongs to the row that names it.
enum Checked {
    Transferring,
    SignatureFailed(String),
    Ready(Box<VerifiedBundle>, Box<Manifest>),
}

fn check(q: &mut impl Query, extension_id: Uuid, row: &EffectiveBundle) -> Result<Checked> {
    let bundle = match verify_stored_bundle(q, row.bundle_id)? {
        StoredBundleState::Transferring => return Ok(Checked::Transferring),
        StoredBundleState::SignatureFailed(rejection) => {
            return Ok(Checked::SignatureFailed(rejection.kind))
        }
        StoredBundleState::Ready(bundle) => bundle,
    };
    let manifest = Manifest::from_verified(&bundle)?;
    let ids = BundleIds::of(&bundle, &manifest);
    if ids.extension_id != extension_id
        || ids.bundle_id != row.bundle_id
        || manifest.version != row.version
    {
        return Ok(Checked::SignatureFailed("bundle_mismatch".to_owned()));
    }
    Ok(Checked::Ready(bundle, Box::new(manifest)))
}

fn effective_in(q: &mut impl Query, extension_id: Uuid) -> Result<Effective> {
    let Some(chosen) = effective_bundle(q, extension_id)? else {
        return Ok(Effective::Transferring(None));
    };
    let (bundle, manifest) = match check(q, extension_id, &chosen)? {
        Checked::Transferring => return Ok(Effective::Transferring(Some(chosen.bundle_id))),
        Checked::SignatureFailed(kind) => {
            return Ok(Effective::SignatureFailed(chosen.bundle_id, kind))
        }
        Checked::Ready(bundle, manifest) => (*bundle, *manifest),
    };
    let mut migrations: Vec<(String, String)> = bundle
        .migrations
        .iter()
        .map(|m| (m.name.clone(), m.sql.clone()))
        .collect();
    // A device that applied a higher bundle keeps its migrations after a downgrade (US7-3) and
    // syncs rows with their columns; every other device runs them too, so those rows do not wait
    // parked for a column that never comes. A retired bundle that does not verify (yet) adds none.
    let retired = retired_above(q, extension_id, &chosen)?;
    for higher in &retired {
        let Checked::Ready(higher_bundle, _) = check(q, extension_id, higher)? else {
            continue;
        };
        for m in &higher_bundle.migrations {
            if !migrations.iter().any(|(name, _)| *name == m.name) {
                migrations.push((m.name.clone(), m.sql.clone()));
            }
        }
    }
    Ok(Effective::Ready(Box::new(Prepared {
        bundle_id: chosen.bundle_id,
        own: TablePrefix {
            public_key: manifest.public_key.clone(),
            name: manifest.name.clone(),
        },
        migrations,
        downgrade: !retired.is_empty(),
        bundle,
        manifest,
    })))
}

/// Checks that `prepared` keeps what this device applied and applies its pending migrations, both
/// under the migration lock. Returns the names applied now. Blocking.
pub(crate) fn migrate(
    db: &VaultDb,
    extension_id: Uuid,
    prepared: &Prepared,
    now_ms: i64,
) -> std::result::Result<Vec<String>, MigrationError> {
    let locked = applying().lock().unwrap_or_else(PoisonError::into_inner);
    let (offered, downgrade) = (prepared.migrations.clone(), prepared.downgrade);
    db.read_blocking(move |q| Ok(check_applied_kept(q, extension_id, &offered, downgrade)))
        .unwrap_or(Err(MigrationError::Unavailable))?;
    apply_pending_locked(
        &locked,
        db,
        extension_id,
        &prepared.own,
        &prepared.migrations,
        now_ms,
    )
}

/// Whether a development version of `prefix` is registered in this vault file.
pub(crate) fn dev_prefix_here(db: &VaultDb, prefix: &TablePrefix) -> Result<bool> {
    let prefix = prefix.to_string();
    db.read_blocking(move |q| Ok(crate::extensions::dev::prefixes(q)?.contains(&prefix)))
}

/// Checks and prepares `extension_id` on `device`. Blocking: run it on a blocking thread.
pub fn start(db: &VaultDb, extension_id: Uuid, device: Uuid, now_ms: i64) -> Result<Started> {
    match db.read_blocking(move |q| Ok(registration(q, extension_id)))?? {
        Registration::Missing => return Err(HaexError::ExtensionNotFound),
        Registration::Disabled => return Err(HaexError::ExtensionDisabled),
        Registration::Enabled => {}
    }

    let (status, bundle_id, error, started) = match effective(db, extension_id)? {
        Effective::Transferring(bundle_id) => (DeviceStatus::Transferring, bundle_id, None, None),
        Effective::SignatureFailed(bundle_id, kind) => (
            DeviceStatus::SignatureFailed,
            Some(bundle_id),
            Some(kind),
            None,
        ),
        // A development version of the same prefix runs on this device (US12, research R16).
        Effective::Ready(prepared) if dev_prefix_here(db, &prepared.own)? => (
            DeviceStatus::MigrationFailed,
            Some(prepared.bundle_id),
            Some("dev_prefix_conflict".to_owned()),
            None,
        ),
        Effective::Ready(prepared) => match migrate(db, extension_id, &prepared, now_ms) {
            Ok(_) => {
                let started = Started {
                    bundle_id: prepared.bundle_id,
                    entry: prepared.manifest.entry.clone(),
                    csp: csp::for_bundle(&prepared.bundle, &prefix(extension_id)),
                };
                (
                    DeviceStatus::Ready,
                    Some(prepared.bundle_id),
                    None,
                    Some(started),
                )
            }
            Err(error) => {
                log::warn!("extension {extension_id}: migrations failed: {error:?}");
                (
                    DeviceStatus::MigrationFailed,
                    Some(prepared.bundle_id),
                    Some(migration_error_kind(&error).to_owned()),
                    None,
                )
            }
        },
    };
    db.write_blocking(move |tx| {
        status::set(
            tx,
            extension_id,
            device,
            status,
            bundle_id,
            error.as_deref(),
            now_ms,
        )
        .map_err(Into::into)
    })?;
    started.ok_or_else(|| not_ready(status))
}

#[cfg(test)]
#[path = "start_tests.rs"]
mod tests;
