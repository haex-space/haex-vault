//! `HaexBootstrap` — impl of haex-crdt's `DatabaseBootstrap` hook.
//!
//! Contract: `tauri-commands.md` §"Provider implementations" → `DatabaseBootstrap`.
//! The hook runs inside a crate-owned transaction the crate commits on `Ok` or
//! rolls back on `Err`. Ordering matters — HLC is not yet initialised inside
//! this transaction, so the inserted rows carry NULL row-level HLC and stay
//! sync-invisible until a subsequent UPDATE fires the trigger (documented
//! in the contract; verified in Etappe 0).
//!
//! The vault identity is no longer minted here: without an HLC its row
//! could not sync. `sync::genesis::ensure_sync_state` publishes it after the
//! open (spec 024).

use std::path::PathBuf;

use haex_crdt::db::error::DatabaseError;
use haex_crdt::rusqlite::{params, OptionalExtension, Transaction};
use haex_crdt::{DatabaseBootstrap, Error as CrdtError, Result as CrdtResult};
use uuid::Uuid;

use super::installation::read_or_mint_installation_uuid;
use super::VAULT_SCOPE_UUID;

/// Bootstrap wired to a specific `<AppLocalData>/installation-id` path. In
/// production the path is resolved from `AppHandle::path().app_local_data_dir()`;
/// tests point it at a temp dir.
pub struct HaexBootstrap {
    installation_id_path: PathBuf,
    /// Human-readable alias inserted into the freshly minted `known_devices`
    /// row on first open. Defaults to `"holzi"`; tests may override.
    alias: String,
}

impl HaexBootstrap {
    /// Creates a bootstrap provider backed by the given installation-id file.
    pub fn new(installation_id_path: PathBuf) -> Self {
        Self {
            installation_id_path,
            alias: "holzi".to_string(),
        }
    }

    /// Overrides the alias assigned when this installation is first registered.
    pub fn with_alias(mut self, alias: impl Into<String>) -> Self {
        self.alias = alias.into();
        self
    }
}

impl DatabaseBootstrap for HaexBootstrap {
    /// Ensures the installation, device, and vault identities exist before HLC setup.
    fn bootstrap(&self, tx: &Transaction<'_>) -> CrdtResult<Uuid> {
        // 0. Vault-scope sentinel row (see ADR-0001 and spec 002
        //    §"Vault Scope Sentinel"). Every device inserts it idempotently
        //    on every open so vault-wide `preferences` rows have a valid
        //    FK target under `known_devices(vault_device_uuid)`. The
        //    values are byte-for-byte identical across devices, so parallel
        //    sync inserts collapse conflict-free via LWW.
        tx.execute(
            "INSERT OR IGNORE INTO known_devices \
             (installation_uuid, vault_device_uuid, alias, first_seen) \
             VALUES (?1, ?2, NULL, 0)",
            params![VAULT_SCOPE_UUID.to_string(), VAULT_SCOPE_UUID.to_string()],
        )
        .map_err(step("known_devices sentinel insert"))?;

        // 1. Installation UUID from `<AppLocalData>/installation-id`, minted
        //    and fsynced on first open of this installation.
        let installation_uuid = read_or_mint_installation_uuid(&self.installation_id_path)
            .map_err(|source| DatabaseError::IoError {
                path: self.installation_id_path.display().to_string(),
                source,
            })?;

        // 2. Look up this installation's row in `known_devices`.
        let existing: Option<String> = tx
            .query_row(
                "SELECT vault_device_uuid FROM known_devices \
                 WHERE installation_uuid = ?1",
                params![installation_uuid.to_string()],
                |r| r.get(0),
            )
            .optional()
            .map_err(step("known_devices lookup"))?;

        // 3. Reuse the existing UUID or mint + insert a fresh row. MUST NOT
        //    write the three `_no_sync`-suffixed metadata columns — the crate
        //    fills those in only after HLC init.
        let vault_device_uuid = match existing {
            Some(s) => Uuid::parse_str(&s).map_err(CrdtError::consumer)?,
            None => {
                let fresh = Uuid::new_v4();
                let first_seen = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0);
                tx.execute(
                    "INSERT INTO known_devices \
                     (installation_uuid, vault_device_uuid, alias, first_seen) \
                     VALUES (?1, ?2, ?3, ?4)",
                    params![
                        installation_uuid.to_string(),
                        fresh.to_string(),
                        &self.alias,
                        first_seen,
                    ],
                )
                .map_err(step("known_devices insert"))?;
                fresh
            }
        };

        // 4. Return the vault-device UUID; the crate uses it as HLC node id
        //    for this open.
        Ok(vault_device_uuid)
    }
}

/// Wraps a SQLite failure of one bootstrap step, keeping the typed source.
fn step(step: &'static str) -> impl FnOnce(haex_crdt::rusqlite::Error) -> DatabaseError {
    move |source| DatabaseError::SqliteStep {
        step: step.to_string(),
        source,
    }
}
