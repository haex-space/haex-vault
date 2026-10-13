//! Developer mode (spec 017, US12, FR-063–FR-065, research R16): an extension that is not signed
//! yet runs from a development server on this device.
//!
//! The switch is a setting of this device. The developer picks the project folder; holzi reads
//! `haextension.config.json` (the dev server's host and port, as the SDK writes them),
//! `haextension/manifest.json` and, for the name as in the SDK, `package.json` from it. The server
//! must be `localhost` or `127.0.0.1`.
//! Registration and permissions live in `dev_extensions_no_sync` and
//! `dev_extension_permissions_no_sync`, the key-value store in `dev_extension_kv_no_sync`; the
//! tables the extension creates come from the migrations it registers itself and are created in
//! haex-crdt's local mode, without CRDT columns, so none of it syncs.
//!
//! The manifest's key is unsigned, so a development version never reaches data of an installed
//! extension: loading is refused while an `extensions` row with the same key and name exists in
//! any state (also removed with its data kept) or while tables with its prefix exist that are not
//! its own. The other direction is guarded where installs happen (`registry::install`,
//! `registry::start`) and where synced rows arrive (`sync::inbound::park`).
//!
//! Unloading deletes the registration, its permissions and store, and drops its tables (operator
//! decision 2026-10-04): development data is throwaway, and the prefix is free again.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, PoisonError};

use haex_crdt::rusqlite::params;
use haex_crdt::{AuthContext, Authorization, GuardedWriteOptions, SqlGuard};
use serde::Deserialize;
use uuid::Uuid;

use crate::error::{HolziError, Result};
use crate::extensions::bundle::Manifest;
use crate::extensions::ids::{dev_extension_id, ExtensionName, PublicKey, TablePrefix};
use crate::extensions::registry::install::{
    apply_declarations, publisher_fingerprint, DeclaredPermissionView, InstallPreview,
    PermissionChoice,
};
use crate::extensions::registry::purge::{prefixed, quoted};
use crate::extensions::sql::authorizer::MIGRATION_JOURNAL;
use crate::extensions::sql::migrate::applying;
use crate::storage::preferences::{self, PrefScope};
use crate::storage::query::Query;
use crate::vault_gate::VaultDb;

/// The device setting that switches developer mode on.
pub const MODE_KEY: &str = "extensions.devMode";

/// The hosts a development server may have; the frame shim acts on the same (`protocol::shim`).
/// Not `[::1]`: a Content-Security-Policy source cannot name an IPv6 address, so holzi's window
/// could not frame it.
pub(crate) const LOOPBACK: [&str; 2] = ["localhost", "127.0.0.1"];

fn refused(reason: &str) -> HolziError {
    HolziError::ExtensionInstall {
        reason: reason.to_owned(),
    }
}

/// Whether developer mode is on for `device`.
pub fn mode(q: &mut impl Query, device: Uuid) -> Result<bool> {
    let value = preferences::get(q, PrefScope::Device(device), MODE_KEY)?;
    Ok(preferences::parse_bool(value.as_deref()).unwrap_or(false))
}

/// Switches developer mode for `device`.
pub fn set_mode(
    tx: &mut haex_crdt::CrdtTransaction<'_>,
    device: Uuid,
    enabled: bool,
) -> Result<()> {
    preferences::insert_or_update(
        tx,
        PrefScope::Device(device),
        MODE_KEY,
        if enabled { "true" } else { "false" },
    )?;
    Ok(())
}

/// `haextension.config.json` as far as holzi reads it.
#[derive(Debug, Default, Deserialize)]
struct ProjectConfig {
    #[serde(default)]
    dev: DevConfig,
}

#[derive(Debug, Deserialize)]
struct DevConfig {
    #[serde(default = "default_host")]
    host: String,
    #[serde(default = "default_port")]
    port: u16,
    #[serde(default = "default_dir")]
    haextension_dir: String,
}

impl Default for DevConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            haextension_dir: default_dir(),
        }
    }
}

fn default_host() -> String {
    "localhost".to_owned()
}

fn default_port() -> u16 {
    5173
}

fn default_dir() -> String {
    "haextension".to_owned()
}

/// A project folder as holzi reads it for developer mode.
#[derive(Debug, Clone)]
pub struct DevProject {
    pub path: PathBuf,
    pub manifest: Manifest,
    /// Origin of the development server, `http://host:port`.
    pub url: String,
}

impl DevProject {
    pub fn prefix(&self) -> TablePrefix {
        TablePrefix {
            public_key: self.manifest.public_key.clone(),
            name: self.manifest.name.clone(),
        }
    }
}

/// The origin of a development server on `host` and `port`; only a loopback host.
pub fn server_url(host: &str, port: u16) -> Result<String> {
    if !LOOPBACK.contains(&host) {
        return Err(refused("dev_server_not_loopback"));
    }
    Ok(format!("http://{host}:{port}"))
}

/// Reads the project folder at `path`. Blocking.
///
/// `name` comes from `package.json` as in the SDK (`Manifest::from_dev_file`).
pub fn read_project(path: &Path) -> Result<DevProject> {
    let config: ProjectConfig = match std::fs::read_to_string(path.join("haextension.config.json"))
    {
        Ok(text) => serde_json::from_str(&text).map_err(|_| refused("dev_config_invalid"))?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => ProjectConfig::default(),
        Err(_) => return Err(refused("dev_config_unreadable")),
    };
    // The folder of the manifest must stay inside the project.
    let dir = Path::new(&config.dev.haextension_dir);
    if dir.is_absolute()
        || dir
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return Err(refused("dev_config_invalid"));
    }
    let manifest_text = std::fs::read_to_string(path.join(dir).join("manifest.json"))
        .map_err(|_| refused("dev_manifest_missing"))?;
    let package_text = std::fs::read_to_string(path.join("package.json")).ok();
    let manifest = Manifest::from_dev_file(&manifest_text, package_text.as_deref())
        .map_err(|rejection| refused(&rejection.kind))?;
    // As for a bundle: the key must be a usable Ed25519 key (FR-004), even unsigned.
    if haex_bundle::verify::strict_public_key(&manifest.public_key.bytes()).is_none() {
        return Err(refused("dev_public_key_invalid"));
    }
    Ok(DevProject {
        path: path.to_path_buf(),
        manifest,
        url: server_url(&config.dev.host, config.dev.port)?,
    })
}

/// A registered development version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DevRegistration {
    pub id: Uuid,
    pub prefix: TablePrefix,
    pub title: String,
    pub project_path: String,
    pub url: String,
}

const COLUMNS: &str = "id, public_key, name, display_name, project_path, dev_url";

type RawRegistration = (String, String, String, Option<String>, String, String);

fn raw(r: &haex_crdt::rusqlite::Row<'_>) -> haex_crdt::rusqlite::Result<RawRegistration> {
    Ok((
        r.get(0)?,
        r.get(1)?,
        r.get(2)?,
        r.get(3)?,
        r.get(4)?,
        r.get(5)?,
    ))
}

/// A row as a registration; `None` for a row holzi cannot read.
fn parsed(
    (id, key, name, display_name, project_path, url): RawRegistration,
) -> Option<DevRegistration> {
    let prefix = TablePrefix {
        public_key: PublicKey::parse_case_insensitive(&key).ok()?,
        name: ExtensionName::parse(&name).ok()?,
    };
    Some(DevRegistration {
        id: Uuid::parse_str(&id).ok()?,
        title: display_name.unwrap_or_else(|| name.clone()),
        prefix,
        project_path,
        url,
    })
}

/// The development versions registered on `device`; a row holzi cannot read is left out.
pub fn registrations(q: &mut impl Query, device: Uuid) -> Result<Vec<DevRegistration>> {
    let rows = q.query_map(
        &format!(
            "SELECT {COLUMNS} FROM dev_extensions_no_sync WHERE vault_device_uuid = ?1 ORDER BY name"
        ),
        params![device.to_string()],
        raw,
    )?;
    Ok(rows.into_iter().filter_map(parsed).collect())
}

/// The registration `id`, on any device of this vault file.
pub fn registration(q: &mut impl Query, id: Uuid) -> Result<Option<DevRegistration>> {
    let row = q.query_row(
        &format!("SELECT {COLUMNS} FROM dev_extensions_no_sync WHERE id = ?1"),
        params![id.to_string()],
        raw,
    )?;
    Ok(row.and_then(parsed))
}

/// The project of `registration` as its folder reads now. Refused when the folder now names
/// another prefix (key or name changed): the tables, permissions and store belong to the
/// registered one, so the developer loads the project again. Blocking.
pub fn current_project(registration: &DevRegistration) -> Result<DevProject> {
    let project = read_project(Path::new(&registration.project_path))?;
    if project.prefix() != registration.prefix {
        return Err(refused("dev_project_changed"));
    }
    Ok(project)
}

/// The prefixes of every development version in this vault file, as table prefixes.
pub fn prefixes(q: &mut impl Query) -> haex_crdt::Result<HashSet<String>> {
    let rows: Vec<(String, String)> = q.query_map(
        "SELECT public_key, name FROM dev_extensions_no_sync",
        &[],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    Ok(rows
        .into_iter()
        .filter_map(|(key, name)| {
            let prefix = TablePrefix {
                public_key: PublicKey::parse_case_insensitive(&key).ok()?,
                name: ExtensionName::parse(&name).ok()?,
            };
            Some(prefix.to_string())
        })
        .collect())
}

/// Why a development version of `prefix` may not load on `device`, if it may not.
pub fn conflict(
    q: &mut impl Query,
    prefix: &TablePrefix,
    device: Uuid,
) -> Result<Option<&'static str>> {
    let installed = q
        .query_row(
            "SELECT COUNT(*) FROM extensions WHERE lower(public_key) = ?1 AND name = ?2",
            params![prefix.public_key.as_str(), prefix.name.as_str()],
            |r| r.get::<_, i64>(0),
        )?
        .unwrap_or(0)
        > 0;
    if installed {
        return Ok(Some("dev_installed_conflict"));
    }
    let own = registration(
        q,
        dev_extension_id(device, &prefix.public_key, &prefix.name),
    )?;
    if own.is_none() && !prefixed(q, prefix)?.is_empty() {
        return Ok(Some("dev_tables_exist"));
    }
    Ok(None)
}

/// What the developer confirms before a development version loads.
pub fn preview(q: &mut impl Query, project: &DevProject, device: Uuid) -> Result<InstallPreview> {
    if let Some(reason) = conflict(q, &project.prefix(), device)? {
        return Err(refused(reason));
    }
    let manifest = &project.manifest;
    let id = dev_extension_id(device, &manifest.public_key, &manifest.name);
    Ok(InstallPreview {
        signature_valid: false,
        error: None,
        extension_id: Some(id.to_string()),
        name: Some(manifest.name.as_str().to_owned()),
        display_name: manifest.display_name.clone(),
        version: Some(manifest.version.to_string()),
        description: manifest.description.clone(),
        author: manifest.author.clone(),
        publisher_fingerprint: Some(publisher_fingerprint(manifest.public_key.as_str())),
        declared: manifest
            .permissions
            .declared
            .iter()
            .map(|d| DeclaredPermissionView::of(d, manifest))
            .collect(),
        unsupported_categories: manifest.permissions.unsupported_categories.clone(),
        existing: None,
        replaces_same_version: false,
        same_name_other_publisher: false,
    })
}

/// Registers the project at `path` on `device` with the developer's permission choices, or takes
/// over its manifest again. Refused while developer mode is off or the prefix is taken. Blocking.
pub fn confirm(
    db: &VaultDb,
    path: &Path,
    choices: Vec<PermissionChoice>,
    device: Uuid,
    now_ms: i64,
) -> Result<Uuid> {
    let project = read_project(path)?;
    db.write_blocking(move |tx| {
        if !mode(tx, device)? {
            return Err(refused("dev_mode_off").into());
        }
        let prefix = project.prefix();
        if let Some(reason) = conflict(tx, &prefix, device)? {
            return Err(refused(reason).into());
        }
        let manifest = &project.manifest;
        let id = dev_extension_id(device, &manifest.public_key, &manifest.name);
        let known = registration(tx, id)?.is_some();
        let path = project.path.to_string_lossy().into_owned();
        if known {
            tx.execute(
                "UPDATE dev_extensions_no_sync SET display_name = ?2, project_path = ?3, \
                 dev_url = ?4, updated_at = ?5 WHERE id = ?1",
                params![
                    id.to_string(),
                    manifest.display_name,
                    path,
                    project.url,
                    now_ms
                ],
            )?;
        } else {
            tx.execute(
                "INSERT INTO dev_extensions_no_sync (id, public_key, name, display_name, enabled, \
                 vault_device_uuid, project_path, dev_url, installed_at, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, 1, ?5, ?6, ?7, ?8, ?8)",
                params![
                    id.to_string(),
                    manifest.public_key.as_str(),
                    manifest.name.as_str(),
                    manifest.display_name,
                    device.to_string(),
                    path,
                    project.url,
                    now_ms
                ],
            )?;
        }
        apply_declarations(
            tx,
            id,
            &manifest.permissions.declared,
            &choices,
            device,
            now_ms,
        )?;
        Ok(id)
    })
}

/// Unloads a development version: drops its tables and deletes its journal, logs, registration,
/// permissions and store. Blocking.
///
/// Runs under the migration lock and reads the tables in the same write that drops them: a
/// migration of a frame still open cannot create a table in between, which would stay behind
/// without a registration and block loading the prefix again (`dev_tables_exist`).
pub fn unload(db: &VaultDb, id: Uuid) -> Result<()> {
    let _applying = applying().lock().unwrap_or_else(PoisonError::into_inner);
    // holzi's own statements on names read from sqlite_master, as when clearing up a removal.
    let guard = SqlGuard {
        authorizer: Arc::new(|_: &AuthContext<'_>| Authorization::Allow),
        progress: None,
        max_value_bytes: None,
    };
    let found = db.write_guarded_blocking(
        &guard,
        GuardedWriteOptions {
            schema_mode: true,
            local: true,
        },
        move |tx| {
            let Some(registration) = registration(tx, id)? else {
                return Ok(false);
            };
            let objects = prefixed(tx, &registration.prefix)?;
            for (kind, name) in &objects {
                let kind = if kind == "view" { "VIEW" } else { "TABLE" };
                tx.execute(&format!("DROP {kind} IF EXISTS {}", quoted(name)), &[])?;
            }
            let ext = id.to_string();
            tx.execute(
                &format!("DELETE FROM {MIGRATION_JOURNAL} WHERE extension_id = ?1"),
                params![ext],
            )?;
            tx.execute(
                "DELETE FROM extension_logs_no_sync WHERE extension_id = ?1",
                params![ext],
            )?;
            // Schema mode switches foreign keys off, so nothing cascades: every row goes by name.
            for table in [
                "dev_extension_permissions_no_sync",
                "dev_extension_kv_no_sync",
            ] {
                tx.execute(
                    &format!("DELETE FROM {table} WHERE extension_id = ?1"),
                    params![ext],
                )?;
            }
            tx.execute(
                "DELETE FROM dev_extensions_no_sync WHERE id = ?1",
                params![ext],
            )?;
            Ok(true)
        },
    )?;
    if found {
        Ok(())
    } else {
        Err(HolziError::ExtensionNotFound)
    }
}

/// The registration a frame of `id` opens on `device`: developer mode must be on.
pub fn start(q: &mut impl Query, id: Uuid, device: Uuid) -> Result<DevRegistration> {
    if !mode(q, device)? {
        return Err(refused("dev_mode_off"));
    }
    registration(q, id)?
        .filter(|r| dev_extension_id(device, &r.prefix.public_key, &r.prefix.name) == r.id)
        .ok_or(HolziError::ExtensionNotFound)
}

#[cfg(test)]
#[path = "dev_tests.rs"]
mod tests;
