//! SQL of migration `0069_agent_memory` (spec 097, data-model.md): the pages of the shared memory
//! of the agents and two columns of a conversation.
//!
//! `haex_agent_memory_pages` belongs to the vault, not to a device (FR-001), so it carries no
//! `vault_device_uuid`; `author_device` and `changed_device` only say where a page was written. It
//! has no UNIQUE constraint besides its primary key (a conflict would halt the sync): two devices
//! can give two pages the same title, and the store resolves a title to the one changed last.
//!
//! `chat_threads.memory_toc` holds the table of contents as it was when the conversation's first
//! turn started (FR-011); `memory_foreign_at` marks when the conversation first saw foreign
//! content (FR-019). Both are written once and synced with the thread.
//!
//! A new CRDT table and new columns of a tracked one raise
//! [`super::migrations::HAEX_TRIGGER_VERSION`].

/// The migration `0069_agent_memory`; registered in [`super::migrations::haex_migration_source`].
pub const AGENT_MEMORY_0069: &str = r#"CREATE TABLE haex_agent_memory_pages (
  id TEXT PRIMARY KEY NOT NULL,
  title TEXT NOT NULL,
  description TEXT NOT NULL,
  body TEXT NOT NULL,
  author_kind TEXT NOT NULL,
  author_key TEXT,
  author_name TEXT NOT NULL,
  author_device TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  changed_kind TEXT,
  changed_key TEXT,
  changed_name TEXT,
  changed_device TEXT,
  changed_at INTEGER
);
--> statement-breakpoint
ALTER TABLE chat_threads ADD COLUMN memory_toc TEXT;
--> statement-breakpoint
ALTER TABLE chat_threads ADD COLUMN memory_foreign_at INTEGER;"#;
