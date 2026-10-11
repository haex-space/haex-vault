//! SQL of migration `0030_encrypted_folders` (spec 048, data-model.md, research R6, R8): what this
//! device has read of encrypted folders, so a second opening needs only a listing (SC-003). Both
//! tables are caches each device rebuilds from the bucket, so they end in `_no_sync`, get no CRDT
//! triggers and need no new [`super::migrations::HOLZI_TRIGGER_VERSION`]. Folder keys and file keys
//! never land here; `wrapped_dek` stays wrapped under the folder key.

/// The migration `0030_encrypted_folders`; registered in
/// [`super::migrations::holzi_migration_source`].
pub const ENCRYPTED_FOLDERS_0030: &str = r#"CREATE TABLE encrypted_folder_heads_no_sync (
  storage_id TEXT NOT NULL,
  object_key TEXT NOT NULL,
  etag TEXT NOT NULL,
  folder_id TEXT NOT NULL,
  name TEXT,
  state TEXT NOT NULL,
  key_id BLOB,
  open_ack INTEGER NOT NULL,
  loaded_at INTEGER NOT NULL,
  PRIMARY KEY (storage_id, object_key)
);
--> statement-breakpoint
CREATE TABLE encrypted_folder_entries_no_sync (
  storage_id TEXT NOT NULL,
  folder_id TEXT NOT NULL,
  sidecar TEXT NOT NULL,
  etag TEXT NOT NULL,
  entry_id TEXT NOT NULL,
  parent_id TEXT NOT NULL,
  name TEXT NOT NULL,
  kind TEXT NOT NULL,
  revision INTEGER NOT NULL,
  base TEXT,
  written INTEGER NOT NULL,
  size INTEGER,
  modified INTEGER,
  content_type TEXT,
  sha256 TEXT,
  object_id TEXT,
  content_id TEXT,
  wrapped_dek BLOB,
  PRIMARY KEY (storage_id, folder_id, sidecar)
);
--> statement-breakpoint
CREATE INDEX idx_encrypted_folder_entries_parent
  ON encrypted_folder_entries_no_sync(storage_id, folder_id, parent_id);"#;

/// The tables of [`ENCRYPTED_FOLDERS_0030`]; both stay on this device.
pub const DEVICE_TABLES: [&str; 2] = [
    "encrypted_folder_heads_no_sync",
    "encrypted_folder_entries_no_sync",
];
