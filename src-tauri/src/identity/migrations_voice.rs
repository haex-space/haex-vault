//! SQL of migration `0050_voice_conversations` (spec 066, data-model.md, T039, T124): spoken
//! conversations with the built-in agent.
//!
//! `chat_threads` gains `origin`: `NULL` for a typed conversation, `'voice'` for one started by
//! speaking. `voice_transcript_entries` keeps everything said in a spoken conversation, in both
//! directions (FR-047, FR-048); `chat_messages` stays what the model sees.
//!
//! Each local write numbers its entry `MAX(seq) + 1` for the thread inside the same transaction.
//! Devices writing at the same time may produce the same `seq`, so the synced table has no UNIQUE
//! constraint on `(thread_id, seq)` (a conflict would halt the sync, as in
//! [`super::migrations_contacts`]); readers order by `(thread_id, seq, id)`. There is no cascading
//! foreign key: deleting a thread deletes its entries explicitly in the same CRDT write so each gets
//! its own delete marker. The new column and table raise
//! [`super::migrations::HOLZI_TRIGGER_VERSION`].

/// The migration `0050_voice_conversations`; registered in
/// [`super::migrations::holzi_migration_source`].
pub const VOICE_0050: &str = r#"ALTER TABLE chat_threads ADD COLUMN origin TEXT;
--> statement-breakpoint
CREATE TABLE voice_transcript_entries (
  id TEXT PRIMARY KEY NOT NULL,
  thread_id TEXT NOT NULL,
  seq INTEGER NOT NULL,
  spoken_at INTEGER NOT NULL,
  speaker TEXT NOT NULL,
  kind TEXT NOT NULL,
  text TEXT NOT NULL,
  spoken_until INTEGER,
  outcome TEXT,
  ref_id TEXT,
  created_at INTEGER NOT NULL
);
--> statement-breakpoint
CREATE INDEX voice_transcript_entries_thread ON voice_transcript_entries (thread_id, seq, id);"#;
