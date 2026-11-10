//! SQL of migration `0051_assistant_tones` (spec 066 FR-045a, data-model.md "Eigene Töne", T156):
//! the user's own tones for the voice assistant. One row is one tone: its prepared sound (16-bit
//! mono WAV, silence trimmed, peak matched to the built-in tones) is the BLOB of that row, so the
//! sync brings a tone whole or not at all.
//!
//! Vault-wide and synced, so a tone imported once can be chosen on every device; which tone sounds
//! for which occasion is the device preference `assistant.tones.custom`. Like the icon sets
//! ([`super::migrations_icon_sets`]) the table carries no UNIQUE constraint (a conflict would halt
//! the sync) and a row never changes: replacing a tone is a new row and the delete of the old. The
//! new CRDT table raises [`super::migrations::HOLZI_TRIGGER_VERSION`].

/// The migration `0051_assistant_tones`; registered in
/// [`super::migrations::holzi_migration_source`].
pub const ASSISTANT_TONES_0051: &str = r#"CREATE TABLE haex_assistant_tones (
  id TEXT PRIMARY KEY NOT NULL,
  name TEXT NOT NULL,
  duration_ms INTEGER NOT NULL,
  sample_rate INTEGER NOT NULL,
  sha256 TEXT NOT NULL,
  sound BLOB NOT NULL,
  created_at INTEGER NOT NULL
);"#;
