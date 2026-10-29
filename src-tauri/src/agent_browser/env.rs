//! The agent browser's view of the app (spec 065, research R8, data-model.md): the rights of an
//! external agent come from its grants (021), those of the built-in agent from the device setting
//! [`BUILTIN_KEY`]; the browser starts with the open vault, its profile in the vault instance's
//! folder and its process a child of the vault session.

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use uuid::Uuid;

use super::error::BrowserError;
use super::executor::BrowserEnv;
use super::locate::{self, Os, System};
use super::profile::{self, Workdir};
use super::profile_store::{self, ProfileStore};
use super::sites::Site;
use super::StartConfig;
use crate::caller::Caller;
use crate::mcp_server::grants::{validate, BrowserRights, Rights, SiteGrant, SiteLevel};
use crate::mcp_server::registry;
use crate::storage::preferences::{self, PrefScope};
use crate::storage::query::Query;
use crate::AppState;

/// The device setting with the browser rights of the built-in agent (data-model.md); unset is off.
pub const BUILTIN_KEY: &str = "agent_browser.builtin";

/// The table of the browser chosen on this device (migration 0047).
const TABLE: &str = "haex_agent_browser_no_sync";

/// The open vault's device.
async fn device<R: Runtime>(app: &AppHandle<R>) -> Result<Uuid, String> {
    let state = app.state::<AppState>();
    crate::mcp_server::commands::open_device(app, &state)
        .await
        .map_err(|error| error.to_string())
}

/// The built-in agent's browser rights on `device`.
pub async fn builtin_rights<R: Runtime>(app: &AppHandle<R>) -> Option<BrowserRights> {
    let device = device(app).await.ok()?;
    let db = app.state::<AppState>().database().ok()?;
    let raw = db
        .read(move |q| preferences::get(q, PrefScope::Device(device), BUILTIN_KEY))
        .await
        .ok()
        .flatten()?;
    serde_json::from_str(&raw).ok()
}

/// Sets (or, with `None`, switches off) the built-in agent's browser rights.
pub async fn set_builtin_rights<R: Runtime>(
    app: &AppHandle<R>,
    rights: Option<BrowserRights>,
) -> Result<(), String> {
    if let Some(rights) = &rights {
        validate(&Rights {
            operate: Default::default(),
            browser: Some(rights.clone()),
        })
        .map_err(|error| error.to_string())?;
    }
    let device = device(app).await?;
    let db = app
        .state::<AppState>()
        .database()
        .map_err(|error| error.to_string())?;
    let value = rights
        .map(|rights| serde_json::to_string(&rights))
        .transpose()
        .map_err(|error| error.to_string())?;
    db.write(move |tx| {
        match value {
            Some(value) => {
                preferences::insert_or_update(tx, PrefScope::Device(device), BUILTIN_KEY, &value)?
            }
            None => preferences::delete(tx, PrefScope::Device(device), BUILTIN_KEY)?,
        };
        Ok(())
    })
    .await
    .map_err(|error| error.to_string())
}

/// The browser chosen on this device; `None` means: search.
pub async fn chosen_path<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    let device = device(app).await.ok()?;
    let db = app.state::<AppState>().database().ok()?;
    db.read(move |q| {
        q.query_row(
            &format!("SELECT browser_path FROM {TABLE} WHERE vault_device_uuid = ?1"),
            haex_crdt::rusqlite::params![device.to_string()],
            |row| row.get::<_, Option<String>>(0),
        )
    })
    .await
    .ok()
    .flatten()
    .flatten()
    .filter(|path| !path.is_empty())
    .map(PathBuf::from)
}

/// Chooses the browser of this device (`None`: search again).
pub async fn set_chosen_path<R: Runtime>(
    app: &AppHandle<R>,
    path: Option<String>,
) -> Result<(), String> {
    let device = device(app).await?;
    let db = app
        .state::<AppState>()
        .database()
        .map_err(|error| error.to_string())?;
    let now = crate::passwords::clock::unix_millis(std::time::SystemTime::now());
    db.write(move |tx| {
        tx.execute(
            &format!(
                "INSERT INTO {TABLE} (vault_device_uuid, browser_path, created_at) \
                 VALUES (?1, ?2, ?3) \
                 ON CONFLICT(vault_device_uuid) DO UPDATE SET browser_path = excluded.browser_path"
            ),
            haex_crdt::rusqlite::params![device.to_string(), path, now],
        )?;
        Ok(())
    })
    .await
    .map_err(|error| error.to_string())
}

/// The folder of the profile of the open vault instance.
pub fn profile_dir<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, BrowserError> {
    let instance = app
        .state::<AppState>()
        .active_name()
        .ok()
        .flatten()
        .ok_or(BrowserError::VaultClosed)?;
    let data = crate::instances::paths::get_app_local_data(app)
        .map_err(|error| BrowserError::Profile(error.to_string()))?;
    Ok(profile_store::dir(&data, &instance))
}

/// The app as the browser executor sees it.
pub struct TauriBrowserEnv<R: Runtime> {
    app: AppHandle<R>,
}

impl<R: Runtime> TauriBrowserEnv<R> {
    pub fn new(app: AppHandle<R>) -> Self {
        Self { app }
    }
}

#[async_trait]
impl<R: Runtime> BrowserEnv for TauriBrowserEnv<R> {
    async fn rights(&self, caller: &Caller) -> Option<BrowserRights> {
        match caller {
            Caller::BuiltinAgent => builtin_rights(&self.app).await,
            Caller::ExternalAgent { id } => {
                let agent = Uuid::parse_str(id).ok()?;
                let device = device(&self.app).await.ok()?;
                let db = self.app.state::<AppState>().database().ok()?;
                db.read(move |q| registry::load_rights(q, device, agent))
                    .await
                    .ok()?
                    .browser
            }
            _ => None,
        }
    }

    async fn remember(&self, caller: &Caller, site: &Site, level: SiteLevel) -> Result<(), String> {
        let grant = SiteGrant {
            level,
            fill_remembered: false,
        };
        match caller {
            Caller::BuiltinAgent => {
                let mut rights = builtin_rights(&self.app)
                    .await
                    .ok_or("the browser is off for the built-in agent")?;
                rights.sites.insert(site.clone(), grant);
                set_builtin_rights(&self.app, Some(rights)).await
            }
            Caller::ExternalAgent { id } => {
                let agent = Uuid::parse_str(id).map_err(|error| error.to_string())?;
                let device = device(&self.app).await?;
                let db = self
                    .app
                    .state::<AppState>()
                    .database()
                    .map_err(|error| error.to_string())?;
                let site = site.clone();
                db.write(move |tx| {
                    let mut rights = registry::load_rights(tx, device, agent)?;
                    if let Some(browser) = rights.browser.as_mut() {
                        browser.sites.insert(site, grant);
                        registry::save_rights(tx, device, agent, &rights)?;
                    }
                    Ok(())
                })
                .await
                .map_err(|error| error.to_string())?;
                let _ = self
                    .app
                    .emit(crate::mcp_server::prompts::EVENT_AGENTS_CHANGED, ());
                Ok(())
            }
            _ => Err("not an agent".to_owned()),
        }
    }

    async fn agent_name(&self, caller: &Caller) -> String {
        let Caller::ExternalAgent { id } = caller else {
            // The window shows its own name for the built-in agent.
            return String::new();
        };
        let (Ok(agent), Ok(device)) = (Uuid::parse_str(id), device(&self.app).await) else {
            return String::new();
        };
        let Ok(db) = self.app.state::<AppState>().database() else {
            return String::new();
        };
        db.read(move |q| registry::get(q, device, agent))
            .await
            .ok()
            .flatten()
            .map(|agent| agent.client_name)
            .unwrap_or_default()
    }

    async fn start_config(&self) -> Result<StartConfig, BrowserError> {
        let os = Os::current().ok_or(BrowserError::Unsupported)?;
        let chosen = chosen_path(&self.app).await;
        let executable = locate::find(os, chosen.as_deref(), &System).ok_or_else(|| {
            BrowserError::NotFound(
                "no Chromium-based browser (Chromium, Chrome, Edge, Brave) was found".to_owned(),
            )
        })?;
        let state = self.app.state::<AppState>();
        let instance = state
            .active_name()
            .ok()
            .flatten()
            .ok_or(BrowserError::VaultClosed)?;
        let app = self.app.clone();
        let store = ProfileStore::new(
            &profile_dir(&self.app)?,
            Arc::new(move || {
                app.state::<AppState>().database().map_err(|_| {
                    crate::files::FilesError::new(
                        crate::files::FilesErrorCode::Unsupported,
                        "no open vault",
                    )
                })
            }),
        )?;
        Ok(StartConfig {
            os,
            executable,
            workdir: Workdir::of(&profile::root(os)?, &instance),
            store,
            gate: Some(state.gate().clone()),
            headless: false,
        })
    }
}
