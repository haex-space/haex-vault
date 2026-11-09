//! SQL of migration `0058_assistant_speakers` (spec 066 FR-016a, T174): the people the voice
//! assistant listens to. A row is a person's name and their voice print, an embedding of a few
//! utterances read at enrolment, never a recording; the model it was made with is noted, so prints
//! of another model are not compared. Vault-wide and synced: enrolling once is enough for every
//! device. No UNIQUE constraint beyond the primary key (a conflict would halt the sync). The new
//! CRDT table raises [`super::migrations::HOLZI_TRIGGER_VERSION`].

/// The migration `0058_assistant_speakers`; registered in
/// [`super::migrations::holzi_migration_source`].
pub const ASSISTANT_SPEAKERS_0058: &str = r#"CREATE TABLE haex_assistant_speakers (
  id TEXT PRIMARY KEY NOT NULL,
  name TEXT NOT NULL,
  model TEXT NOT NULL,
  embedding BLOB NOT NULL,
  utterances INTEGER NOT NULL,
  created_at INTEGER NOT NULL
);"#;
