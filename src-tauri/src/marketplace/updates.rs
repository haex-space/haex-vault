//! Known updates (spec 069 US3, FR-017, research R8): which installed extension has a newer
//! compatible version in which marketplace, as far as the last catalog page or the last check saw.
//! Nothing here asks a marketplace by itself: the catalog app asks when it opens and on
//! "Aktualisieren", and the settings list only shows what was found.
//!
//! A candidate is kept per `(installed name, marketplace, slug)`. An answer changes only the
//! candidates of its own marketplace and of the entries it answered for, so a filtered catalog
//! page, a failed marketplace or a marketplace that did not know a name erases nothing else.

use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

use futures::future::join_all;
use semver::Version;
use tokio::sync::Semaphore;

use super::catalog::{search, InstalledExtension, Source};
use super::client::Http;
use super::model::{CatalogEntry, CheckUpdatesResult, KnownUpdate, KnownUpdatesView, SourceError};

/// The most update searches in flight at once, over all names and marketplaces.
pub const MAX_PARALLEL: usize = 4;

type Key = (String, String, String);

fn key_of(update: &KnownUpdate) -> Key {
    (
        update.installed_name.clone(),
        update.source_id.clone(),
        update.slug.clone(),
    )
}

/// The update an entry of an answer stands for: the installed extension of its name and the newer
/// compatible version the entry offers. `None` when nothing is installed under the name or
/// nothing newer fits.
pub fn update_of(entry: &CatalogEntry) -> Option<KnownUpdate> {
    let installed = entry.installed.as_ref()?;
    Some(KnownUpdate {
        installed_name: entry.name.clone(),
        installed: installed.version.clone(),
        latest_compatible: entry.update_to.clone()?,
        source_id: entry.source.id.clone(),
        slug: entry.slug.clone(),
    })
}

/// What the host remembers between two checks.
#[derive(Default)]
pub struct KnownUpdates {
    found: Mutex<BTreeMap<Key, KnownUpdate>>,
}

impl KnownUpdates {
    fn lock(&self) -> MutexGuard<'_, BTreeMap<Key, KnownUpdate>> {
        self.found.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// A catalog page was answered: for each installed entry on it the candidate is set when a
    /// newer compatible version exists and removed when none does. Entries that are not on the
    /// page keep what is known.
    pub fn record_page(&self, entries: &[CatalogEntry]) {
        let mut found = self.lock();
        for entry in entries.iter().filter(|entry| entry.installed.is_some()) {
            let key = (
                entry.name.clone(),
                entry.source.id.clone(),
                entry.slug.clone(),
            );
            match update_of(entry) {
                Some(update) => {
                    found.insert(key, update);
                }
                None => {
                    found.remove(&key);
                }
            }
        }
    }

    /// `source_id` answered the search for `name`: its candidates for that name become exactly
    /// `updates`. The candidates of other marketplaces and of other names stay.
    pub fn replace_for(&self, name: &str, source_id: &str, updates: Vec<KnownUpdate>) {
        let mut found = self.lock();
        found.retain(|(known_name, known_source, _), _| {
            known_name != name || known_source != source_id
        });
        for update in updates {
            found.insert(key_of(&update), update);
        }
    }

    /// Every candidate, in a fixed order.
    pub fn candidates(&self) -> Vec<KnownUpdate> {
        self.lock().values().cloned().collect()
    }
}

fn parse(text: &str) -> Option<Version> {
    Version::parse(text.trim()).ok()
}

/// The candidates as the settings list shows them. A candidate counts only while its extension is
/// installed, its marketplace is switched on and its version is still newer than the installed
/// one (the extension may have been updated since); it is shown with the installed version of
/// now. `enabled` is the order of the marketplaces that are on, first rank first. Per installed
/// name the highest compatible version is selected and, on a tie, the marketplace of the better
/// rank.
pub fn view(
    candidates: Vec<KnownUpdate>,
    installed: &[InstalledExtension],
    enabled: &[String],
) -> KnownUpdatesView {
    let rank = |source_id: &str| enabled.iter().position(|id| id == source_id);
    let current: Vec<KnownUpdate> = candidates
        .into_iter()
        .filter_map(|mut candidate| {
            rank(&candidate.source_id)?;
            let now = installed
                .iter()
                .find(|extension| extension.name == candidate.installed_name)?
                .version
                .clone()?;
            let newer = parse(&candidate.latest_compatible)? > parse(&now)?;
            candidate.installed = now;
            newer.then_some(candidate)
        })
        .collect();

    let mut selected: BTreeMap<String, KnownUpdate> = BTreeMap::new();
    for candidate in &current {
        let better = match selected.get(&candidate.installed_name) {
            None => true,
            Some(best) => {
                let (new, old) = (
                    parse(&candidate.latest_compatible),
                    parse(&best.latest_compatible),
                );
                new > old || (new == old && rank(&candidate.source_id) < rank(&best.source_id))
            }
        };
        if better {
            selected.insert(candidate.installed_name.clone(), candidate.clone());
        }
    }
    KnownUpdatesView {
        candidates: current,
        selected: selected.into_values().collect(),
    }
}

/// What one run of the check found, per installed name and marketplace that answered, and the
/// marketplaces that did not.
#[derive(Debug, Default)]
pub struct Checked {
    /// `(installed name, marketplace id, its updates for that name)`.
    pub answered: Vec<(String, String, Vec<KnownUpdate>)>,
    pub source_errors: Vec<SourceError>,
}

/// Asks every marketplace in `sources` for every installed extension with a search by its name
/// (research R8) and keeps the entries whose name equals the installed name exactly. At most
/// [`MAX_PARALLEL`] searches run at once, each within its own time limit. A marketplace that
/// cannot be asked or fails is one error line and answers for none of the names, so nothing it
/// was known to offer is forgotten. Without an installed extension or a marketplace nothing is
/// asked.
pub async fn check_updates(
    http: Http,
    sources: &[Source],
    installed: &[InstalledExtension],
    app: &Version,
) -> Checked {
    let names: Vec<&InstalledExtension> = installed
        .iter()
        .filter(|extension| extension.version.is_some())
        .collect();
    let mut checked = Checked::default();
    let mut jobs = Vec::new();
    for source in sources {
        if let Some(kind) = source.unavailable {
            if !names.is_empty() {
                checked
                    .source_errors
                    .push(source.error(super::client::SourceFailure { kind, detail: None }));
            }
            continue;
        }
        for extension in &names {
            jobs.push((source, *extension));
        }
    }

    // Every search waits for a place, so at most `MAX_PARALLEL` are in flight whatever the number
    // of names and marketplaces.
    let places = Semaphore::new(MAX_PARALLEL);
    let places = &places;
    let answers = join_all(jobs.into_iter().map(|(source, extension)| async move {
        let _place = places.acquire().await.ok();
        let entries = search(http, source, &extension.name, installed, app).await;
        (source, extension, entries)
    }))
    .await;

    let mut failed: Vec<&str> = Vec::new();
    for (source, extension, entries) in answers {
        match entries {
            Ok(entries) => {
                let updates = entries
                    .iter()
                    .filter(|entry| entry.name == extension.name)
                    .filter_map(update_of)
                    .collect();
                checked
                    .answered
                    .push((extension.name.clone(), source.id.clone(), updates));
            }
            Err(failure) => {
                if !failed.contains(&source.id.as_str()) {
                    failed.push(&source.id);
                    checked.source_errors.push(source.error(failure));
                }
            }
        }
    }
    checked
}

impl Checked {
    /// The result a command reports: the updates this run found and the marketplaces that failed.
    pub fn result(&self) -> CheckUpdatesResult {
        CheckUpdatesResult {
            updates: self
                .answered
                .iter()
                .flat_map(|(_, _, updates)| updates.iter().cloned())
                .collect(),
            source_errors: self.source_errors.clone(),
        }
    }
}

#[cfg(test)]
#[path = "updates_tests.rs"]
mod tests;
