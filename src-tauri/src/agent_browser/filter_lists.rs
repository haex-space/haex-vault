//! The filter lists of the agent browser's tracker blocking (spec 065 FR-045, research R16): the
//! default lists of uBlock Origin and the user's own, downloaded from their publishers (never
//! shipped), kept as files on this device without sync, refreshed after seven days and compiled
//! into one [`TrackerFilter`]. A list that fails keeps its last good copy.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant, SystemTime};

use serde::{Deserialize, Serialize};
use tokio::task::JoinHandle;

use super::filter_fetch::{looks_like_list, require_https, ListFetcher};
use super::filters::TrackerFilter;

/// A list is downloaded again after this long.
pub const REFRESH_AFTER: Duration = Duration::from_secs(7 * 24 * 60 * 60);
/// How long the first start waits for lists it has never had.
pub const FIRST_WAIT: Duration = Duration::from_secs(10);
/// How often a running holzi looks at the age of its lists again.
const RECHECK: Duration = Duration::from_secs(60 * 60);

/// A list holzi knows by itself.
pub struct DefaultList {
    pub id: &'static str,
    pub name: &'static str,
    pub url: &'static str,
}

/// What uBlock Origin enables out of the box (its `assets.json`).
pub const DEFAULT_LISTS: [DefaultList; 9] = [
    DefaultList {
        id: "ublock-filters",
        name: "uBlock filters – Ads",
        url: "https://ublockorigin.github.io/uAssets/filters/filters.txt",
    },
    DefaultList {
        id: "ublock-badware",
        name: "uBlock filters – Badware risks",
        url: "https://ublockorigin.github.io/uAssets/filters/badware.txt",
    },
    DefaultList {
        id: "ublock-privacy",
        name: "uBlock filters – Privacy",
        url: "https://ublockorigin.github.io/uAssets/filters/privacy.txt",
    },
    DefaultList {
        id: "ublock-unbreak",
        name: "uBlock filters – Unbreak",
        url: "https://ublockorigin.github.io/uAssets/filters/unbreak.txt",
    },
    DefaultList {
        id: "ublock-quick-fixes",
        name: "uBlock filters – Quick fixes",
        url: "https://ublockorigin.github.io/uAssets/filters/quick-fixes.txt",
    },
    DefaultList {
        id: "easylist",
        name: "EasyList",
        url: "https://ublockorigin.github.io/uAssets/thirdparties/easylist.txt",
    },
    DefaultList {
        id: "easyprivacy",
        name: "EasyPrivacy",
        url: "https://ublockorigin.github.io/uAssets/thirdparties/easyprivacy.txt",
    },
    DefaultList {
        id: "urlhaus",
        name: "Online Malicious URL Blocklist",
        url: "https://malware-filter.gitlab.io/urlhaus-filter/urlhaus-filter-vivaldi-online.txt",
    },
    DefaultList {
        id: "peter-lowe",
        name: "Peter Lowe's Ad and tracking server list",
        url: "https://pgl.yoyo.org/adservers/serverlist.php?hostformat=adblockplus&showintro=1&mimetype=plaintext",
    },
];

fn yes() -> bool {
    true
}

/// A list the user added by its address.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomList {
    pub id: String,
    pub name: String,
    pub url: String,
    #[serde(default = "yes")]
    pub enabled: bool,
}

/// What the user chose: blocking as a whole, the default lists switched off, and own lists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackerSettings {
    #[serde(default = "yes")]
    pub enabled: bool,
    /// The ids of the default lists switched off.
    #[serde(default)]
    pub off: BTreeSet<String>,
    #[serde(default)]
    pub custom: Vec<CustomList>,
}

impl Default for TrackerSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            off: BTreeSet::new(),
            custom: Vec::new(),
        }
    }
}

/// One list that counts, with where it comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveList {
    pub id: String,
    pub url: String,
}

impl TrackerSettings {
    /// The lists switched on, defaults first.
    pub fn active(&self) -> Vec<ActiveList> {
        let defaults = DEFAULT_LISTS
            .iter()
            .filter(|list| !self.off.contains(list.id))
            .map(|list| ActiveList {
                id: list.id.to_owned(),
                url: list.url.to_owned(),
            });
        let custom = self
            .custom
            .iter()
            .filter(|list| list.enabled)
            .map(|list| ActiveList {
                id: list.id.clone(),
                url: list.url.clone(),
            });
        defaults.chain(custom).collect()
    }
}

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
    errors: Mutex<BTreeMap<String, String>>,
}

impl FilterLists {
    pub fn new(dir: PathBuf, fetcher: Arc<dyn ListFetcher>) -> Arc<Self> {
        Arc::new(Self {
            dir,
            fetcher,
            settings: Mutex::default(),
            version: std::sync::atomic::AtomicU64::new(1),
            state: tokio::sync::Mutex::default(),
            errors: Mutex::default(),
        })
    }

    pub fn settings(&self) -> TrackerSettings {
        self.settings
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
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

    /// The engine over the lists that are switched on; `None` while blocking is off or no list is
    /// there yet. Lists never had are waited for up to [`FIRST_WAIT`]; lists older than
    /// [`REFRESH_AFTER`] are used as they are and downloaded again meanwhile.
    pub async fn engine(self: &Arc<Self>) -> Option<Arc<TrackerFilter>> {
        let settings = self.settings();
        if !settings.enabled {
            return None;
        }
        let version = self.version.load(std::sync::atomic::Ordering::SeqCst);
        let mut state = self.state.lock().await;
        if state.version == version && state.checked.is_some_and(|at| at.elapsed() < RECHECK) {
            return state.engine.clone();
        }
        let (mut missing, mut stale) = (Vec::new(), Vec::new());
        for list in settings.active() {
            match age(&self.path_of(&list.id)) {
                None => missing.push(list),
                Some(age) if age > REFRESH_AFTER => stale.push(list),
                Some(_) => {}
            }
        }
        let mut first = self.download_all(missing);
        let waited = tokio::time::timeout(FIRST_WAIT, &mut first).await.is_ok();
        let engine = self.build(&settings).await;
        state.engine = engine.clone();
        state.version = version;
        state.checked = Some(Instant::now());
        drop(state);
        if !waited || !stale.is_empty() {
            let (this, later) = (Arc::clone(self), self.download_all(stale));
            tokio::spawn(async move {
                if !waited {
                    let _ = first.await;
                }
                let _ = later.await;
                this.rebuild().await;
            });
        }
        engine
    }

    /// Builds the engine again from the files, after downloads finished.
    async fn rebuild(&self) {
        let settings = self.settings();
        let version = self.version.load(std::sync::atomic::Ordering::SeqCst);
        let mut state = self.state.lock().await;
        if version != state.version {
            // The settings changed meanwhile; the next ask builds for them.
            return;
        }
        state.engine = self.build(&settings).await;
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
    /// remembered and leaves the last good copy.
    fn download_all(self: &Arc<Self>, lists: Vec<ActiveList>) -> JoinHandle<()> {
        let this = Arc::clone(self);
        tokio::spawn(async move {
            let downloads = lists.into_iter().map(|list| {
                let this = Arc::clone(&this);
                async move {
                    let result = match require_https(&list.url) {
                        Ok(()) => this.fetcher.get(&list.url).await.and_then(looks_like_list),
                        Err(error) => Err(error),
                    };
                    let result = result.and_then(|text| this.store(&list.id, &text));
                    let mut errors = this.errors.lock().unwrap_or_else(PoisonError::into_inner);
                    match result {
                        Ok(()) => drop(errors.remove(&list.id)),
                        Err(error) => {
                            log::warn!("agent browser: filter list {}: {error}", list.id);
                            errors.insert(list.id, error);
                        }
                    }
                }
            });
            futures::future::join_all(downloads).await;
        })
    }

    /// Writes the list to a temporary file and moves it over the old copy.
    fn store(&self, id: &str, text: &str) -> Result<(), String> {
        std::fs::create_dir_all(&self.dir).map_err(|error| error.to_string())?;
        let path = self.path_of(id);
        let temp = path.with_extension("part");
        std::fs::write(&temp, text).map_err(|error| error.to_string())?;
        std::fs::rename(&temp, &path).map_err(|error| error.to_string())
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
