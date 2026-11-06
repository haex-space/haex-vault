//! The agent browser's view of the app (spec 065, research R8, data-model.md): the rights of an
//! external agent come from its grants (021), those of the built-in agent from the device setting
//! [`BUILTIN_KEY`]; the browser starts with the open vault, its profile in the vault instance's
//! folder and its process a child of the vault session. Downloads land in the vault's folder of
//! agent downloads in the system's downloads folder; uploads follow the agent's file rights (044).

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use tauri::{AppHandle, Manager, Runtime};
use uuid::Uuid;

use super::dns;
use super::error::BrowserError;
use super::executor::BrowserEnv;
use super::filter_fetch::HttpFetcher;
use super::filter_lists::FilterLists;
use super::filters::LiveFilter;
use super::locate::{self, Os, System};
use super::profile::{self, Workdir};
use super::profile_store::{self, ProfileStore};
use super::prompts::BrowserProfileNote;
use super::sites::Site;
use super::StartConfig;
use crate::caller::Caller;
use crate::files::access::AgentGrants;
use crate::files::local::OwnPlaces;
use crate::files::permissions::{self, BUILTIN_AGENT};
use crate::files::state::FilesState;
use crate::mcp_server::grants::{
    validate, BrowserRights, PasswordRights, RequestRule, Rights, SiteGrant, SiteLevel,
};
use crate::mcp_server::registry;
use crate::passwords::service::PasswordsService;

#[cfg(test)]
#[path = "env_tests.rs"]
mod tests;
use crate::storage::preferences::{self, PrefScope};
use crate::storage::query::Query;
use crate::AppState;

/// The device setting with the browser rights of the built-in agent (data-model.md); unset is off.
pub const BUILTIN_KEY: &str = "agent_browser.builtin";

/// The device setting with the password rights of the built-in agent for the agent browser
/// (decided 2026-11-02): the entries its fills may use; unset is off.
pub const BUILTIN_PASSWORDS_KEY: &str = "agent_browser.builtin_passwords";

/// The table of the browser chosen on this device (migration 0047).
const TABLE: &str = "haex_agent_browser_no_sync";

/// The open vault's device.
pub(super) async fn device<R: Runtime>(app: &AppHandle<R>) -> Result<Uuid, String> {
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
            ..Default::default()
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

/// The built-in agent's password rights for fills on this device.
pub async fn builtin_password_rights<R: Runtime>(app: &AppHandle<R>) -> Option<PasswordRights> {
    let device = device(app).await.ok()?;
    let db = app.state::<AppState>().database().ok()?;
    let raw = db
        .read(move |q| preferences::get(q, PrefScope::Device(device), BUILTIN_PASSWORDS_KEY))
        .await
        .ok()
        .flatten()?;
    serde_json::from_str(&raw).ok()
}

/// Sets (or, with `None`, switches off) the built-in agent's password rights for fills.
pub async fn set_builtin_password_rights<R: Runtime>(
    app: &AppHandle<R>,
    rights: Option<PasswordRights>,
) -> Result<(), String> {
    validate_builtin_password_rights(rights.as_ref())?;
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
        let scope = PrefScope::Device(device);
        match value {
            Some(value) => preferences::insert_or_update(tx, scope, BUILTIN_PASSWORDS_KEY, &value)?,
            None => preferences::delete(tx, scope, BUILTIN_PASSWORDS_KEY)?,
        };
        Ok(())
    })
    .await
    .map_err(|error| error.to_string())
}

/// The built-in agent's password rights follow the rules of an external agent's: writing (to save
/// new logins, spec 065 FR-036, operator 2026-11-02) needs the tag for new entries within tags.
fn validate_builtin_password_rights(rights: Option<&PasswordRights>) -> Result<(), String> {
    let Some(rights) = rights else {
        return Ok(());
    };
    validate(&Rights {
        passwords: Some(rights.clone()),
        ..Default::default()
    })
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

/// How to start the agent browser now: the browser found or chosen on this device, the working
/// directory and the encrypted store of the profile of the open vault instance.
pub async fn start_config<R: Runtime>(app: &AppHandle<R>) -> Result<StartConfig, BrowserError> {
    let os = Os::current().ok_or(BrowserError::Unsupported)?;
    let chosen = chosen_path(app).await;
    let executable = locate::find(os, chosen.as_deref(), &System).ok_or_else(|| {
        BrowserError::NotFound(
            "no Chromium-based browser (Chromium, Chrome, Edge, Brave) was found".to_owned(),
        )
    })?;
    let state = app.state::<AppState>();
    let instance = state
        .active_name()
        .ok()
        .flatten()
        .ok_or(BrowserError::VaultClosed)?;
    let handle = app.clone();
    let store = ProfileStore::new(
        &profile_dir(app)?,
        Arc::new(move || {
            handle.state::<AppState>().database().map_err(|_| {
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
        dns: dns::load(&device_dir(app)?),
    })
}

/// The app as the browser executor sees it.
pub struct TauriBrowserEnv<R: Runtime> {
    app: AppHandle<R>,
    /// The filter lists of the tracker blocking; their files live on this device (FR-045).
    filters: Option<Arc<FilterLists>>,
}

impl<R: Runtime> TauriBrowserEnv<R> {
    /// With the filter lists of this device ([`device_filter_lists`]), which the settings share.
    pub fn new(app: AppHandle<R>, filters: Option<Arc<FilterLists>>) -> Self {
        Self { app, filters }
    }
}

/// The filter lists of this device, with the choice kept next to them; `None` when their folder
/// or the download client cannot be had, which leaves the agent browser without blocking.
pub fn device_filter_lists<R: Runtime>(app: &AppHandle<R>) -> Option<Arc<FilterLists>> {
    filter_lists_dir(app)
        .and_then(|dir| Ok(FilterLists::new(dir, Arc::new(HttpFetcher::new()?))))
        .map_err(|error| log::warn!("agent browser: no tracker filters: {error}"))
        .ok()
}

/// The agent browser's folder of this device: its device-wide files (the filter lists, the DNS
/// choice) live here, outside every vault and every sync. An instance name has no dot, so a file
/// with one cannot be an instance's folder.
pub fn device_dir<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, BrowserError> {
    crate::instances::paths::get_app_local_data(app)
        .map(|data| data.join("agent-browser"))
        .map_err(|error| BrowserError::Profile(error.to_string()))
}

/// The folder of the downloaded filter lists.
fn filter_lists_dir<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    device_dir(app)
        .map(|dir| dir.join("filters"))
        .map_err(|error| error.to_string())
}

#[async_trait]
impl<R: Runtime> BrowserEnv for TauriBrowserEnv<R> {
    async fn filter(&self) -> Option<LiveFilter> {
        self.filters.as_ref()?.live().await
    }

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
        let site = site.clone();
        super::env_rights::change_browser_rights(&self.app, caller, move |rights| {
            rights.sites.insert(site, grant);
            true
        })
        .await
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

    async fn profile_note(&self, caller: &Caller) -> Option<BrowserProfileNote> {
        let Caller::ExternalAgent { id } = caller else {
            return None;
        };
        let agent = Uuid::parse_str(id).ok()?;
        let db = self.app.state::<AppState>().database().ok()?;
        db.read(move |q| {
            let (profile, _) = crate::agent_profiles::resolve::profile_of(
                q,
                &crate::agent_profiles::Subject::Agent(agent),
            )?;
            let view = crate::agent_profiles::subjects::profile_views(q)?
                .into_iter()
                .find(|view| view.id == profile.to_string());
            Ok(view.map(|view| BrowserProfileNote {
                id: view.id,
                name: view.name,
                is_standard: view.is_standard,
                member_count: view.member_count,
            }))
        })
        .await
        .ok()
        .flatten()
    }

    fn locale(&self) -> String {
        self.app.state::<AppState>().extensions().context().locale
    }

    fn downloads_folder(&self) -> Result<PathBuf, String> {
        let vault = self
            .app
            .state::<AppState>()
            .active_name()
            .ok()
            .flatten()
            .ok_or("no open vault")?;
        let downloads = self
            .app
            .path()
            .download_dir()
            .map_err(|error| error.to_string())?;
        Ok(super::downloads::folder(&downloads, &vault))
    }

    async fn file_rights(&self, caller: &Caller) -> Option<(OwnPlaces, AgentGrants)> {
        let agent_id = match caller {
            Caller::BuiltinAgent => BUILTIN_AGENT.to_owned(),
            Caller::ExternalAgent { id } => id.clone(),
            _ => return None,
        };
        let own = self.app.state::<FilesState>().own.clone();
        let db = self.app.state::<AppState>().database().ok()?;
        let grants = db
            .read(move |q| Ok(permissions::grants_of(q, &agent_id)?))
            .await
            .ok()?;
        Some((own, grants))
    }

    async fn password_rights(&self, caller: &Caller) -> Option<PasswordRights> {
        match caller {
            Caller::BuiltinAgent => builtin_password_rights(&self.app).await,
            Caller::ExternalAgent { id } => {
                let agent = Uuid::parse_str(id).ok()?;
                let device = device(&self.app).await.ok()?;
                let db = self.app.state::<AppState>().database().ok()?;
                db.read(move |q| registry::load_rights(q, device, agent))
                    .await
                    .ok()?
                    .passwords
            }
            _ => None,
        }
    }

    fn passwords(&self) -> Option<PasswordsService> {
        let state = self.app.state::<AppState>();
        let db = state.database().ok()?;
        Some(PasswordsService::with_usage(db, state.usage()))
    }

    async fn switch_on(&self, caller: &Caller) -> Result<(), String> {
        match caller {
            // Already on (another call allowed it meanwhile): keep its websites.
            Caller::BuiltinAgent if builtin_rights(&self.app).await.is_some() => Ok(()),
            Caller::BuiltinAgent => {
                set_builtin_rights(&self.app, Some(BrowserRights::switched_on())).await
            }
            _ => Err("only the built-in agent is switched on from the chat".to_owned()),
        }
    }

    async fn remember_fill(&self, caller: &Caller, site: &Site) -> Result<(), String> {
        let site = site.clone();
        super::env_rights::change_browser_rights(&self.app, caller, move |rights| {
            super::sites::remember_fill(rights, &site)
        })
        .await
    }

    async fn remember_request(
        &self,
        caller: &Caller,
        site: &Site,
        rule: RequestRule,
    ) -> Result<(), String> {
        let site = site.clone();
        super::env_rights::change_browser_rights(&self.app, caller, move |rights| {
            rights.requests.insert(site, rule);
            true
        })
        .await
    }

    async fn start_config(&self) -> Result<StartConfig, BrowserError> {
        start_config(&self.app).await
    }
}
