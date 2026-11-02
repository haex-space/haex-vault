//! Old journal names of migrations that parallel branches renumbered.
//!
//! A vault opened by such a branch before the renumbering keeps the old name in its journal, and
//! haex-crdt refuses to open it when the source lacks that name. The SQL is unchanged, so a source
//! that carries the old name in place of the new one opens the vault without running anything
//! twice.

use std::sync::Arc;

use haex_crdt::{MigrationName, StaticMigrationSource};

use super::migrations::holzi_migration_source;

/// A migration whose journal name changed after it had been applied on a branch.
struct Renamed {
    legacy: &'static str,
    current: &'static str,
    /// Whether the current name stays in the source next to the old one. Only for SQL that can
    /// safely run a second time; a migration that adds a column must not.
    keep_current: bool,
}

const RENAMED: [Renamed; 3] = [
    // The chat-message tool-origin migration landed first and took `0035`.
    Renamed {
        legacy: "0035_file_icon_sets",
        current: "0036_file_icon_sets",
        keep_current: false,
    },
    // The peer schema migration took `0040`; the `UPDATE` may run again.
    Renamed {
        legacy: "0040_contacts_card_display_name",
        current: "0041_contacts_card_display_name",
        keep_current: true,
    },
    // The MCP agents migration took `0045` (spec 021) while spec 061 PR B was open.
    Renamed {
        legacy: "0045_sync_rules_decrypt",
        current: "0046_sync_rules_decrypt",
        keep_current: false,
    },
];

/// Whether `name` is the old journal name of a renumbered migration.
pub fn is_legacy_migration_name(name: &str) -> bool {
    RENAMED.iter().any(|renamed| renamed.legacy == name)
}

/// Returns the current migration set with each given old journal name in place of its new one.
///
/// Names that are not old names of a renumbered migration are ignored.
pub fn holzi_migration_source_with_legacy_names(legacy: &[String]) -> Arc<StaticMigrationSource> {
    let mut migrations = holzi_migration_source().0.clone();
    for renamed in RENAMED
        .iter()
        .filter(|renamed| legacy.iter().any(|name| name == renamed.legacy))
    {
        let current = MigrationName::from(renamed.current);
        let sql = if renamed.keep_current {
            migrations.get(&current).cloned()
        } else {
            migrations.remove(&current)
        }
        .unwrap_or_else(|| panic!("current migration source contains {}", renamed.current));
        migrations.insert(MigrationName::from(renamed.legacy), sql);
    }

    Arc::new(StaticMigrationSource(migrations))
}
