//! SQL of migration `0022_passwords` (spec 034-password-manager, data-model.md): the twelve
//! `haex_passwords_*` tables of the password manager and their indexes. Table and column names are
//! those of haex-vault 2.x (Legacy); the deviations are A1–A6 of the data model (BLOB data, two columns for the
//! place before the trash, `orphaned_at`, `ON DELETE RESTRICT` for the binary links, and no UNIQUE
//! constraint, because a UNIQUE conflict halts the sync of haex-crdt and the uniqueness of tags, tag
//! links and passkeys comes from derived ids instead, research R2).
//!
//! All tables are CRDT-tracked (no `_no_sync` suffix): haex-crdt adds the HLC columns and the
//! triggers, so none of them is in the SQL. Every table has a primary key and none is
//! `WITHOUT ROWID`. The statements are separated the way the other holzi migrations are
//! (`--> statement-breakpoint`). The `identity/migrations.rs` file is over the 500-line boundary
//! already, so the new SQL lives here and that file only registers it.

/// The migration `0022_passwords`; registered in [`super::migrations::holzi_migration_source`].
pub const PASSWORDS_0022: &str = r#"CREATE TABLE haex_passwords_item_details (
  id TEXT PRIMARY KEY NOT NULL,
  title TEXT,
  username TEXT,
  password TEXT,
  note TEXT,
  icon TEXT,
  color TEXT,
  url TEXT,
  otp_secret TEXT,
  otp_digits INTEGER DEFAULT 6,
  otp_period INTEGER DEFAULT 30,
  otp_algorithm TEXT DEFAULT 'SHA1',
  expires_at TEXT,
  autofill_aliases TEXT,
  created_at TEXT DEFAULT (CURRENT_TIMESTAMP),
  updated_at TEXT DEFAULT (CURRENT_TIMESTAMP)
);
--> statement-breakpoint
CREATE TABLE haex_passwords_item_key_values (
  id TEXT PRIMARY KEY NOT NULL,
  item_id TEXT NOT NULL REFERENCES haex_passwords_item_details(id) ON DELETE CASCADE,
  key TEXT,
  value TEXT,
  updated_at TEXT DEFAULT (CURRENT_TIMESTAMP)
);
--> statement-breakpoint
CREATE TABLE haex_passwords_groups (
  id TEXT PRIMARY KEY NOT NULL,
  name TEXT,
  description TEXT,
  icon TEXT,
  sort_order INTEGER,
  color TEXT,
  parent_id TEXT REFERENCES haex_passwords_groups(id) ON DELETE CASCADE,
  created_at TEXT DEFAULT (CURRENT_TIMESTAMP),
  updated_at TEXT DEFAULT (CURRENT_TIMESTAMP),
  trashed_from_parent_id TEXT
);
--> statement-breakpoint
CREATE TABLE haex_passwords_group_items (
  item_id TEXT PRIMARY KEY NOT NULL REFERENCES haex_passwords_item_details(id) ON DELETE CASCADE,
  group_id TEXT REFERENCES haex_passwords_groups(id) ON DELETE CASCADE,
  trashed_from_group_id TEXT
);
--> statement-breakpoint
CREATE TABLE haex_passwords_binaries (
  hash TEXT PRIMARY KEY NOT NULL,
  data BLOB NOT NULL,
  size INTEGER NOT NULL,
  type TEXT DEFAULT 'attachment',
  created_at TEXT DEFAULT (CURRENT_TIMESTAMP),
  orphaned_at TEXT
);
--> statement-breakpoint
CREATE TABLE haex_passwords_item_binaries (
  id TEXT PRIMARY KEY NOT NULL,
  item_id TEXT NOT NULL REFERENCES haex_passwords_item_details(id) ON DELETE CASCADE,
  binary_hash TEXT NOT NULL REFERENCES haex_passwords_binaries(hash) ON DELETE RESTRICT,
  file_name TEXT NOT NULL
);
--> statement-breakpoint
CREATE TABLE haex_passwords_item_snapshots (
  id TEXT PRIMARY KEY NOT NULL,
  item_id TEXT NOT NULL REFERENCES haex_passwords_item_details(id) ON DELETE CASCADE,
  snapshot_data TEXT NOT NULL,
  created_at TEXT DEFAULT (CURRENT_TIMESTAMP),
  modified_at TEXT
);
--> statement-breakpoint
CREATE TABLE haex_passwords_snapshot_binaries (
  id TEXT PRIMARY KEY NOT NULL,
  snapshot_id TEXT NOT NULL REFERENCES haex_passwords_item_snapshots(id) ON DELETE CASCADE,
  binary_hash TEXT NOT NULL REFERENCES haex_passwords_binaries(hash) ON DELETE RESTRICT,
  file_name TEXT NOT NULL
);
--> statement-breakpoint
CREATE TABLE haex_passwords_generator_presets (
  id TEXT PRIMARY KEY NOT NULL,
  name TEXT NOT NULL,
  length INTEGER NOT NULL DEFAULT 16,
  uppercase INTEGER NOT NULL DEFAULT 1,
  lowercase INTEGER NOT NULL DEFAULT 1,
  numbers INTEGER NOT NULL DEFAULT 1,
  symbols INTEGER NOT NULL DEFAULT 1,
  exclude_chars TEXT DEFAULT '',
  use_pattern INTEGER NOT NULL DEFAULT 0,
  pattern TEXT DEFAULT '',
  is_default INTEGER NOT NULL DEFAULT 0,
  created_at TEXT DEFAULT (CURRENT_TIMESTAMP),
  updated_at TEXT DEFAULT (CURRENT_TIMESTAMP)
);
--> statement-breakpoint
CREATE TABLE haex_passwords_tags (
  id TEXT PRIMARY KEY NOT NULL,
  name TEXT NOT NULL,
  color TEXT,
  created_at TEXT DEFAULT (CURRENT_TIMESTAMP)
);
--> statement-breakpoint
CREATE TABLE haex_passwords_item_tags (
  id TEXT PRIMARY KEY NOT NULL,
  item_id TEXT NOT NULL REFERENCES haex_passwords_item_details(id) ON DELETE CASCADE,
  tag_id TEXT NOT NULL REFERENCES haex_passwords_tags(id) ON DELETE CASCADE
);
--> statement-breakpoint
CREATE TABLE haex_passwords_passkeys (
  id TEXT PRIMARY KEY NOT NULL,
  item_id TEXT REFERENCES haex_passwords_item_details(id) ON DELETE CASCADE,
  credential_id TEXT NOT NULL,
  relying_party_id TEXT NOT NULL,
  relying_party_name TEXT,
  user_name TEXT,
  user_display_name TEXT,
  user_handle TEXT NOT NULL,
  private_key TEXT NOT NULL,
  public_key TEXT NOT NULL,
  algorithm INTEGER NOT NULL DEFAULT -7,
  sign_count INTEGER NOT NULL DEFAULT 0,
  is_discoverable INTEGER NOT NULL DEFAULT 1,
  icon TEXT,
  color TEXT,
  nickname TEXT,
  created_at TEXT DEFAULT (CURRENT_TIMESTAMP),
  last_used_at TEXT
);
--> statement-breakpoint
CREATE INDEX idx_pw_items_updated ON haex_passwords_item_details (updated_at);
--> statement-breakpoint
CREATE INDEX idx_pw_kv_item ON haex_passwords_item_key_values (item_id);
--> statement-breakpoint
CREATE INDEX idx_pw_groups_parent ON haex_passwords_groups (parent_id);
--> statement-breakpoint
CREATE INDEX idx_pw_group_items_group ON haex_passwords_group_items (group_id);
--> statement-breakpoint
CREATE INDEX idx_pw_item_bin_item ON haex_passwords_item_binaries (item_id);
--> statement-breakpoint
CREATE INDEX idx_pw_item_bin_hash ON haex_passwords_item_binaries (binary_hash);
--> statement-breakpoint
CREATE INDEX idx_pw_snap_item ON haex_passwords_item_snapshots (item_id, modified_at);
--> statement-breakpoint
CREATE INDEX idx_pw_snap_bin_snap ON haex_passwords_snapshot_binaries (snapshot_id);
--> statement-breakpoint
CREATE INDEX idx_pw_snap_bin_hash ON haex_passwords_snapshot_binaries (binary_hash);
--> statement-breakpoint
CREATE INDEX idx_pw_tags_name ON haex_passwords_tags (name);
--> statement-breakpoint
CREATE INDEX idx_pw_item_tags_item ON haex_passwords_item_tags (item_id);
--> statement-breakpoint
CREATE INDEX idx_pw_item_tags_tag ON haex_passwords_item_tags (tag_id);
--> statement-breakpoint
CREATE INDEX idx_pw_passkeys_cred ON haex_passwords_passkeys (credential_id);
--> statement-breakpoint
CREATE INDEX idx_pw_passkeys_rp ON haex_passwords_passkeys (relying_party_id);"#;
