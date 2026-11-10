//! The filter lists of the agent browser's tracker blocking (spec 065 FR-045, research R16): the
//! default lists of uBlock Origin and the user's own, downloaded from their publishers (never
//! shipped), kept as files on this device without sync (the downloaded copies only; the choice of
//! lists belongs to the vault, see `choices`), refreshed after seven days and compiled
//! into one [`TrackerFilter`]. A list that fails keeps its last good copy.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant, SystemTime};

use tokio::task::JoinHandle;

use super::filter_fetch::{looks_like_list, require_https, ListFetcher};
use super::filter_settings::{
    ActiveList, TrackerListView, TrackerOverview, TrackerSettings, DEFAULT_LISTS,
};
use super::filters::{LiveFilter, TrackerFilter};

/// A list is downloaded again after this long.
pub const REFRESH_AFTER: Duration = Duration::from_secs(7 * 24 * 60 * 60);
/// How long the first start waits for lists it has never had.
pub const FIRST_WAIT: Duration = Duration::from_secs(10);
/// How often a running holzi looks at the age of its lists again.
const RECHECK: Duration = Duration::from_secs(60 * 60);

#[derive(Default)]
struct State {
    engine: Option<Arc<TrackerFilter>>,
    /// The version of the settings the engine was built for.
    version: u64,
    checked: Option<Instant>,
}

/// The lists of this device and the engine over them.
pub struct FilterLists {
    dir: PathBuf,
    fetcher: Arc<dyn ListFetcher>,
    settings: Mutex<TrackerSettings>,
    version: std::sync::atomic::AtomicU64,
    state: tokio::sync::Mutex<State>,
    /// The engine of [`State`] as the tabs ask it per request.
    live: LiveFilter,
    errors: Mutex<BTreeMap<String, String>>,
    /// Downloads that run now.
    downloading: std::sync::atomic::AtomicUsize,
}

impl FilterLists {
    /// The lists in `dir`, with the default choice until a vault gives its own
    /// ([`Self::set_settings`]).
    pub fn new(dir: PathBuf, fetcher: Arc<dyn ListFetcher>) -> Arc<Self> {
        Arc::new(Self {
            dir,
            fetcher,
            settings: Mutex::new(TrackerSettings::default()),
            version: std::sync::atomic::AtomicU64::new(1),
            state: tokio::sync::Mutex::default(),
            live: LiveFilter::default(),
            errors: Mutex::default(),
            downloading: std::sync::atomic::AtomicUsize::new(0),
        })
    }

    pub fn settings(&self) -> TrackerSettings {
        self.settings
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// The user's choice (the settings page, `settings.agentBrowser.trackers.set`): checked and
    /// applied at once, to the tabs that are open as well. Lists that are new or switched on are
    /// downloaded now. Keeping the choice in the vault is the caller's business (`choices`), and
    /// it does that first with [`TrackerSettings::validated`]. Answers the choice as it counts.
    pub async fn apply(
        self: &Arc<Self>,
        settings: TrackerSettings,
    ) -> Result<TrackerSettings, String> {
        let settings = settings.validated()?;
        let before = self.settings();
        self.set_settings(settings.clone());
        // Counted as a download from here on, so the answer of [`Self::overview`] already says
        // "loading" for a list that is new, before the task below has started its downloads.
        self.downloading
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let this = Arc::clone(self);
        let removed: Vec<String> = before
            .custom
            .iter()
            .filter(|old| settings.custom.iter().all(|new| new.id != old.id))
            .map(|old| old.id.clone())
            .collect();
        tokio::spawn(async move {
            this.engine().await;
            this.downloading
                .fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        });
        // The copy and the error of a list that is gone stay nowhere.
        for id in removed {
            let _ = tokio::fs::remove_file(self.path_of(&id)).await;
            self.errors
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(&id);
        }
        Ok(settings)
    }

    /// The lists, their files and their errors, for the settings page.
    pub fn overview(&self) -> TrackerOverview {
        let settings = self.settings();
        let errors = self.errors.lock().unwrap_or_else(PoisonError::into_inner);
        let view = |id: &str, name: &str, url: &str, custom: bool, enabled: bool| {
            let fetched = std::fs::metadata(self.path_of(id))
                .and_then(|meta| meta.modified())
                .ok()
                .and_then(|time| time.duration_since(SystemTime::UNIX_EPOCH).ok())
                .and_then(|since| i64::try_from(since.as_millis()).ok());
            TrackerListView {
                id: id.to_owned(),
                name: name.to_owned(),
                url: url.to_owned(),
                custom,
                enabled,
                fetched_at: fetched,
                error: errors.get(id).cloned(),
            }
        };
        let lists: Vec<TrackerListView> = DEFAULT_LISTS
            .iter()
            .map(|list| {
                view(
                    list.id,
                    list.name,
                    list.url,
                    false,
                    !settings.off.contains(list.id),
                )
            })
            .chain(
                settings
                    .custom
                    .iter()
                    .map(|list| view(&list.id, &list.name, &list.url, true, list.enabled)),
            )
            .collect();
        let loading = settings.enabled
            && self.downloading.load(std::sync::atomic::Ordering::SeqCst) > 0
            && lists
                .iter()
                .any(|list| list.enabled && list.fetched_at.is_none());
        TrackerOverview {
            enabled: settings.enabled,
            lists,
            loading,
        }
    }

    /// Takes the user's choice; the engine is built anew when it is next asked for.
    pub fn set_settings(&self, settings: TrackerSettings) {
        *self.settings.lock().unwrap_or_else(PoisonError::into_inner) = settings;
        self.version
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }

    /// Why the last download of list `id` failed; `None` when it worked or has not run.
    pub fn error_of(&self, id: &str) -> Option<String> {
        self.errors
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(id)
            .cloned()
    }

    /// What a new tab asks per request: `None` while blocking is off; otherwise the engine as it is
    /// now and after every rebuild (see [`Self::engine`]), so every tab gets lists that arrive late.
    pub async fn live(self: &Arc<Self>) -> Option<LiveFilter> {
        self.engine().await;
        self.settings().enabled.then(|| self.live.clone())
    }

    /// The engine over the lists that are switched on; `None` while blocking is off or no list is
    /// there yet. Lists never had are waited for up to [`FIRST_WAIT`] while no engine is there;
    /// lists older than [`REFRESH_AFTER`] are used as they are and downloaded again meanwhile. The
    /// engine is compiled when the settings changed or a download brought a new copy, not else.
    pub async fn engine(self: &Arc<Self>) -> Option<Arc<TrackerFilter>> {
        // The version before the settings: a change between both is built for at the next ask.
        let version = self.version.load(std::sync::atomic::Ordering::SeqCst);
        let settings = self.settings();
        if !settings.enabled {
            self.live.set(None);
            return None;
        }
        let mut state = self.state.lock().await;
        if state.version == version && state.checked.is_some_and(|at| at.elapsed() < RECHECK) {
            return state.engine.clone();
        }
        state.checked = Some(Instant::now());
        let (mut missing, mut stale) = (Vec::new(), Vec::new());
        for list in settings.active() {
            match age(&self.path_of(&list.id)) {
                None => missing.push(list),
                Some(age) if age > REFRESH_AFTER => stale.push(list),
                Some(_) => {}
            }
        }
        if state.version == version && state.engine.is_some() {
            // The engine stands; new copies replace it once they are there.
            missing.append(&mut stale);
            if !missing.is_empty() {
                self.rebuild_after(self.download_all(missing));
            }
            return state.engine.clone();
        }
        // Only while nothing blocks yet is a download waited for.
        let wait = if state.engine.is_none() {
            FIRST_WAIT
        } else {
            Duration::ZERO
        };
        let mut first = self.download_all(missing);
        let waited = tokio::time::timeout(wait, &mut first).await.is_ok();
        let engine = self.build(&settings).await;
        state.engine = engine.clone();
        state.version = version;
        self.live.set(engine.clone());
        drop(state);
        if !waited {
            self.rebuild_after(first);
        }
        if !stale.is_empty() {
            self.rebuild_after(self.download_all(stale));
        }
        engine
    }

    /// Builds the engine again once `downloads` are done, when they brought a new copy.
    fn rebuild_after(self: &Arc<Self>, downloads: JoinHandle<bool>) {
        let this = Arc::clone(self);
        tokio::spawn(async move {
            if downloads.await.unwrap_or(false) {
                this.rebuild().await;
            }
        });
    }

    /// Builds the engine again from the files, after downloads finished.
    async fn rebuild(&self) {
        let version = self.version.load(std::sync::atomic::Ordering::SeqCst);
        let settings = self.settings();
        let mut state = self.state.lock().await;
        if version != state.version {
            // The settings changed meanwhile; the next ask builds for them.
            return;
        }
        state.engine = self.build(&settings).await;
        self.live.set(state.engine.clone());
    }

    fn path_of(&self, id: &str) -> PathBuf {
        let name: String = id
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        self.dir.join(format!("{name}.txt"))
    }

    /// Downloads `lists` side by side; each success replaces the file of its list, each failure is
    /// remembered and leaves the last good copy. Ends with whether any copy is new.
    fn download_all(self: &Arc<Self>, lists: Vec<ActiveList>) -> JoinHandle<bool> {
        let this = Arc::clone(self);
        let count = lists.len();
        this.downloading
            .fetch_add(count, std::sync::atomic::Ordering::SeqCst);
        tokio::spawn(async move {
            let downloads = lists.into_iter().map(|list| {
                let this = Arc::clone(&this);
                async move {
                    let result = match require_https(&list.url) {
                        Ok(()) => this.fetcher.get(&list.url).await.and_then(looks_like_list),
                        Err(error) => Err(error),
                    };
                    let result = match result {
                        Ok(text) => this.store(&list.id, &text).await,
                        Err(error) => Err(error),
                    };
                    let mut errors = this.errors.lock().unwrap_or_else(PoisonError::into_inner);
                    match result {
                        Ok(()) => {
                            errors.remove(&list.id);
                            true
                        }
                        Err(error) => {
                            log::warn!("agent browser: filter list {}: {error}", list.id);
                            errors.insert(list.id, error);
                            false
                        }
                    }
                }
            });
            let stored = futures::future::join_all(downloads)
                .await
                .into_iter()
                .any(|stored| stored);
            this.downloading
                .fetch_sub(count, std::sync::atomic::Ordering::SeqCst);
            stored
        })
    }

    /// Writes the list to a temporary file of its own (another holzi on this device may store the
    /// same list) and moves it over the old copy, so the copy is either the old or the new one.
    async fn store(&self, id: &str, text: &str) -> Result<(), String> {
        tokio::fs::create_dir_all(&self.dir)
            .await
            .map_err(|error| error.to_string())?;
        let path = self.path_of(id);
        let temp = path.with_extension(format!("{}.part", uuid::Uuid::new_v4()));
        let written = match tokio::fs::write(&temp, text).await {
            Ok(()) => tokio::fs::rename(&temp, &path).await,
            Err(error) => Err(error),
        };
        if written.is_err() {
            let _ = tokio::fs::remove_file(&temp).await;
        }
        written.map_err(|error| error.to_string())
    }

    /// Compiles the lists on disk that are switched on; `None` when none is there.
    async fn build(&self, settings: &TrackerSettings) -> Option<Arc<TrackerFilter>> {
        let paths: Vec<PathBuf> = settings
            .active()
            .iter()
            .map(|list| self.path_of(&list.id))
            .collect();
        tokio::task::spawn_blocking(move || {
            let texts: Vec<String> = paths
                .iter()
                .filter_map(|path| std::fs::read_to_string(path).ok())
                .collect();
            (!texts.is_empty()).then(|| Arc::new(TrackerFilter::new(texts)))
        })
        .await
        .ok()
        .flatten()
    }
}

/// How old the file at `path` is; `None` when there is none.
fn age(path: &Path) -> Option<Duration> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    Some(
        SystemTime::now()
            .duration_since(modified)
            .unwrap_or_default(),
    )
}

/// The filter lists of this device as Tauri state, for the settings commands; `None` where they
/// could not be set up.
pub struct TrackerLists(pub Option<Arc<FilterLists>>);

impl TrackerLists {
    pub fn get(&self) -> crate::error::Result<&Arc<FilterLists>> {
        self.0
            .as_ref()
            .ok_or_else(|| crate::error::HolziError::InvalidInput {
                reason: "the tracker filters are not available on this device".to_owned(),
            })
    }
}
