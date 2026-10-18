---
description: 'Task list for spec 037-haex-vault-import'
---

# Tasks: Passwörter aus haex-vault 2.x (Legacy) übernehmen

**Input**: Design documents from `/specs/037-haex-vault-import/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md), [data-model.md](./data-model.md), [contracts/haex-vault-mapping.md](./contracts/haex-vault-mapping.md), [contracts/tauri-commands.md](./contracts/tauri-commands.md), [quickstart.md](./quickstart.md)

**Tests**: Included. The constitution requires a runnable check for non-trivial logic. Tests use Rust unit tests in sibling `*_tests.rs` files, integration tests in `src-tauri/tests/`, and `pnpm check:passwords`. Write each test task before its implementation and see it fail first.

**Organization**: Tasks are grouped by user story in priority order: US1 (P1), US2 (P1), US3 (P2).

**Conventions**:

- **Branch**: `037-haex-vault-import` in `.worktrees/037-haex-vault-import`, branched from `main` at 4168acc.
- **Rust commands**: run them as `nix develop --command scripts/with-nix-host-bridge.sh cargo … --manifest-path src-tauri/Cargo.toml -j 4`, with targeted test targets only. A full local `cargo test` has taken the editor down before.
- **Frontend commands**: run them as `nix develop --command bash -c '…'`.
- **Feature configurations**: everything must compile both with default features and with `--no-default-features`.
- **Test data**: never use real secrets. Use invented values with the marker pattern `SECRET-MARKER-<name>`.
- **Commits**: follow Conventional Commits, without any agent attribution.
- **graphify**: consult it (T002) before authoring a new named artifact.
- **File size**: keep new files ≤ 500 lines. `apply.rs` (677 lines) and `model.rs` (662 lines) receive only a few lines each.

**Shipping note**: One PR, with commits cut along the plan's Complexity Tracking table:

1. reuse folders
2. model and writer
3. reader and fixture
4. UI and texts

---

## Phase 1: Setup

- [x] T001 Prepare the worktree:
  - Do a real `pnpm install` (no symlinked `node_modules`).
  - Reflink the Rust build cache from the primary checkout with `cp -a --reflink=always ../../src-tauri/target src-tauri/target`, but only if `src-tauri/Cargo.lock` matches.
  - Record baseline counts for `cargo test --test passwords_import --test passwords_import_keepass`, `cargo test --lib passwords::import` and `pnpm check:passwords` in this task's note. **Note**: Build cache reflinked from `.worktrees/fix-passwords-keyvalues` (same `Cargo.lock`). Baseline on 4168acc: `passwords_import` 10 pass, `passwords_import_keepass` 15 pass, `pnpm check:passwords` 119 pass.
- [x] T002 [P] Run `graphify query` (budget ~1000) before the first new name, for each of these: "import group write reuse existing folder", "tag color set", "generator preset save", "passkey insert", "sqlcipher open connection key", "temporary directory copy". Prefer an existing candidate. Note which candidates were evaluated and why they did not match. On a borderline match, stop and ask the operator. **Note**: Evaluated `presets::save`, `tags::set_color`, `tags::get_or_create`, `passkeys::insert` and `binaries::ensure_binary`; all are reused. `ensure_group_path` (import/mod.rs) only merges within one model, so it does not replace the lookup of folders already in the vault (`existing_group`, new). `haex_crdt::SqlCipherKey` belongs to `Database::open`, which runs holzi's migrations, so it is unsuitable for a foreign file. The graph had no candidate for opening a foreign SQLCipher file or copying it to a temporary directory. No borderline match.

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: the source variant, model fields, report kinds and error reasons that every story uses, plus the test fixture that builds a haex-vault 2.x vault at run time.

- [x] T003 Extend `src-tauri/src/passwords/import/mod.rs` as in [data-model.md](./data-model.md) §2:
  - Add `ImportSource::LegacyHaexVault` (serde lowercase gives `"legacyhaexvault"`).
  - `ImportItem`: add `color: Option<String>` and `autofill_aliases: Option<serde_json::Value>`.
  - `ImportGroup`: add `color: Option<String>` and `sort_order: Option<i64>`.
  - `ImportModel`: add `tag_colors: Vec<(String, String)>`, `passkeys: Vec<PasskeyInput>` and `presets: Vec<PresetInput>`.
  - Every new field must default to empty, so the KeePass, Bitwarden and LastPass parsers compile with `..Default::default()` or explicit `None`/`Vec::new()` and stay unchanged in behaviour.
  - Until T014 lands, `parse()` returns `PasswordsImportFailed { reason: "unsupported_format" }` for `LegacyHaexVault`, because that source does not come through bytes.
- [x] T004 [P] Extend `src-tauri/src/passwords/model.rs` and `src-tauri/src/passwords/import/report.rs`:
  - `ImportPreview`: add `tags: u32` and `presets: u32`.
  - `AttentionKind`: add `HistoryUnreadable`, `GroupReparented`, `TagMerged` and `UnknownSourceData`, with snake_case names in `report.rs`.
  - Extend `preview()` in `import/mod.rs` to count distinct folded tag names and `presets`. `passkeys` counts the passkeys on items plus `model.passkeys`.
  - Extend `src-tauri/src/passwords/import/report_tests.rs` (or the existing report test file) with one case per new kind. **Note**: There is no separate report test file; the new kinds are covered by the integration tests (`passwords_import_legacy_haex_vault.rs`), which check for every kind in the report.
- [x] T005 [P] Add `tests/fixtures/passwords/legacy_haex_vault_0000_passwords.sql` (path relative to `src-tauri/`):
  - Copy the twelve `CREATE TABLE` statements and the two unique indexes verbatim from haex-vault 2.x `src-tauri/database/migrations/0000_jazzy_chat.sql:414-552` at `8dce379d94e18fcd42c3b73686a06f984ca3f574`, without the `--> statement-breakpoint` lines.
  - The header comment names the repository, the full SHA and the path.
- [x] T006 Create `src-tauri/tests/common/legacy_haex_vault_fixture.rs`, following the pattern of `tests/common/kdbx_fixture.rs`:
  - Constants: `PASSWORD = "legacy-haex-vault-fixture"` and `MARKER`.
  - `build(dir) -> PathBuf`:
    - Create `<dir>/vault.db` with `haex_crdt::rusqlite::Connection::open`, `pragma_update(None, "key", PASSWORD)` and `journal_mode=WAL`.
    - Execute the SQL of T005.
    - Add `haex_hlc TEXT`, `haex_column_hlcs TEXT NOT NULL DEFAULT '{}'` and `haex_column_sigs TEXT NOT NULL DEFAULT '{}'` to every table.
    - Insert sample rows so that every row of [contracts/haex-vault-mapping.md](./contracts/haex-vault-mapping.md) is covered:
      - an entry with all fields, where `created_at` uses the `YYYY-MM-DD HH:MM:SS` form and `updated_at` uses ISO;
      - `otp_digits=8`, `otp_period=60`, `otp_algorithm='SHA256'`;
      - an entry with NULL OTP settings and an invalid secret;
      - `expires_at='2027-01-31'`;
      - `autofill_aliases` JSON;
      - three key-values in insertion order;
      - nested folders three levels deep with `color`/`sort_order`/`description`/`icon`;
      - the `trash` row with one entry and one folder holding an entry below it;
      - two tags, one with colour, plus a case variant (`Work`/`work`);
      - an attachment (Base64 of known bytes) and a binary icon (`binary:<sha256>` with a `type='icon'` row);
      - an icon `binary:<hash>` without a row;
      - icons `i-lucide-key`, `mdi:bank` and an unknown name;
      - three snapshots: editor form with `tagNames`, KeePass form with attachments only in `snapshot_binaries`, external form with `tags: null`;
      - one snapshot with invalid JSON;
      - a passkey on an entry (ES256 pair generated at run time through the existing `kdbx_fixture::passkey` helper, PKCS8/SPKI Base64, `nickname`, `sign_count=5`, `last_used_at`);
      - a standalone passkey (`item_id` NULL);
      - two generator presets, one `is_default=1`.
  - `build_with_wal(dir) -> (PathBuf, Connection)`: same as `build`, but returns the open connection with `wal_autocheckpoint=0` after inserting one more entry titled `MARKER-wal`, so that entry exists only in `vault.db-wal`.
  - `sha256_files(path)`: hashes `vault.db` and, if present, `-wal`/`-shm`, for the unchanged-source checks.

**Checkpoint**: the crate compiles in both feature configurations; the existing import tests are still green (counts from T001).

---

## Phase 3: User Story 1 - Alle Passwörter aus haex-vault 2.x in holzi holen (P1) 🎯 MVP

**Goal**: choose the source, file and password, see a preview with every count, and import everything losslessly.

**Independent Test**: [quickstart.md](./quickstart.md) §1. A fixture vault imports with every field equal to the source (SC-001), and the report lists every deviation.

### Tests for User Story 1

- [x] T007 [P] [US1] Write `src-tauri/tests/passwords_import_legacy_haex_vault.rs`, parser level, failing first:
  - `legacy_haex_vault::read(path, &credentials)` on the T006 fixture returns an `ImportModel` that matches the expected values field by field for each row of the mapping contract.
  - Groups come parent before child; the `trash` row has `is_recycle_bin`; trashed entries, including the one in the folder below `trash`, have `trashed: true`.
  - Key-values are in rowid order.
  - Snapshots of all three JSON forms are read and sorted by `modified_at ?? created_at`.
  - The KeePass-form snapshot gets its attachment from `snapshot_binaries`; the invalid-JSON snapshot is missing and reported as `HistoryUnreadable`.
  - `public_key`, `nickname`, `sign_count` and `last_used_at` of the passkey come over unchanged; the standalone passkey is in `model.passkeys`.
  - `presets` holds two entries.
  - `tag_colors` holds the coloured tag, and `TagMerged` is reported for `Work`/`work`.
  - The missing binary icon gives `IconNotMapped`, the unknown name gives `IconNotMapped`, `i-lucide-key` maps to `lucide:key`, and `mdi:bank` maps to a `lucide:` name.
  - Times are run through `holzi_time`; `expires_at` stays `2027-01-31`.
- [x] T008 [P] [US1] In the same file, write a service-level test (`fixture()` pattern of `tests/passwords_import.rs`: a temporary holzi vault plus `PasswordsService`):
  - Run `import_preview`, then `import_run` with `Skip`, for source `LegacyHaexVault`.
  - Read the rows back with `with_connection` (`#![allow(clippy::disallowed_methods)]`).
  - Entries have `color` and `autofill_aliases`; groups have `color` and `sort_order`; tags have colours.
  - The attachment bytes are equal to the source bytes; the binary icon is set (`binary:` plus the holzi hash).
  - The passkey on an entry and the standalone passkey (`item_id IS NULL`) exist, the latter with the source `public_key`.
  - Both presets exist, and the source default becomes the default only when holzi had none.
  - The trashed entries are under `trash`.
  - The preview counts `tags` and `presets` and its `warnings` contain `legacy_haex_vault_close_first`.
- [x] T009 [P] [US1] Write `src-tauri/src/passwords/import/legacy_haex_vault/icons_tests.rs`, following `import/icons_tests.rs:18`:
  - Every target name of the mapping table occurs as a literal in `src/lib/passwords/icons.ts`.
  - `i-lucide-x` maps to `lucide:x` only if `x` is known.
  - `binary:` values are not handled by the table.

### Implementation for User Story 1

- [x] T010 [P] [US1] Create `src-tauri/src/passwords/import/legacy_haex_vault/icons.rs`:
  - `pub(super) fn map_icon(name: &str) -> Option<&'static str>`.
  - Rules: `i-lucide-<n>` maps to `lucide:<n>`; `lucide:<n>` passes through; `mdi:*` names from haex-vault 2.x `src/components/haex/system/passwords/editor/iconPicker.vue:101-181` (69 names) and the legacy short names in `src/composables/passwords/useIconComponents.ts:8-57` map to the closest `lucide:` name. All of these are at haex-vault 2.x `8dce379`.
  - Every target must exist in `IMPORT_ICONS` of `src/lib/passwords/icons.ts`; add missing literals there (T009 enforces this).
  - Unknown names give `None`.
- [x] T011 [US1] Create `src-tauri/src/passwords/import/legacy_haex_vault/open.rs`, following [research.md](./research.md) R1, R2 and R10:
  - `pub(super) struct Source { _dir: tempfile::TempDir, conn: Connection }`.
  - `pub(super) fn open(path: &Path, password: &str) -> Result<Source>`:
    1. Read the first 16 bytes. A file shorter than 4096 bytes or one starting with `SQLite format 3\0` is `unsupported_format`.
    2. Copy the file to `<tmp>/vault.db` with `std::fs::copy`, and `<path>-wal` to `<tmp>/vault.db-wal` if it exists. An I/O error is `unreadable`.
    3. Open with `Connection::open_with_flags(READ_WRITE)`, then `pragma_update(None, "key", password)`.
    4. Probe with `SELECT count(*) FROM sqlite_master`. `SQLITE_NOTADB` is `legacy_haex_vault_locked`.
  - Add a `ponytail:` comment on the key being rendered into SQL text, naming the ceiling and the upgrade path from R10.
  - Never touch `path` beyond reading it.
  - Write `open_tests.rs` for the header checks using small temp files.
- [x] T012 [US1] In the same file, add `pub(super) fn check_schema(conn) -> Result<Vec<Problem>>`:
  - If `haex_passwords_item_details` is missing, fail with `no_passwords`.
  - Required tables and columns are those of T005, timestamps excepted. A missing one is `unsupported_format`.
  - Extra columns, apart from the three CRDT columns, and extra tables with the `haex_passwords_` prefix are each reported as `UnknownSourceData` with table and column.
  - Tables named like `%__haex-pass__haex_passwords_%` produce the preview warning `haex_pass_tables_ignored`, returned separately.
  - Use `PRAGMA table_info` and `sqlite_master` only.
- [x] T013 [US1] Create `src-tauri/src/passwords/import/legacy_haex_vault/read.rs`: `pub(super) fn to_model(conn) -> Result<ImportModel>`, following [contracts/haex-vault-mapping.md](./contracts/haex-vault-mapping.md). Query columns by name, never with `SELECT *`.
  - **Binaries**: decode each referenced hash once, through a `HashMap` cache, with `base64::engine::general_purpose::STANDARD`.
  - **Groups**: emit them in topological order (parents first), because `write_groups` resolves `parent_ref` from earlier groups. Break cycles and missing parents by setting `parent_ref = None` and reporting `GroupReparented`.
  - **Trash**: an entry is trashed when its group chain reaches `trash`.
  - **Tags**: fold names through `push_tag`; report case collisions as `TagMerged`; collect colours.
  - **Attachments**: use `ImportAttachment::within_limit`.
  - **Snapshots**: deserialize into `SnapshotData`, with attachments taken from `snapshot_binaries`.
  - **Passkeys**: build `PasskeyInput` 1:1 with `is_discoverable != 0`.
  - **Presets**: build `PresetInput` with an empty `id`.
  - Run every time through `holzi_time`.
  - Call `check_otp` on each item.
  - Split helpers (`groups.rs`, `history.rs`) only if the file exceeds 500 lines.
- [x] T014 [US1] Create `src-tauri/src/passwords/import/legacy_haex_vault/mod.rs` with `pub fn read(path: &Path, credentials: &Credentials) -> Result<ImportModel>`:
  - The password is required; a missing one is `legacy_haex_vault_locked`.
  - Steps: `open`, then `check_schema`, then `to_model`. Append the schema problems to `source_problems`.
  - The `TempDir` drops at the end of the function.
  - Declare `mod legacy_haex_vault;` in `import/mod.rs`.
  - In `src-tauri/src/passwords/service/import.rs::read_model`, branch for `ImportSource::LegacyHaexVault` before `std::fs::read` and call `legacy_haex_vault::read` inside the existing `spawn_blocking`.
  - The preview adds `legacy_haex_vault_close_first` and, if needed, `haex_pass_tables_ignored` to `warnings`.
- [x] T015 [US1] Extend `src-tauri/src/passwords/import/apply.rs`:
  - `write_item` inserts `color` and `autofill_aliases` (`serde_json::to_string`) instead of `NULL` (`apply.rs:442`, `:449`).
  - `write_groups` inserts `color` and `sort_order`.
  - Keep the diff small (the file is at 677 lines).
- [x] T016 [US1] Create `src-tauri/src/passwords/import/apply_extras.rs`, called from `write_all` after the entries and before the attachments end. It writes, each in its own `write`:
  - **Tag colours**: for `(name, color)` in `tag_colors`, if the folded tag exists and has no colour, call `tags::set_color` (`tags.rs:128`).
  - **Standalone passkeys**: `model.passkeys` through `passkeys::insert`. `Duplicate` becomes `PasskeyDuplicate` in the report; new ids go to `Ledger.passkeys`.
  - **Presets**: `model.presets` through `presets::save`. Skip a preset when holzi already has one with the same name (case-folded). Clear `is_default` when holzi already has a default. A validation error becomes `ValueNotStorable`. New ids go to `Ledger.presets`.
  - Extend `Ledger` with `passkeys` and `presets`, and make `rollback` delete them.
  - Unit tests go in `apply_extras_tests.rs`. **Note**: The three parts are written together in one `write` (step `Step::Extras`) instead of one each: they are few and small, and the ledger and rollback stay the same.
- [x] T017 [P] [US1] Wire up the wizard in `src/components/passwords/ImportWizard.vue`, following [contracts/tauri-commands.md](./contracts/tauri-commands.md) §Oberfläche:
  - Add `SOURCES` entry `legacyhaexvault`, labelled "haex-vault 2.x", with extension filter `db`.
  - Show the password row for `keepass` and `legacyhaexvault`; the key file only for `keepass`.
  - `canPreview` requires file and password for `legacyhaexvault`.
  - The preview shows `tags` and `presets`.
  - Warnings render through their i18n keys.
  - If the source-to-credentials rule is more than a one-liner, put it as a pure helper in `src/lib/passwords/importReport.ts` and add cases to `scripts/check-passwords-import.ts`.
- [x] T018 [P] [US1] Add to `src/i18n/locales/de.json` and `en.json` (same key trees):
  - the source name;
  - the preview labels for tags and presets;
  - warnings `legacy_haex_vault_close_first` ("Schließe haex-vault 2.x, bevor du die Datei kopierst; sonst können die letzten Änderungen fehlen.") and `haex_pass_tables_ignored`;
  - report texts for `history_unreadable`, `group_reparented`, `tag_merged`, `unknown_source_data`;
  - `importReason.legacy_haex_vault_locked` ("Das Vault-Passwort passt nicht, oder die Datei ist keine haex-vault-2.x-Vault.") and `importReason.no_passwords`.
- [x] T019 [US1] Regenerate the ts-rs bindings (`ImportSource`, `ImportPreview`, `AttentionKind`) through the bridged export test. Strip trailing whitespace with `sed -E 's/[[:space:]]+$//'`, and revert bindings that changed only in whitespace.

**Checkpoint**: T007 to T009 are green; the existing import tests are unchanged; the wizard shows the new source (`pnpm typecheck`, `lint`, `check:templates`, `check:passwords`).

---

## Phase 4: User Story 2 - Falsche Datei, falsches Passwort oder Abbruch ohne Schaden (P1)

**Goal**: every failure is explained, nothing is written, and the source file is never changed.

**Independent Test**: wrong password, foreign files, a vault without password tables, a missing column, and a cancel. Each one leaves the source hashes and the holzi vault unchanged (SC-002).

- [x] T020 [P] [US2] Add tests in `src-tauri/tests/passwords_import_legacy_haex_vault.rs`, failing first, each asserting the reason and the unchanged holzi counts:
  - wrong password gives `legacy_haex_vault_locked`;
  - random bytes of 8 KiB give `legacy_haex_vault_locked`;
  - an unencrypted SQLite file gives `unsupported_format`;
  - an empty file gives `unsupported_format`;
  - an encrypted vault without `haex_passwords_*` gives `no_passwords`;
  - a fixture with `haex_passwords_item_details.password` dropped gives `unsupported_format` (rebuild the table without that column);
  - an extra column `foo` gives an `UnknownSourceData` row while the import succeeds.
- [x] T021 [P] [US2] Add a test in the same file for the unchanged source and the WAL:
  - With `build_with_wal`, the open connection keeps the `MARKER-wal` entry only in `-wal`.
  - `sha256_files` before and after the preview, the run, a failed run with a wrong password and a cancelled run are identical.
  - The `MARKER-wal` entry arrives in holzi.
  - No `-shm` or other new file appears next to the source (list the directory before and after).
  - The source directory may be set read-only for the run (`std::fs::set_permissions`), and the import still succeeds.
- [x] T022 [P] [US2] Add a cancel and rollback test in the same file:
  - Use the `inject` hook of `Control` (as the existing tests in `tests/passwords_import.rs` do) to fail after the standalone passkeys and presets are written.
  - Afterwards, holzi has no new entries, folders, tags, passkeys (including `item_id IS NULL`) or presets.
  - The reason is `cancelled` for a user cancel.
- [x] T023 [US2] Make T020 to T022 pass by adjusting `open.rs`, `mod.rs` and `apply_extras.rs`; no new public surface. Ensure the password from `Credentials` is never formatted into an error, log line or report (grep the new modules for `password` in `format!`/`tracing` calls).
- [x] T024 [P] [US2] Check that the wizard shows the new reasons through `importFailureReason` in `src/lib/passwords/importReport.ts`. Add cases for `legacy_haex_vault_locked` and `no_passwords` to `scripts/check-passwords-import.ts`.

**Checkpoint**: all failure paths are green; SC-002 is proven by test.

---

## Phase 5: User Story 3 - Import wiederholen ohne Doppelte (P2)

**Goal**: a second import with "skip" creates nothing, and folders are never duplicated, for every source.

**Independent Test**: import the same file twice. The second run with "skip" creates 0 entries, folders, tags, passkeys and presets (SC-003), for haex-vault 2.x and for KeePass.

- [x] T025 [P] [US3] Add tests, failing first:
  - In `src-tauri/tests/passwords_import_legacy_haex_vault.rs`, a second `import_run` with `Skip` on the same fixture leaves the row counts of all password tables unchanged. The preview's `duplicates` equals the number of non-trashed entries.
  - In `src-tauri/tests/passwords_import.rs`, the same for the KeePass fixture: before this change the folder tree was duplicated, now the folder count is unchanged.
  - A first import into a vault that already has a top-level folder with the same name as a source folder puts the entries into that existing folder.
  - A rollback after reuse does not delete the pre-existing folder.
- [x] T026 [US3] Change `write_groups` in `src-tauri/src/passwords/import/apply.rs`:
  - Before inserting a non-recycle-bin group, look for an existing row with `name = ?` and `parent_id IS ?` (the already resolved parent, `NULL` at the top).
  - If found, reuse its id: map `reference` to it and do not push it to `created`, so it never enters the ledger.
  - Otherwise insert as before.
  - Document in a short comment why the reuse happens: a repeated import must not duplicate the tree (spec 037 FR-017).
  - If the extra lines push the function past ~60 lines, extract the lookup as `existing_group(tx, name, parent)`.
- [x] T027 [US3] Verify that standalone passkeys with an existing credential id appear as `PasskeyDuplicate` on the second run and that presets are skipped by name (covered in T016; assert both in T025's haex-vault 2.x case).

**Checkpoint**: SC-003 holds for haex-vault 2.x and KeePass.

---

## Phase 6: Polish & Cross-Cutting Concerns

- [x] T028 [P] Add a short section "haex-vault 2.x" to `specs/034-password-manager/contracts/import-mapping.md` that links to [contracts/haex-vault-mapping.md](./contracts/haex-vault-mapping.md), and note there that folders are now reused on re-import for every source.
- [x] T029 Run the CI parity set from [quickstart.md](./quickstart.md) §2 (`cargo fmt --check`, `pnpm lint:rust` with both feature sets, the targeted `cargo test` targets of §1, `pnpm lint`, `typecheck`, `format:check`, `check:templates`, `check:passwords`). Record the results in this task's note, including anything not run. **Note** (2026-10-04): `cargo fmt --check` ok; `pnpm lint:rust` (clippy, default and `--no-default-features`, `-D warnings`) ok; `cargo test --lib passwords::` 275 pass; `--test passwords_import` 12, `passwords_import_legacy_haex_vault` 12, `passwords_import_keepass` 15 pass; `pnpm lint`, `typecheck`, `format:check`, `check:templates` (136 templates) ok; `check:passwords` 122 pass. Not run: the full `cargo test` (only targeted runs, see Conventions) and the e2e suite (no import scenario).
- [x] T030 Commit along the four cuts of the shipping note, push with the `haexmas` account and open the PR against `main`. The PR description names the spec, the change to folder reuse for all sources, what was tested, and that T031 is still open. **Note**: PR #269. Cuts 2 and 3 are one commit (see the shipping note), plus a docs commit.
- [x] T031 Manual quickstart §3 with the operator's real haex-vault 2.x file (operator task; the agent does not have the file). **Note**: Done by the operator on 2026-10-05 with their real vault; the import worked.

---

## Dependencies & Execution Order

- **Setup** (T001, T002): first.
- **Foundational** (T003 to T006): blocks every story. T004 and T005 can run in parallel with T003. T006 needs T005.
- **US1** (T007 to T019): needs Phase 2.
  - Tests T007 to T009 first.
  - T010 can run in parallel with T011 and T012.
  - T013 needs T010 and T012; T014 needs T011 to T013.
  - T015 and T016 can run in parallel with the reader but are needed by T008.
  - T017 to T019 come after T003 and T004.
- **US2** (T020 to T024): needs T014 and T016. It is mostly tests on top of US1's code.
- **US3** (T025 to T027): needs T016 for presets and passkeys. T026 touches the shared `write_groups` and is independent of the reader; it can land before US1 (commit cut 1).
- **Polish**: after all stories.

## Parallel Example: User Story 1

```text
T007 parser-level tests   |  T009 icon table tests  |  T010 icons.rs
T011 open.rs              |  T015 apply.rs fields   |  T017 ImportWizard.vue  |  T018 i18n
```

## Implementation Strategy

1. **MVP** = Phase 1, Phase 2 and US1. With that, the operator can import their vault. Stop and validate with quickstart §1.
2. **US2** hardens every failure path. It is P1 because the data is all of the operator's passwords, so it ships in the same PR.
3. **US3** makes repeating safe and fixes the folder duplication for all sources.
4. Then Polish, the PR, and the operator's manual run (T031).
