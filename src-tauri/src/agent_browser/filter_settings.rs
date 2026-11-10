//! What the user chose for the tracker blocking (spec 065 FR-045, research R16) and the lists
//! holzi knows by itself: the default lists of uBlock Origin and the user's own, by their `https`
//! address. The choice is kept in the vault (`choices`), not here.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use ts_rs::TS;

use super::filter_fetch::require_https;

/// The most lists of one's own.
const MAX_CUSTOM: usize = 50;
/// The longest address of a list.
const MAX_URL: usize = 2048;
/// The longest name of a list.
const MAX_NAME: usize = 100;
/// The longest id of a list.
const MAX_ID: usize = 64;

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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct CustomList {
    /// Empty for a list the user just added; holzi gives it one.
    #[serde(default)]
    pub id: String,
    /// Empty takes the host of the address.
    #[serde(default)]
    pub name: String,
    pub url: String,
    #[serde(default = "yes")]
    pub enabled: bool,
}

/// What the user chose: blocking as a whole, the default lists switched off, and own lists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
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

impl TrackerSettings {
    /// The choice as it is kept: addresses must be `https`, a list without id gets one, a list
    /// without name is named by its host, and the ids of default lists that do not exist go.
    pub fn validated(mut self) -> Result<Self, String> {
        if self.custom.len() > MAX_CUSTOM {
            return Err(format!("at most {MAX_CUSTOM} lists of your own"));
        }
        self.off
            .retain(|id| DEFAULT_LISTS.iter().any(|list| list.id == id));
        let mut taken: BTreeSet<String> = DEFAULT_LISTS
            .iter()
            .map(|list| list.id.to_owned())
            .collect();
        for list in &mut self.custom {
            list.url = list.url.trim().to_owned();
            if list.url.len() > MAX_URL {
                return Err("the address of a list is too long".to_owned());
            }
            require_https(&list.url)?;
            list.name = list.name.trim().chars().take(MAX_NAME).collect();
            if list.name.is_empty() {
                list.name = url::Url::parse(&list.url)
                    .ok()
                    .and_then(|url| url.host_str().map(str::to_owned))
                    .unwrap_or_else(|| list.url.clone());
            }
            if list.id.is_empty() {
                list.id = uuid::Uuid::new_v4().to_string();
            }
            // The id names the file of the list (anything else there becomes `_`, so two ids could
            // share one file): only what names a file as it is.
            if list.id.len() > MAX_ID
                || !list
                    .id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-')
            {
                return Err("the id of a list may only hold letters, digits and dashes".to_owned());
            }
            if !taken.insert(list.id.clone()) {
                return Err("two lists have the same id".to_owned());
            }
        }
        Ok(self)
    }
}

/// One list as "KI → Browser" shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct TrackerListView {
    pub id: String,
    pub name: String,
    pub url: String,
    /// One of the user's own; a default list cannot be removed.
    pub custom: bool,
    pub enabled: bool,
    /// When its file was last stored, in milliseconds since 1970; `None` while there is none.
    #[ts(optional, type = "number | null")]
    pub fetched_at: Option<i64>,
    /// Why the last download failed.
    #[ts(optional = nullable)]
    pub error: Option<String>,
}

/// The state of the tracker blocking for "KI → Browser" (FR-045).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct TrackerOverview {
    /// Blocking as a whole.
    pub enabled: bool,
    pub lists: Vec<TrackerListView>,
    /// Lists that are switched on are still downloading for the first time; nothing they would
    /// block is blocked until they are there.
    pub loading: bool,
}
