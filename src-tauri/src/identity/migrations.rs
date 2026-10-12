//! holzi-owned migrations shipped via `MigrationSource`.
//!
//! Maintainability exception (spaex 500-LoC rule): migration SQL and its
//! versioned ordering are intentionally co-located so the schema history stays
//! reviewable in one place. The next migration batch should extract each table
//! family into `identity_migrations.rs` and `conversation_migrations.rs`, while
//! retaining this file as the ordered `MigrationSource`.
//!
//! Contract: `tauri-commands.md` §"holzi-owned data layer". Slices a and b
//! land seven tables: identity (`vault_identity`, `known_devices`) plus the
//! Etappe-2 tables (`providers`, `models`, `chat_threads`, `chat_messages`,
//! `device_downloaded_models_no_sync`) and their supporting indexes.
//!
//! CRDT-tracked tables (no `_no_sync` suffix) get the three metadata
//! columns (`haex_hlc_no_sync`, `haex_column_hlcs_no_sync`,
//! `haex_column_sigs_no_sync`) added by haex-crdt's `CrdtTransformer` at
//! migration time, and the triggers installed by
//! `ensure_triggers_initialized` inside `Database::open`. holzi does NOT
//! call `install_crdt` (Etappe-0 finding: the transformer + trigger
//! installer cover every non-`_no_sync` *table* automatically).
//!
//! That automatic coverage stops at the table level. Each trigger bakes
//! its tracked-*column* list in at creation time, and the installer
//! early-returns on a vault whose stored trigger version is already
//! current — so a migration that adds a column to an existing tracked
//! table must bump [`HOLZI_TRIGGER_VERSION`] as well, or the new column
//! stays untracked on every already-provisioned vault.
//!
//! `device_downloaded_models_no_sync` has the `_no_sync` suffix on the
//! table name so haex-crdt skips it entirely — downloaded GGUF files are
//! per-installation state (their absolute path lives under this host's
//! `AppLocalData/models/`) and must not sync.

use std::collections::BTreeMap;
use std::sync::Arc;

use haex_crdt::{MigrationName, StaticMigrationSource};

use super::migrations_agent_files::AGENT_FILES_0029;
use super::migrations_encrypted_folders::ENCRYPTED_FOLDERS_0030;
use super::migrations_extensions::DEV_EXTENSION_KV_0024;
use super::migrations_extensions::EXTENSIONS_0023;
use super::migrations_extensions::SYNC_PARKING_0025;
use super::migrations_passwords::PASSWORDS_0022;
use super::migrations_passwords_owner::PASSWORDS_OWNER_0027;
use super::migrations_passwords_refs::PASSWORDS_REFS_0026;
use super::migrations_storage::{STORAGE_0028, STORAGE_PROTOCOLS_0031};

/// CRDT trigger-schema version holzi installs on open.
///
/// haex-crdt bakes the tracked-column list into each table's
/// `AFTER UPDATE OF <cols>` trigger when the trigger is created, and
/// `ensure_triggers_initialized` skips the rewrite entirely once the
/// vault's stored version is >= the one passed in. A migration that adds
/// a column to a CRDT-tracked table therefore leaves that column
/// untracked on every already-provisioned vault — writes to it neither
/// mark the row dirty nor record a per-column HLC, so the value never
/// reaches another device.
///
/// Bump this whenever a migration alters the column shape of a
/// CRDT-tracked (non-`_no_sync`) table, so `Database::open` recreates
/// the triggers against the migrated schema.
///
/// - 1: `DEFAULT_TRIGGER_VERSION`, migrations 0001-0008.
/// - 2: `0009_models_add_tokenizer_repo` added a column to `models`.
/// - 3: `0010_providers_add_adapter` added a column to `providers`.
/// - 4: `0011_preferences` introduced a new CRDT-tracked table whose
///   `AFTER UPDATE OF <cols>` trigger must be created on every existing
///   vault, not just genesis ones.
/// - 5: `0013_chat_messages_add_idempotency_key` added a column to
///   `chat_messages`.
/// - 6: `0014_chat_messages_tool_columns` added columns to `chat_messages`.
/// - 7: `0015_models_add_huggingface_source` added columns to `models`.
/// - 8: `0016_providers_add_capability` added a column to `providers`.
/// - 9: `0017_chat_messages_add_autonomy_mode` added a column to
///   `chat_messages`.
/// - 10: `0018_models_add_capabilities` added a column to `models`.
/// - 11: `0019_shell_layout` introduced three new CRDT-tracked tables.
/// - 12: `0021_own_device_sync` rebuilt `vault_identity` and introduced four
///   new CRDT-tracked tables.
/// - 13: no migration; haex-crdt's triggers changed (haexmas/haex-crdt#37,
///   #38): BLOB values and keys keep their type, and the update trigger
///   stamps rows written before the HLC existed.
/// - 14: `0022_passwords` (spec 034) introduced twelve new CRDT-tracked tables.
/// - 15: `0023_extensions` (spec 017) introduced nine new CRDT-tracked tables.
/// - 16: `0026_passwords_refs` (spec 036) introduced the CRDT-tracked
///   `haex_passwords_passkey_links`.
/// - 17: `0027_passwords_owner` (spec 038) added `owner` to `haex_passwords_item_details`.
/// - 18: `0028_storage_connections` (spec 038) introduced two new CRDT-tracked tables.
/// - 19: `0029_agent_file_permissions` (spec 044) introduced the CRDT-tracked
///   `agent_file_permissions`.
/// - 20: `0031_storage_protocols` (spec 050) added `protocol`, `host_key` and
///   `host_key_confirmed_at` to `haex_storage_connections`.
pub const HOLZI_TRIGGER_VERSION: i32 = 20;

/// Returns the frozen holzi migration set at the pinned haex-crdt revision.
pub fn holzi_migration_source() -> Arc<StaticMigrationSource> {
    let mut m: BTreeMap<MigrationName, String> = BTreeMap::new();

    // vault_identity — singleton row inserted at Genesis by the bootstrap
    // hook. Never mutated after Genesis; copied along with the .db, so
    // every replica of the same vault shares the row by construction.
    m.insert(
        MigrationName::from("0001_vault_identity"),
        "CREATE TABLE vault_identity (\
            id INTEGER PRIMARY KEY CHECK (id = 1), \
            pubkey BLOB NOT NULL, \
            privkey BLOB NOT NULL\
         );"
        .to_string(),
    );

    // known_devices — one row per (vault × installation). `installation_uuid`
    // is the immutable primary key carried in CRDT `row_pks`; the further
    // columns are plain CRDT-tracked fields. See contract §"Vault identity
    // and device model" for why installation_uuid is a PK, not a mutable
    // column update.
    m.insert(
        MigrationName::from("0002_known_devices"),
        "CREATE TABLE known_devices (\
            installation_uuid TEXT PRIMARY KEY NOT NULL, \
            vault_device_uuid TEXT NOT NULL UNIQUE, \
            alias TEXT, \
            first_seen INTEGER NOT NULL\
         );"
        .to_string(),
    );

    // providers — one row per configured provider. `kind` decides how the
    // row is used: `local` = mistralrs GGUF (credentials NULL, base_url
    // NULL), `api_key` = external HTTP API (credentials required),
    // `cli_delegate` = spawn an external CLI (credentials NULL, base_url
    // holds the CLI binary name or path). Credentials sync per operator
    // decision 2026-09-08 (plan §"Anbietermodelle").
    m.insert(
        MigrationName::from("0003_providers"),
        "CREATE TABLE providers (\
            id TEXT PRIMARY KEY NOT NULL, \
            kind TEXT NOT NULL, \
            name TEXT NOT NULL, \
            base_url TEXT, \
            credentials BLOB, \
            created_at INTEGER NOT NULL\
         );"
        .to_string(),
    );

    // models — cache of the models each provider exposes. For local
    // providers, one row per catalog entry the user has picked or
    // downloaded. `fetched_at` is the last catalog/API refresh in epoch
    // milliseconds; NULL for entries seeded from the built-in catalog
    // before any refresh has happened. `context_window` is best-effort
    // metadata for the pre-flight length check (plan §"Anbietermodelle":
    // "Überschreitet der Verlauf das Kontextfenster … scheitert die
    // Anfrage").
    m.insert(
        MigrationName::from("0004_models"),
        "CREATE TABLE models (\
            id TEXT PRIMARY KEY NOT NULL, \
            provider_id TEXT NOT NULL, \
            name TEXT NOT NULL, \
            context_window INTEGER, \
            fetched_at INTEGER\
         );"
        .to_string(),
    );

    // chat_threads — one row per conversation. `last_provider_id` /
    // `last_model_id` record what was most recently used so a new
    // message can prefill the selector; individual messages carry their
    // own provider/model (plan §"Anbietermodelle": mid-conversation
    // model switch keeps per-message attribution).
    m.insert(
        MigrationName::from("0005_chat_threads"),
        "CREATE TABLE chat_threads (\
            id TEXT PRIMARY KEY NOT NULL, \
            title TEXT NOT NULL, \
            last_provider_id TEXT, \
            last_model_id TEXT, \
            created_at INTEGER NOT NULL, \
            updated_at INTEGER NOT NULL\
         );"
        .to_string(),
    );

    // chat_messages — one row per finalised message. `parent_id` tracks
    // conversation branching (plan §"Datenmodell": Elternbezüge erhalten
    // Verzweigungen). `finish_reason` is one of 'complete' | 'cancelled'
    // | 'error'; in-flight generations are NOT stored here — they live
    // in `app_settings_no_sync` (later slice) and only land here on
    // finalisation.
    m.insert(
        MigrationName::from("0006_chat_messages"),
        "CREATE TABLE chat_messages (\
            id TEXT PRIMARY KEY NOT NULL, \
            thread_id TEXT NOT NULL, \
            parent_id TEXT, \
            role TEXT NOT NULL, \
            content TEXT NOT NULL, \
            provider_id TEXT, \
            model_id TEXT, \
            prompt_tokens INTEGER, \
            completion_tokens INTEGER, \
            finish_reason TEXT, \
            created_at INTEGER NOT NULL\
         );"
        .to_string(),
    );

    // device_downloaded_models_no_sync — this installation's GGUF file
    // registry. NOT synced (plan §"Datenmodell": local file paths, and
    // model bytes stay under `AppLocalData/models/`, never in Git or in
    // the sync payload). `id` matches `models.id` for the corresponding
    // catalog entry. `relative_path` is relative to
    // `AppLocalData/models/` so the same row is portable across the
    // sandboxing rules of desktop, Android and iOS.
    m.insert(
        MigrationName::from("0007_device_downloaded_models_no_sync"),
        "CREATE TABLE device_downloaded_models_no_sync (\
            id TEXT PRIMARY KEY NOT NULL, \
            relative_path TEXT NOT NULL, \
            size_bytes INTEGER NOT NULL, \
            sha256 TEXT, \
            verified_at INTEGER NOT NULL\
         );"
        .to_string(),
    );

    // list_messages and chat history assembly filter by thread and order by
    // creation time. Keep the lookup bounded as a thread grows. This is a
    // new migration so the already-issued 0007 migration remains immutable.
    m.insert(
        MigrationName::from("0008_chat_messages_thread_index"),
        "CREATE INDEX idx_chat_messages_thread \
         ON chat_messages (thread_id, created_at, id);"
            .to_string(),
    );

    // `load_local_model` needs a tokenizer repo to feed mistralrs. Until
    // this migration it was looked up from the built-in catalog only,
    // which meant arbitrary HuggingFace downloads (via
    // `download_model_from_hf` with a caller-supplied `tokenizer_repo`)
    // could not be loaded after a restart — the value was thrown away.
    // Column is NULL for api_key/cli_delegate rows (they do not tokenize
    // locally) and is set on download / import going forward. Pre-existing
    // catalog rows are back-filled lazily by `models::commands` on the
    // next `list_installed_models` call.
    m.insert(
        MigrationName::from("0009_models_add_tokenizer_repo"),
        "ALTER TABLE models ADD COLUMN tokenizer_repo TEXT;".to_string(),
    );

    // API-key providers must retain the adapter/vendor they belong to. A
    // nullable value keeps older rows readable while refresh rejects those
    // rows until they are explicitly migrated to a supported adapter.
    m.insert(
        MigrationName::from("0010_providers_add_adapter"),
        "ALTER TABLE providers ADD COLUMN adapter TEXT;".to_string(),
    );

    // Preferences — namespaced key/value entries per-device or vault-wide.
    // Composite PK on `(vault_device_uuid, key)` with a hard FK to
    // `known_devices(vault_device_uuid) ON DELETE CASCADE` implements the
    // ADR-0001 convention: retiring a device automatically cleans up its
    // preference rows without any Retire-command needing to know about
    // this table. The nil UUID is the vault-scope sentinel — bootstrap
    // ensures the sentinel row exists in `known_devices` so vault-wide
    // preferences can reference it. Multi-statement via the drizzle
    // breakpoint so the index lands with the table under one migration.
    m.insert(
        MigrationName::from("0011_preferences"),
        "CREATE TABLE preferences (\
            vault_device_uuid TEXT NOT NULL \
              REFERENCES known_devices(vault_device_uuid) ON DELETE CASCADE, \
            key TEXT NOT NULL, \
            value TEXT, \
            PRIMARY KEY (vault_device_uuid, key)\
         );\n\
         --> statement-breakpoint\n\
         CREATE INDEX idx_preferences_key ON preferences (key, vault_device_uuid);"
            .to_string(),
    );

    // Retires `device_downloaded_models_no_sync`. The filesystem under
    // `<AppLocalData>/models/<slug>/<file.gguf>` is now the authority
    // for "installed on this device" (spec 002 §"Installed model",
    // data-model.md, research.md §Entscheidung 4). Callers move to the
    // filesystem-scan path in `list_installed_models` and the shared
    // `models::paths::canonical_model_file` selector before this
    // migration removes the table.
    m.insert(
        MigrationName::from("0012_drop_device_downloaded_models_no_sync"),
        "DROP TABLE device_downloaded_models_no_sync;".to_string(),
    );

    // idempotency_key — set only on the user-message row of a
    // `send_message` call (contract §send_message). The partial unique
    // index allows unlimited NULLs (assistant rows, and every
    // pre-0013 message) while still rejecting two user messages that
    // reuse the same key.
    m.insert(
        MigrationName::from("0013_chat_messages_add_idempotency_key"),
        "ALTER TABLE chat_messages ADD COLUMN idempotency_key TEXT;\n\
         --> statement-breakpoint\n\
         CREATE UNIQUE INDEX idx_chat_messages_idempotency_key \
         ON chat_messages (idempotency_key) WHERE idempotency_key IS NOT NULL;"
            .to_string(),
    );

    // Agent tool loop (feature 003): a `tool_call` row carries `tool_name`,
    // `tool_call_id`, `tool_input` and `tool_source`; its paired
    // `tool_result` row carries `tool_call_id` and `tool_is_error`. All
    // five are nullable — `user`/`assistant`/`system` rows leave them NULL
    // (data-model.md). Bumps HOLZI_TRIGGER_VERSION to 6.
    m.insert(
        MigrationName::from("0014_chat_messages_tool_columns"),
        "ALTER TABLE chat_messages ADD COLUMN tool_name TEXT;\n\
         --> statement-breakpoint\n\
         ALTER TABLE chat_messages ADD COLUMN tool_call_id TEXT;\n\
         --> statement-breakpoint\n\
         ALTER TABLE chat_messages ADD COLUMN tool_input TEXT;\n\
         --> statement-breakpoint\n\
         ALTER TABLE chat_messages ADD COLUMN tool_is_error INTEGER;\n\
         --> statement-breakpoint\n\
         ALTER TABLE chat_messages ADD COLUMN tool_source TEXT;"
            .to_string(),
    );

    // Free HuggingFace model discovery (spec 005): additive source and
    // integrity columns on the shared `models` row. `hf_revision` is the
    // commit SHA resolved and used for the install; `hf_revision_ref` is
    // the optional mutable branch/tag tracked for later update checks —
    // both NULL for catalog/imported/provider rows. `file_sha256` is the
    // full-file hash written only after a successful atomic publish;
    // `load_model` recomputes and compares it before every local runtime
    // load (research.md Entscheidung 6). `integrity_status` defaults to
    // `unknown` for every pre-existing row — they predate hashing and have
    // no `file_sha256` to verify against until their next
    // download/update/import. `source_kind` defaults to `provider`; the
    // existing `backfill_tokenizer_repo` idiom in `list_installed_models`
    // gets a `backfill_source_kind` sibling that reclassifies local-provider
    // rows into `catalog`/`imported` from the compiled catalog id list,
    // since a SQL-only migration cannot see that list. Bumps
    // HOLZI_TRIGGER_VERSION to 7.
    m.insert(
        MigrationName::from("0015_models_add_huggingface_source"),
        "ALTER TABLE models ADD COLUMN hf_repo TEXT;\n\
         --> statement-breakpoint\n\
         ALTER TABLE models ADD COLUMN hf_filename TEXT;\n\
         --> statement-breakpoint\n\
         ALTER TABLE models ADD COLUMN hf_revision TEXT;\n\
         --> statement-breakpoint\n\
         ALTER TABLE models ADD COLUMN hf_revision_ref TEXT;\n\
         --> statement-breakpoint\n\
         ALTER TABLE models ADD COLUMN file_sha256 TEXT;\n\
         --> statement-breakpoint\n\
         ALTER TABLE models ADD COLUMN integrity_status TEXT NOT NULL DEFAULT 'unknown';\n\
         --> statement-breakpoint\n\
         ALTER TABLE models ADD COLUMN source_kind TEXT NOT NULL DEFAULT 'provider';"
            .to_string(),
    );

    // Voice control (spec 008): a provider row now serves either the `chat`
    // capability (existing rows) or the new `transcription` capability (the
    // bundled local Whisper adapter, and optionally a user-configured
    // external STT service). `find_local_provider` narrows on this column
    // too (providers/local.rs) — without it, adding the local transcription
    // row would make that lookup ambiguous between the two `kind = 'local'`
    // rows. Bumps HOLZI_TRIGGER_VERSION to 8.
    m.insert(
        MigrationName::from("0016_providers_add_capability"),
        "ALTER TABLE providers ADD COLUMN capability TEXT NOT NULL DEFAULT 'chat';".to_string(),
    );

    // Autonomous Delegate Mode (spec 009): a `chat_messages` row now records
    // which `AutonomyMode` its turn ran under — `standard`/`ungated`/
    // `gated_permissive`, or `NULL` for legacy and non-delegate rows
    // (data-model.md Decision B). Bumps HOLZI_TRIGGER_VERSION to 9.
    m.insert(
        MigrationName::from("0017_chat_messages_add_autonomy_mode"),
        "ALTER TABLE chat_messages ADD COLUMN autonomy_mode TEXT;".to_string(),
    );

    // Unified model capabilities (spec 012): one JSON `ModelCapabilities`
    // record per model (reasoning control, accepted attachment kinds, an
    // adapter hint). `NULL` means "not determined yet" — never "unsupported"
    // — so pre-existing rows need no backfill here; provider rows are filled
    // by their next refresh and local rows by `backfill_local_capabilities`.
    // Bumps HOLZI_TRIGGER_VERSION to 10.
    m.insert(
        MigrationName::from("0018_models_add_capabilities"),
        "ALTER TABLE models ADD COLUMN capabilities_json TEXT;".to_string(),
    );

    // Workspace-Shell (spec 015): `workspaces` (no name column — the UI
    // shows "Workspace N" from `position`, FR-019), `shell_windows` and
    // `shell_window_tabs`. Single-column foreign keys target `workspaces
    // (workspace_id)` / `shell_windows(window_id)` via their own `UNIQUE`
    // constraint (research.md R2, confirmed by the CRDT-transformer table-
    // constraint pass-through check in `migrations_tests.rs`), so deleting a
    // workspace or window cascades to its windows/tabs at the database
    // level (I7). All three tables also carry the ADR-0001 FK to
    // `known_devices(vault_device_uuid) ON DELETE CASCADE`. Bumps
    // HOLZI_TRIGGER_VERSION to 11.
    m.insert(
        MigrationName::from("0019_shell_layout"),
        "CREATE TABLE workspaces (\
            vault_device_uuid TEXT NOT NULL \
              REFERENCES known_devices(vault_device_uuid) ON DELETE CASCADE, \
            workspace_id TEXT NOT NULL UNIQUE, \
            position INTEGER NOT NULL, \
            PRIMARY KEY (vault_device_uuid, workspace_id)\
         );\n\
         --> statement-breakpoint\n\
         CREATE INDEX idx_workspaces_device_position \
         ON workspaces (vault_device_uuid, position);\n\
         --> statement-breakpoint\n\
         CREATE TABLE shell_windows (\
            vault_device_uuid TEXT NOT NULL \
              REFERENCES known_devices(vault_device_uuid) ON DELETE CASCADE, \
            window_id TEXT NOT NULL UNIQUE, \
            workspace_id TEXT NOT NULL \
              REFERENCES workspaces(workspace_id) ON DELETE CASCADE, \
            x INTEGER NOT NULL, \
            y INTEGER NOT NULL, \
            width INTEGER NOT NULL, \
            height INTEGER NOT NULL, \
            is_minimized INTEGER NOT NULL DEFAULT 0, \
            is_maximized INTEGER NOT NULL DEFAULT 0, \
            stack_order INTEGER NOT NULL, \
            active_tab_id TEXT NOT NULL, \
            PRIMARY KEY (vault_device_uuid, window_id)\
         );\n\
         --> statement-breakpoint\n\
         CREATE INDEX idx_shell_windows_device_workspace \
         ON shell_windows (vault_device_uuid, workspace_id);\n\
         --> statement-breakpoint\n\
         CREATE TABLE shell_window_tabs (\
            vault_device_uuid TEXT NOT NULL \
              REFERENCES known_devices(vault_device_uuid) ON DELETE CASCADE, \
            tab_id TEXT NOT NULL, \
            window_id TEXT NOT NULL \
              REFERENCES shell_windows(window_id) ON DELETE CASCADE, \
            app_id TEXT NOT NULL, \
            position INTEGER NOT NULL, \
            PRIMARY KEY (vault_device_uuid, tab_id)\
         );\n\
         --> statement-breakpoint\n\
         CREATE INDEX idx_shell_window_tabs_device_window_position \
         ON shell_window_tabs (vault_device_uuid, window_id, position);"
            .to_string(),
    );

    // Opt-in session restore (spec 022-session-restore, research R5/R6).
    // Drops the three spec 015 tables, children first: `DROP TABLE`'s
    // implicit delete fires no triggers, so no delete markers reach other
    // devices, and dropping a child before its parent keeps the parent's
    // drop from cascading into a CRDT table (a `DELETE` on a CRDT table
    // fails here — migrations run before the HLC is initialized). Saved
    // sessions move to the device-local `wm_sessions_no_sync` (one JSON
    // row per device; `_no_sync` keeps it out of sync entirely). The
    // maintenance row asks the first open to `VACUUM` the dropped
    // content out of the file's free pages. No `HOLZI_TRIGGER_VERSION`
    // bump: dropping a CRDT table and adding `_no_sync` tables install no
    // triggers.
    m.insert(
        MigrationName::from("0020_wm_session_no_sync"),
        "DROP TABLE shell_window_tabs;\n\
         --> statement-breakpoint\n\
         DROP TABLE shell_windows;\n\
         --> statement-breakpoint\n\
         DROP TABLE workspaces;\n\
         --> statement-breakpoint\n\
         CREATE TABLE wm_sessions_no_sync (\
            vault_device_uuid TEXT PRIMARY KEY NOT NULL, \
            session_json TEXT NOT NULL, \
            updated_at TEXT NOT NULL\
         );\n\
         --> statement-breakpoint\n\
         CREATE TABLE holzi_maintenance_no_sync (\
            task TEXT PRIMARY KEY NOT NULL\
         );\n\
         --> statement-breakpoint\n\
         INSERT INTO holzi_maintenance_no_sync (task) \
         VALUES ('vacuum_after_legacy_wm_drop');"
            .to_string(),
    );

    // Own-device sync (spec 024, data-model.md, research R2/R3). The
    // placeholder secret of `vault_identity` moves to the device-local
    // `vault_identity_secret_no_sync` as the seed the real identity is derived
    // from after open (`sync::keys::ensure_vault_identity`); it is copied,
    // never regenerated. `vault_identity` is then rebuilt without `privkey`:
    // its placeholder row never had an HLC and never synced, and `DROP TABLE`
    // fires no triggers, so no delete marker reaches other devices (as in
    // 0020). The rebuilt table stays empty until the identity is published
    // with its first HLC. The other tables are new. Bumps
    // HOLZI_TRIGGER_VERSION to 12: the rebuilt and the four new CRDT tables
    // need their triggers on every existing vault.
    m.insert(
        MigrationName::from("0021_own_device_sync"),
        "CREATE TABLE vault_identity_secret_no_sync (\
            id INTEGER PRIMARY KEY CHECK (id = 1), \
            privkey BLOB NOT NULL CHECK (length(privkey) = 32)\
         );\n\
         --> statement-breakpoint\n\
         INSERT OR IGNORE INTO vault_identity_secret_no_sync (id, privkey) \
         SELECT id, privkey FROM vault_identity WHERE id = 1 AND length(privkey) = 32;\n\
         --> statement-breakpoint\n\
         DROP TABLE vault_identity;\n\
         --> statement-breakpoint\n\
         CREATE TABLE vault_identity (\
            id INTEGER PRIMARY KEY CHECK (id = 1), \
            pubkey BLOB NOT NULL\
         );\n\
         --> statement-breakpoint\n\
         CREATE TABLE device_keys_no_sync (\
            installation_uuid TEXT PRIMARY KEY NOT NULL, \
            device_secret BLOB NOT NULL CHECK (length(device_secret) = 32), \
            device_pubkey BLOB NOT NULL CHECK (length(device_pubkey) = 32), \
            endpoint_secret BLOB NOT NULL CHECK (length(endpoint_secret) = 32), \
            endpoint_id BLOB NOT NULL CHECK (length(endpoint_id) = 32), \
            created_at INTEGER NOT NULL\
         );\n\
         --> statement-breakpoint\n\
         CREATE TABLE device_lists (\
            list_hash BLOB PRIMARY KEY NOT NULL, \
            generation INTEGER NOT NULL, \
            payload BLOB NOT NULL, \
            signature BLOB NOT NULL\
         );\n\
         --> statement-breakpoint\n\
         CREATE TABLE vault_key_generations (\
            key_id BLOB PRIMARY KEY NOT NULL, \
            scope TEXT NOT NULL, \
            generation INTEGER NOT NULL, \
            created_by BLOB NOT NULL, \
            created_at INTEGER NOT NULL, \
            device_list_hash BLOB NOT NULL, \
            authorization BLOB NOT NULL\
         );\n\
         --> statement-breakpoint\n\
         CREATE TABLE vault_key_envelopes (\
            key_id BLOB NOT NULL, \
            recipient BLOB NOT NULL, \
            sender BLOB NOT NULL, \
            envelope TEXT NOT NULL, \
            device_list_hash BLOB NOT NULL, \
            authorization BLOB NOT NULL, \
            PRIMARY KEY (key_id, recipient)\
         );\n\
         --> statement-breakpoint\n\
         CREATE TABLE vault_content_keys_no_sync (\
            key_id BLOB PRIMARY KEY NOT NULL, \
            generation INTEGER NOT NULL, \
            key BLOB NOT NULL CHECK (length(key) = 32)\
         );\n\
         --> statement-breakpoint\n\
         CREATE TABLE sync_progress_no_sync (\
            origin TEXT PRIMARY KEY NOT NULL, \
            max_hlc TEXT NOT NULL\
         );\n\
         --> statement-breakpoint\n\
         CREATE TABLE pending_links_no_sync (\
            link_id BLOB PRIMARY KEY NOT NULL, \
            peer_device_pubkey BLOB NOT NULL, \
            new_list_hash BLOB NOT NULL, \
            new_list_payload BLOB NOT NULL, \
            new_list_signature BLOB NOT NULL, \
            role TEXT NOT NULL CHECK (role IN ('main', 'linked')), \
            resume_secret BLOB NOT NULL, \
            state TEXT NOT NULL \
              CHECK (state IN ('transferring', 'awaiting_publication', 'completed')), \
            created_at INTEGER NOT NULL\
         );\n\
         --> statement-breakpoint\n\
         CREATE TABLE admission_requests (\
            device_pubkey BLOB PRIMARY KEY NOT NULL, \
            vault_device_uuid TEXT NOT NULL, \
            endpoint_id BLOB NOT NULL, \
            name TEXT NOT NULL, \
            requested_at INTEGER NOT NULL, \
            signature BLOB NOT NULL, \
            state TEXT NOT NULL\
         );\n\
         --> statement-breakpoint\n\
         CREATE TABLE device_presence_no_sync (\
            device_pubkey BLOB PRIMARY KEY NOT NULL, \
            last_seen INTEGER NOT NULL, \
            endpoint_addr BLOB, \
            problem TEXT CHECK (problem IN ('incompatible_version', 'duplicate'))\
         );"
        .to_string(),
    );

    m.insert(
        MigrationName::from("0022_passwords"),
        PASSWORDS_0022.to_owned(),
    );
    m.insert(
        MigrationName::from("0023_extensions"),
        EXTENSIONS_0023.to_owned(),
    );
    m.insert(
        MigrationName::from("0025_sync_parking"),
        SYNC_PARKING_0025.to_owned(),
    );
    m.insert(
        MigrationName::from("0024_dev_extension_kv"),
        DEV_EXTENSION_KV_0024.to_owned(),
    );
    m.insert(
        MigrationName::from("0026_passwords_refs"),
        PASSWORDS_REFS_0026.to_owned(),
    );
    m.insert(
        MigrationName::from("0027_passwords_owner"),
        PASSWORDS_OWNER_0027.to_owned(),
    );
    m.insert(
        MigrationName::from("0028_storage_connections"),
        STORAGE_0028.to_owned(),
    );
    m.insert(
        MigrationName::from("0029_agent_file_permissions"),
        AGENT_FILES_0029.to_owned(),
    );
    m.insert(
        MigrationName::from("0030_encrypted_folders"),
        ENCRYPTED_FOLDERS_0030.to_owned(),
    );
    m.insert(
        MigrationName::from("0031_storage_protocols"),
        STORAGE_PROTOCOLS_0031.to_owned(),
    );

    Arc::new(StaticMigrationSource(m))
}
