//! SQL of migration `0052_tts_voices` (spec 066 FR-049, FR-050, data-model.md "Eigene Stimmen",
//! T145): the user's own voices for the voice assistant. A voice is a reference recording the
//! speech model imitates, like the built-in ones (research R20); its row carries name, language
//! and when the user confirmed the speaker's consent, the recording itself lies once per content
//! in `haex_tts_voices_binaries` (pattern `haex_contacts_binaries`).
//!
//! Vault-wide and synced, so a voice made once can be chosen on every device; which voice a
//! language speaks with stays the device preference `assistant.voice.<sprache>`. No UNIQUE
//! constraint beyond the primary keys (a conflict would halt the sync); a recording no voice uses
//! any more is marked `orphaned_at` and removed after seven days. Imported models (FR-051,
//! `haex_tts_models_no_sync`) come with their own migration in PR K part 2. The new CRDT tables
//! raise [`super::migrations::HOLZI_TRIGGER_VERSION`].

/// The migration `0052_tts_voices`; registered in [`super::migrations::holzi_migration_source`].
pub const TTS_VOICES_0052: &str = r#"CREATE TABLE haex_tts_voices (
  id TEXT PRIMARY KEY NOT NULL,
  name TEXT NOT NULL,
  language TEXT NOT NULL,
  engine TEXT NOT NULL,
  reference_hash TEXT NOT NULL,
  sample_rate INTEGER NOT NULL,
  consent_confirmed_at INTEGER NOT NULL,
  created_at INTEGER NOT NULL
);
--> statement-breakpoint
CREATE TABLE haex_tts_voices_binaries (
  hash TEXT PRIMARY KEY NOT NULL,
  data BLOB NOT NULL,
  orphaned_at TEXT
);"#;
