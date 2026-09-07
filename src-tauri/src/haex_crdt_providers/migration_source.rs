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
use std::path::{Path, PathBuf};

use haex_crdt::error::{Error as CrdtError, MigrationJournal, Result as CrdtResult};
use haex_crdt::{MigrationName, MigrationSource};
use serde::Deserialize;

/// In-memory index of every consumer-owned migration, keyed by
/// [`MigrationName`] so `list_migrations` walks them in the required
/// lexicographic order without an extra sort.
pub struct HaexVaultMigrationSource {
    migrations: BTreeMap<MigrationName, String>,
}

/// Drizzle-style journal entry. Public within the module — both the
/// test constructor and the production `from_migrations_dir` share the
/// parser.
#[derive(Debug, Deserialize)]
struct JournalEntry {
    idx: u32,
    tag: String,
}

#[derive(Debug, Deserialize)]
struct JournalFile {
    entries: Vec<JournalEntry>,
}

impl HaexVaultMigrationSource {
    /// Production constructor. Reads the drizzle + manual migrations from
    /// a caller-supplied directory that must contain a `src-tauri`-style
    /// layout: `<manifest_root>/database/migrations/meta/_journal.json`
    /// (required) and `<manifest_root>/database/migrations-manual/_journal.json`
    /// (optional).
    ///
    /// Batch 5 wires this into `AppState` construction — for desktop the
    /// path is resolved from `tauri::AppHandle::path_resolver()`, for
    /// Android from the FS plugin's resource loader. Both flows land here
    /// once the bytes are on disk. The `#[cfg(test)]` `from_embedded`
    /// convenience below uses `CARGO_MANIFEST_DIR` so unit tests don't
    /// need any `AppHandle`.
    pub fn from_migrations_dir(manifest_root: PathBuf) -> CrdtResult<Self> {
        Self::from_manifest_root(&manifest_root)
    }

    /// Test-only convenience. Resolves `CARGO_MANIFEST_DIR` and delegates
    /// to the same parser production uses — so a green
    /// `from_embedded()` test proves the shipped parser works against the
    /// shipped fixture, not a test-only branch.
    #[cfg(test)]
    pub fn from_embedded() -> CrdtResult<Self> {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        Self::from_manifest_root(&root)
    }

    /// Build from `<manifest_root>/database/{migrations,migrations-manual}`.
    /// Shared by every constructor so the trait behaviour under test is
    /// exactly what production runs.
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
