//! `haex-crdt` [`MigrationSource`] adapter.
//!
//! haex-vault ships two migration streams:
//!
//! 1. Drizzle-generated migrations under `src-tauri/database/migrations/`,
//!    enumerated by `meta/_journal.json`.
//! 2. Hand-maintained manual migrations under
//!    `src-tauri/database/migrations-manual/`, enumerated by their own
//!    `_journal.json`. Convention: manual tags carry a `manual_` prefix so
//!    they cannot collide with drizzle tags in the by-name applied
//!    tracking.
//!
//! Both streams together form the consumer-owned migration journal that
//! `haex-crdt` reconciles against `haex_app_migrations`. Order of insertion
//! into the returned map is irrelevant — the trait contract requires
//! lexicographic total order via `MigrationName: Ord`, and `BTreeMap`
//! provides that already.
//!
//! # Retro-worthy friction
//!
//! Production paths currently read migrations via `tauri_plugin_fs::FsExt`
//! (needed on Android where resources live inside the APK). The test path
//! reads directly from `CARGO_MANIFEST_DIR` since unit tests have no
//! `AppHandle`. The two loaders share the same journal parser and the
//! same file-lookup shape — so a shipped drift is caught by the
//! integration path, not the unit tests, but the trait behaviour is
//! covered here.

use std::collections::BTreeMap;
#[cfg(test)]
use std::path::{Path, PathBuf};

use haex_crdt::error::{Error as CrdtError, MigrationJournal, Result as CrdtResult};
use haex_crdt::{MigrationName, MigrationSource};
#[cfg(test)]
use serde::Deserialize;

/// In-memory index of every consumer-owned migration, keyed by
/// [`MigrationName`] so `list_migrations` walks them in the required
/// lexicographic order without an extra sort.
pub struct HaexVaultMigrationSource {
    migrations: BTreeMap<MigrationName, String>,
}

/// Drizzle-style journal entry. Test-only for now; Batch 5 will re-open
/// this to production once the `AppHandle`-backed loader lands.
#[cfg(test)]
#[derive(Debug, Deserialize)]
struct JournalEntry {
    idx: u32,
    tag: String,
}

#[cfg(test)]
#[derive(Debug, Deserialize)]
struct JournalFile {
    entries: Vec<JournalEntry>,
}

impl HaexVaultMigrationSource {
    /// Test-only constructor. Reads the drizzle + manual migrations from
    /// disk relative to `CARGO_MANIFEST_DIR` — no `AppHandle`, no Tauri
    /// resource plumbing required.
    ///
    /// This is named `from_embedded` for symmetry with the integration
    /// plan's illustrative test; the migrations are read at unit-test
    /// runtime, not literally compiled in. That difference is intentional:
    /// embedding every SQL file via `include_str!` would require a
    /// hard-coded list that will drift as new migrations land, whereas
    /// reading from disk stays in lockstep with the shipped fixture.
    #[cfg(test)]
    pub fn from_embedded() -> CrdtResult<Self> {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        Self::from_manifest_root(&root)
    }

    /// Build from `<repo>/src-tauri/database/{migrations,migrations-manual}`.
    /// Split out so both the test constructor and any future
    /// `from_app_handle` variant can share the parser. Batch 5 will lift
    /// the `#[cfg(test)]` guards on this helper stack when it adds the
    /// production constructor.
    #[cfg(test)]
    fn from_manifest_root(manifest_root: &Path) -> CrdtResult<Self> {
        let drizzle_dir = manifest_root.join("database").join("migrations");
        let manual_dir = manifest_root.join("database").join("migrations-manual");

        let mut migrations: BTreeMap<MigrationName, String> = BTreeMap::new();

        // Drizzle stream — required.
        let drizzle_journal = drizzle_dir.join("meta").join("_journal.json");
        for name in read_journal(&drizzle_journal)? {
            let sql_path = drizzle_dir.join(format!("{name}.sql"));
            let sql = read_sql_file(&sql_path)?;
            migrations.insert(MigrationName::from(name), sql);
        }

        // Manual stream — optional. Absent journal is fine, matches the
        // graceful degradation the drizzle loader already provides.
        let manual_journal = manual_dir.join("_journal.json");
        if manual_journal.exists() {
            for name in read_journal(&manual_journal)? {
                let sql_path = manual_dir.join(format!("{name}.sql"));
                let sql = read_sql_file(&sql_path)?;
                migrations.insert(MigrationName::from(name), sql);
            }
        }

        Ok(Self { migrations })
    }
}

#[cfg(test)]
fn read_journal(path: &Path) -> CrdtResult<Vec<String>> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        CrdtError::Message(format!("read migration journal {}: {e}", path.display()))
    })?;
    let mut journal: JournalFile = serde_json::from_str(&content).map_err(|e| {
        CrdtError::Message(format!("parse migration journal {}: {e}", path.display()))
    })?;
    journal.entries.sort_by_key(|e| e.idx);
    Ok(journal.entries.into_iter().map(|e| e.tag).collect())
}

#[cfg(test)]
fn read_sql_file(path: &Path) -> CrdtResult<String> {
    std::fs::read_to_string(path)
        .map_err(|e| CrdtError::Message(format!("read migration sql {}: {e}", path.display())))
}

impl MigrationSource for HaexVaultMigrationSource {
    fn load_migration(&self, name: &MigrationName) -> CrdtResult<String> {
        self.migrations
            .get(name)
            .cloned()
            .ok_or_else(|| CrdtError::MigrationMissingFromSource {
                journal: MigrationJournal::ConsumerOwned,
                name: name.as_str().to_string(),
            })
    }

    fn list_migrations(&self) -> CrdtResult<Vec<MigrationName>> {
        Ok(self.migrations.keys().cloned().collect())
    }
}
