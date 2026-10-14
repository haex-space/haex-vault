//! SQL of migration `0033_contacts` (spec 051, data-model.md): the address book. Contacts and
//! their values, confirmed holzi addresses from business cards, groups, photos, sync targets, the
//! per-phone mapping to the phone's address book (ADR 0001: synced, keyed by `vault_device_uuid`),
//! open phone issues, the state of each phone's sync, the own business card and blocked vaults.
//!
//! All tables are CRDT-tracked (no `_no_sync` suffix): haex-crdt adds the HLC columns and the
//! triggers. Every table has a primary key, none is `WITHOUT ROWID`, and none has a UNIQUE
//! constraint, which would halt the sync on a conflict; uniqueness comes from derived ids
//! (`contacts::ids`) and from natural primary keys (`vault_pubkey`, `hash`). New CRDT tables raise
//! [`super::migrations::HOLZI_TRIGGER_VERSION`].

/// The migration `0033_contacts`; registered in [`super::migrations::holzi_migration_source`].
pub const CONTACTS_0033: &str = r#"CREATE TABLE haex_contacts (
  id TEXT PRIMARY KEY NOT NULL,
  display_name TEXT,
  given_name TEXT,
  family_name TEXT,
  middle_name TEXT,
  name_prefix TEXT,
  name_suffix TEXT,
  nickname TEXT,
  organization TEXT,
  job_title TEXT,
  note TEXT,
  photo TEXT,
  origin_kind TEXT NOT NULL,
  origin_device TEXT NOT NULL,
  origin_caller TEXT NOT NULL,
  origin_account TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
--> statement-breakpoint
CREATE TABLE haex_contacts_values (
  id TEXT PRIMARY KEY NOT NULL,
  contact_id TEXT NOT NULL REFERENCES haex_contacts(id) ON DELETE CASCADE,
  kind TEXT NOT NULL,
  label TEXT NOT NULL,
  value TEXT NOT NULL,
  position INTEGER NOT NULL
);
--> statement-breakpoint
CREATE INDEX haex_contacts_values_contact ON haex_contacts_values(contact_id);
--> statement-breakpoint
CREATE TABLE haex_contacts_cards (
  vault_pubkey TEXT PRIMARY KEY NOT NULL,
  contact_id TEXT NOT NULL REFERENCES haex_contacts(id) ON DELETE CASCADE,
  card BLOB NOT NULL,
  issued_at TEXT NOT NULL,
  received_at TEXT NOT NULL
);
--> statement-breakpoint
CREATE TABLE haex_contacts_groups (
  id TEXT PRIMARY KEY NOT NULL,
  name TEXT NOT NULL
);
--> statement-breakpoint
CREATE TABLE haex_contacts_group_members (
  id TEXT PRIMARY KEY NOT NULL,
  group_id TEXT NOT NULL REFERENCES haex_contacts_groups(id) ON DELETE CASCADE,
  contact_id TEXT NOT NULL REFERENCES haex_contacts(id) ON DELETE CASCADE
);
--> statement-breakpoint
CREATE TABLE haex_contacts_binaries (
  hash TEXT PRIMARY KEY NOT NULL,
  data BLOB NOT NULL,
  orphaned_at TEXT
);
--> statement-breakpoint
CREATE TABLE haex_contacts_targets (
  id TEXT PRIMARY KEY NOT NULL,
  contact_id TEXT NOT NULL REFERENCES haex_contacts(id) ON DELETE CASCADE,
  vault_device_uuid TEXT NOT NULL
    REFERENCES known_devices(vault_device_uuid) ON DELETE CASCADE,
  account_type TEXT NOT NULL,
  account_name TEXT NOT NULL
);
--> statement-breakpoint
CREATE TABLE haex_contacts_phone_links (
  id TEXT PRIMARY KEY NOT NULL,
  vault_device_uuid TEXT NOT NULL
    REFERENCES known_devices(vault_device_uuid) ON DELETE CASCADE,
  contact_id TEXT NOT NULL REFERENCES haex_contacts(id) ON DELETE CASCADE,
  raw_contact_id INTEGER NOT NULL,
  lookup_key TEXT NOT NULL,
  account_type TEXT NOT NULL,
  account_name TEXT NOT NULL,
  source_id TEXT,
  version INTEGER NOT NULL,
  read_only INTEGER NOT NULL,
  base TEXT NOT NULL,
  photo_base TEXT
);
--> statement-breakpoint
CREATE TABLE haex_contacts_phone_issues (
  id TEXT PRIMARY KEY NOT NULL,
  vault_device_uuid TEXT NOT NULL
    REFERENCES known_devices(vault_device_uuid) ON DELETE CASCADE,
  contact_id TEXT NOT NULL REFERENCES haex_contacts(id) ON DELETE CASCADE,
  reason TEXT NOT NULL,
  detail TEXT,
  at TEXT NOT NULL
);
--> statement-breakpoint
CREATE TABLE haex_contacts_phone_runs (
  vault_device_uuid TEXT PRIMARY KEY NOT NULL
    REFERENCES known_devices(vault_device_uuid) ON DELETE CASCADE,
  state TEXT NOT NULL,
  paused_reason TEXT,
  last_run_at TEXT,
  counts TEXT
);
--> statement-breakpoint
CREATE TABLE haex_contacts_own_card (
  id INTEGER PRIMARY KEY NOT NULL,
  card BLOB NOT NULL,
  issued_at TEXT NOT NULL
);
--> statement-breakpoint
CREATE TABLE haex_contacts_blocked_vaults (
  vault_pubkey TEXT PRIMARY KEY NOT NULL,
  blocked_at TEXT NOT NULL
);"#;
