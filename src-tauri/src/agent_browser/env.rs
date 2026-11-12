//! The agent browser's view of the app (spec 065, research R8, data-model.md): the rights of an
//! external agent and of the local agent come from their agent profile (spec 083); the browser starts with the open vault, its profile in the vault instance's
//! folder and its process a child of the vault session. Downloads land in the vault's folder of
//! agent downloads in the system's downloads folder; uploads follow the agent's file rights (044).

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use uuid::Uuid;

use super::choices;
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
use crate::agent_profiles::identity::Identity;
use crate::caller::Caller;
use crate::files::access::AgentGrants;
use crate::files::local::OwnPlaces;
use crate::files::permissions::BUILTIN_AGENT;
use crate::files::state::FilesState;
use crate::mcp_server::grants::{
    BrowserRights, PasswordRights, RequestRule, Rights, SiteGrant, SiteLevel,
};
use crate::mcp_server::registry;
use crate::passwords::service::PasswordsService;

use crate::storage::query::Query;
use crate::AppState;

/// What the local agent may do in the running call (spec 083 US5): the rights of the agent profile
/// of the model of its turn, "Standard" outside one.
async fn local_rights<R: Runtime>(app: &AppHandle<R>) -> Option<Rights> {
    let db = app.state::<AppState>().database().ok()?;
    let subject = crate::agent_profiles::local::builtin_subject();
    db.read(move |q| crate::agent_profiles::local::rights_for(q, subject.as_ref()))
        .await
        .ok()
}

/// The table of the browser chosen on this device (migration 0047).
const TABLE: &str = "haex_agent_browser_no_sync";

/// The open vault's device.
pub(super) async fn device<R: Runtime>(app: &AppHandle<R>) -> Result<Uuid, String> {
    let state = app.state::<AppState>();
    crate::mcp_server::commands::open_device(app, &state)
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
        dns: choices::dns(&state.database().map_err(|_| BrowserError::VaultClosed)?)
            .await
            .map_err(|error| BrowserError::Profile(error.to_string()))?,
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

/// The folder of the downloaded filter lists.
fn filter_lists_dir<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    crate::instances::paths::vault_paths(app)
        .map(|paths| paths.filter_lists())
        .map_err(|error| error.to_string())
}

#[async_trait]
impl<R: Runtime> BrowserEnv for TauriBrowserEnv<R> {
    async fn filter(&self) -> Option<LiveFilter> {
        self.filters.as_ref()?.live().await
    }

    async fn reject_cookies(&self) -> bool {
        let Ok(db) = self.app.state::<AppState>().database() else {
            return true;
        };
        super::choices::consent(&db)
            .await
            .map_or(true, |settings| settings.reject_optional)
    }

    async fn rights(&self, caller: &Caller) -> Option<BrowserRights> {
        match caller {
            Caller::BuiltinAgent => local_rights(&self.app).await?.browser,
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

    async fn remember(
        &self,
        caller: &Caller,
        site: &Site,
        level: SiteLevel,
        profile_id: Option<&str>,
    ) -> Result<(), String> {
        let grant = SiteGrant {
            level,
            fill_remembered: false,
        };
        let site = site.clone();
        super::env_rights::change_browser_rights(
            &self.app,
            caller,
            profile_id.map(str::to_owned),
            move |rights| {
                rights.sites.insert(site, grant);
                true
            },
        )
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
        let agent = match caller {
            Caller::ExternalAgent { id } => Some(Uuid::parse_str(id).ok()?),
            Caller::BuiltinAgent => None,
            _ => return None,
        };
        let device = device(&self.app).await.ok()?;
        let builtin = crate::agent_profiles::local::builtin_subject();
        let db = self.app.state::<AppState>().database().ok()?;
        db.read(move |q| {
            let subject = match agent {
                Some(agent) => Some(registry::subject_of_id(q, device, agent)?),
                None => builtin,
            };
            let profile = match &subject {
                Some(subject) => crate::agent_profiles::resolve::profile_of(q, subject)?.0,
                None => crate::agent_profiles::STANDARD_PROFILE_ID,
            };
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
        let state = self.app.state::<AppState>();
        let db = state.database().ok()?;
        let device = device(&self.app).await.ok()?;
        let identity = Identity::of(&agent_id, device)?;
        let (rights, _) = identity.rights(&db).await.ok()?;
        Some((own, rights.files.grants()))
    }

    async fn password_rights(&self, caller: &Caller) -> Option<PasswordRights> {
        match caller {
            Caller::BuiltinAgent => local_rights(&self.app).await?.passwords,
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

    async fn switch_on(&self, caller: &Caller, profile_id: Option<&str>) -> Result<(), String> {
        match caller {
            // Already on (another call allowed it meanwhile): keep its websites. The group is
            // switched on in the agent profile of the model of the turn (spec 083 US5).
            Caller::BuiltinAgent => {
                let subject = crate::agent_profiles::local::builtin_subject();
                let db = self
                    .app
                    .state::<AppState>()
                    .database()
                    .map_err(|error| error.to_string())?;
                let expected = profile_id.map(str::to_owned);
                db.write(move |tx| {
                    if let Some(expected) = expected.as_deref() {
                        let actual = match subject.as_ref() {
                            Some(subject) => {
                                crate::agent_profiles::resolve::profile_of(tx, subject)?
                                    .0
                                    .to_string()
                            }
                            None => crate::agent_profiles::STANDARD_PROFILE_ID.to_string(),
                        };
                        if actual != expected {
                            return Err(haex_crdt::Error::consumer(
                                "agent profile changed while approval was open",
                            ));
                        }
                    }
                    crate::agent_profiles::remember::switch_browser_on(tx, subject.as_ref())
                })
                .await
                .map_err(|error| error.to_string())?;
                let _ = self
                    .app
                    .emit(crate::mcp_server::prompts::EVENT_AGENTS_CHANGED, ());
                Ok(())
            }
            _ => Err("only the built-in agent is switched on from the chat".to_owned()),
        }
    }

    async fn remember_fill(
        &self,
        caller: &Caller,
        site: &Site,
        profile_id: Option<&str>,
    ) -> Result<(), String> {
        let site = site.clone();
        super::env_rights::change_browser_rights(
            &self.app,
            caller,
            profile_id.map(str::to_owned),
            move |rights| super::sites::remember_fill(rights, &site),
        )
        .await
    }

    async fn remember_request(
        &self,
        caller: &Caller,
        site: &Site,
        rule: RequestRule,
        profile_id: Option<&str>,
    ) -> Result<(), String> {
        let site = site.clone();
        super::env_rights::change_browser_rights(
            &self.app,
            caller,
            profile_id.map(str::to_owned),
            move |rights| {
                rights.requests.insert(site, rule);
                true
            },
        )
        .await
    }

    async fn start_config(&self) -> Result<StartConfig, BrowserError> {
        start_config(&self.app).await
    }
}
