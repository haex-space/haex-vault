//! The vault a new installation creates to join a link (spec 024,
//! FR-024, FR-025, research R11).
//!
//! It is created like any vault, with the user's own passphrase, but with no
//! identity of its own: the vault identity, the data and the device list
//! come from the main device. It is not published as the active instance
//! while the link runs. Until the link finished, [`LinkVault::discard`]
//! leaves nothing behind; only [`LinkVault::keep`] turns it into a vault the
//! user can open.

use std::path::PathBuf;
use std::sync::Arc;

use haex_crdt::Database;
use tauri::{AppHandle, Runtime};

use crate::error::{HolziError, Result};
use crate::identity::{installation_id_path, HolziBootstrap};
use crate::instances::cleanup::take_back;
use crate::instances::passphrase::Passphrase;
use crate::instances::paths::{
    get_app_local_data, get_instance_path, get_pending_marker_path, validate_instance_name,
};
use crate::instances::reserve::reserve;
use crate::instances::vault_config::vault_config;
use crate::sync::keys::{self, DeviceKeys};

/// A vault being filled by a link.
pub struct LinkVault {
    pub name: String,
    pub db: Arc<Database>,
    /// This installation's keys in the new vault.
    pub keys: DeviceKeys,
    db_path: PathBuf,
    pending_marker: PathBuf,
}

impl LinkVault {
    /// Creates the empty vault `name` under `passphrase`, with this
    /// installation's device keys and `device_name` as its alias. The
    /// `.pending` marker stays until [`LinkVault::keep`], so a crash
    /// mid-link is cleaned up on the next start like a half-made vault.
    pub async fn create<R: Runtime>(
        app: &AppHandle<R>,
        name: &str,
        device_name: &str,
        passphrase: Passphrase,
    ) -> Result<Self> {
        validate_instance_name(name)?;
        passphrase.validate_new()?;
        let db_path = get_instance_path(app, name)?;
        let pending_marker = get_pending_marker_path(&db_path);
        let installation_id_file = installation_id_path(&get_app_local_data(app)?);
        reserve(&db_path).map_err(|e| match e.kind() {
            std::io::ErrorKind::AlreadyExists => HolziError::NameConflict {
                name: name.to_string(),
            },
            _ => HolziError::from(e),
        })?;

        let (open_path, id_file, alias) = (
            db_path.clone(),
            installation_id_file.clone(),
            device_name.to_string(),
        );
        let opened = tauri::async_runtime::spawn_blocking(move || {
            open_empty(passphrase.as_str(), &open_path, &id_file, &alias)
        })
        .await;
        let cleanup = || {
            take_back(&db_path);
        };
        match opened {
            Ok(Ok((db, keys))) => Ok(Self {
                name: name.to_string(),
                db,
                keys,
                db_path,
                pending_marker,
            }),
            Ok(Err(e)) => {
                cleanup();
                Err(e)
            }
            Err(e) => {
                cleanup();
                Err(HolziError::CrdtInit {
                    reason: format!("database open task failed: {e}"),
                })
            }
        }
    }

    /// The link finished: closes the vault and lets it stay. The user opens
    /// it like any other, with the passphrase they chose.
    pub fn keep(self) -> Result<()> {
        let Self {
            db, pending_marker, ..
        } = self;
        drop(db);
        std::fs::remove_file(pending_marker)?;
        Ok(())
    }

    /// The link did not finish: closes the vault and removes it.
    pub fn discard(self) {
        let Self { db, db_path, .. } = self;
        drop(db);
        take_back(&db_path);
    }
}

/// Opens the new vault with no genesis and creates this installation's keys.
fn open_empty(
    passphrase: &str,
    db_path: &std::path::Path,
    installation_id_file: &std::path::Path,
    alias: &str,
) -> Result<(Arc<Database>, DeviceKeys)> {
    let mut config = vault_config(passphrase, db_path, installation_id_file, true);
    config.bootstrap =
        Arc::new(HolziBootstrap::new(installation_id_file.to_path_buf()).with_alias(alias));
    let db = Database::open(config)?;
    crate::storage::maintenance::run_after_open(&db);
    let installation = crate::identity::read_or_mint_installation_uuid(installation_id_file)?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0);
    let keys = db.write(|tx| keys::ensure_device_keys(tx, installation, now))?;
    Ok((Arc::new(db), keys))
}
