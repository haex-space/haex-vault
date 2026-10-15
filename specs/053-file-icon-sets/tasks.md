---
description: 'Task list for spec 053-file-icon-sets'
---

# Tasks: Austauschbare Icon-Sets für den Dateibrowser

**Input**: Design documents from `/specs/053-file-icon-sets/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md), [data-model.md](./data-model.md), [contracts/tauri-commands.md](./contracts/tauri-commands.md), [quickstart.md](./quickstart.md)

**Tests**: Included. The constitution requires a runnable check for non-trivial logic; SC-002 (coverage of the bundled sets), SC-004 (speed), SC-005 (nothing executable is stored or shown), SC-006 (a set arrives whole), SC-007 (always a fallback image) and SC-008 (licenses of the bundled sets) need dedicated tests. Rust tests live in `*_tests.rs` next to the file (`#[cfg(test)] #[path = "x_tests.rs"] mod tests;`), never inline. Fixtures live in `src-tauri/tests/fixtures/icon_sets/`. No network services in `cargo test`. Build only the affected targets with `-j 4`. Every command runs in the devShell: `nix develop --command …`, cargo through `scripts/with-nix-host-bridge.sh` (rule of the holzi atom 0.10.0).

**Organization**: Grouped by user story in priority order (P1: US1, US2; P2: US3, US4; P3: US5). The resolver and the bundled packs are shared by US1 and US2 and sit in Phase 2. US1 is the MVP.

**Shipping note** (plan.md): PR A = Phase 1 (docs, #443) + Phase 2 + US2; PR B = US1 + the license page of US5; PR C = US3 + the remove action of US5; PR D = US4 + Polish. Each PR is reviewed and merged before the next one starts on top of `main`.

**Rules for every task**: no new dependency (`quick-xml`, `image`, `zip`, `haex-bundle`, `sha2` are there); one implementation per rule (resolution, role, sanitizing only in `src-tauri/src/files/icons/`; path rules through `haex_bundle::format`; license texts only inside the packs); no `unwrap()` or `expect()` on data from files, archives or the vault; every file in `files/icons/` stays under 500 lines; before authoring a new named function, component or module consult graphify (`graphify query`) and extend a candidate if one fits (plan.md lists those evaluated); an image is never embedded into the DOM, only used as the source of an `<img>` (FR-010).

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependency on an incomplete task)
- **[Story]**: User story from spec.md (US1–US5)

---

## Phase 1: Setup (PR A, docs)

- [x] T001 Spec, plan, research, data model, contract, quickstart in `specs/053-file-icon-sets/` (PR #443)
- [x] T002 [P] Add a row for 053 to `plans/README.md` after the 052 row: "053: Austauschbare Icon-Sets für den Dateibrowser (`specs/053-file-icon-sets/`)", priority P2, effort L, status "Spezifiziert, geplant und in Aufgaben zerlegt 2026-10-14; Lieferungen PR A–D", gate "044, 023 und 024 im Einsatz"
- [x] T003 Run `nix develop --command pnpm exec prettier --write specs/053-file-icon-sets plans/README.md`, commit `tasks.md` and the README row, push to PR #443

---

## Phase 2: Foundational (PR A, blocks all user stories)

**Purpose**: the table, the pure pieces (sanitizer, image check, pack, manifest, role, resolution), the store, the bundled packs and the read commands. After this phase `files_icons_resolve` answers for every set.

### Tests for the foundation

- [x] T004 [P] `src-tauri/src/identity/migrations_icon_sets_tests.rs`: `SYNCED_TABLES` lists every table of `0036` (like `migrations_storage_tests.rs:57-65`); `id` is the primary key; no `UNIQUE` anywhere; inserting a row and deleting it leaves a marker in `haex_deleted_rows`; the table is CRDT-tracked (not `_no_sync`)
- [x] T005 [P] `src-tauri/src/files/icons/svg_tests.rs`: a `<script>`, an `onload`/`onclick` attribute, `foreignObject`, `<a>`, `<animate>`, `<set>`, `<iframe>`, `<style>` and an external `href` or `xlink:href` (anything not starting with `#`) are gone; `url(http://…)`, `url(data:…)` outside an `image` and `@import` in a `style` attribute are gone; a DOCTYPE, an entity declaration or a processing instruction is refused (`Err`), not expanded; depth over the limit and size over the limit are refused; paths, shapes, gradients, `use href="#id"`, `clipPath`, `mask` survive and the output renders the same elements; sanitizing twice gives the same bytes (idempotent); a non-UTF-8 or malformed file is an `Err`
- [x] T006 [P] `src-tauri/src/files/icons/raster_tests.rs`: a small PNG and a small WebP pass; 513 × 512 and over `Limits` are refused; a PNG named `.webp` (format does not match the extension) is refused; a corrupt PNG is refused; GIF, JPEG and BMP are refused with the reason "format"
- [x] T007 [P] `src-tauri/src/files/icons/pack_tests.rs`: every entry name goes through `haex_bundle::format::path_rule_violation` and `PathChecker` (`..`, absolute, `\`, `:`, control characters, not NFC, segment over 255 bytes, duplicates also after case and NFC folding are refused); directory entries and symlinks are refused; limits "at most 2000 images, 256 KiB per image, 5 MiB unpacked, ratio packed/unpacked at most 200, 8 MiB packed" each refuse one fixture; every `iconPath` of the manifest names an entry in the pack; a pack whose `sha256` differs from the stored one is `damaged`; packing a directory twice gives identical bytes (fixed timestamps, sorted entries)
- [x] T008 [P] `src-tauri/src/files/icons/manifest_tests.rs`: parse the VS Code format from `src-tauri/tests/fixtures/icon_sets/theme-basic/` (`iconDefinitions` with `iconPath`, `file`, `folder`, `fileNames`, `fileExtensions`, `folderNames`, block `light`); definitions with `fontCharacter` are ignored; `folderExpanded`, `folderNamesExpanded`, `rootFolder*`, `languageIds`, `highContrast`, `hidesExplorerArrows` are ignored without an error; names are matched in lower case; `x-holzi.roles` and `x-holzi.origin` are read; an unknown key does not fail; a manifest over the size limit or with a path outside the pack is refused
- [x] T009 [P] `src-tauri/src/files/icons/role_tests.rs`: `viewer_kind` of the name gives `image`, `video`, `audio`, `pdf`, `text`, else `file`; `no_access` on a folder gives `folderNoAccess`; `EncryptedInfo` (readable folder, other vault, newer format, damaged, a file inside) gives `encryptedFolder`, `lockedFolder`, `lockedFile`, `damaged`, and a readable entry inside gives its ordinary role
- [x] T010 [P] `src-tauri/src/files/icons/resolve_tests.rs`: the order of research R7: (0) state, (1) folder name or file name, (2) extension with the longest suffix first (`a.test.ts` tries `test.ts`, then `ts`), (3) `x-holzi.roles`, (4) the `file` or `folder` default of the theme, (5) the fallback table of Lucide; for `scheme = light` every step looks into the block `light` first and then into the main part; an unknown entry gets the fallback symbol and never an error (SC-007); a state is never covered by a name or an extension match

### Implementation of the foundation

- [x] T011 Check `origin/main` for the next free migration number (last was `0034_contacts_phone_known`) and for branches that add one (`git branch -r`, `git grep`), and write it into the header of the new file; then add `src-tauri/src/identity/migrations_icon_sets.rs` with `ICON_SETS_0036` creating `haex_file_icon_sets` exactly as in data-model.md: `id TEXT PRIMARY KEY` (UUID), `name TEXT NOT NULL`, `author TEXT`, `license TEXT`, `source TEXT`, `image_count INTEGER`, `byte_size INTEGER`, `sha256 TEXT`, `pack BLOB`, `created_at TEXT`; no `UNIQUE`, no `CHECK` (limits are checked in code); `SYNCED_TABLES = ["haex_file_icon_sets"]`; register `0036_file_icon_sets` in `src-tauri/src/identity/migrations.rs`, raise `HOLZI_TRIGGER_VERSION` from 23 and add its line to the list above the constant (T004 passes)
- [x] T012 [P] `src-tauri/src/storage/preferences_commands.rs`: in `validate_value` accept `files.iconSet` only with the shape `builtin:lucide`, `builtin:phosphor`, `builtin:material` or `custom:<uuid>` (a UUID of 36 characters); extend the existing test file next to it with one accepted and three refused values; a missing key is not an error when reading
- [x] T013 [P] `src-tauri/src/vault_events.rs` needs no change: the event `vault-data-changed` carries the tables of every committed transaction, so the window listens for `haex_file_icon_sets` by name with `onVaultTablesChanged` (T035); nothing to register
- [x] T014 `src-tauri/src/files/icons/mod.rs` and `src-tauri/src/files/mod.rs` (`pub mod icons;`): the types of the contract with `#[derive(TS)]` and `#[ts(export, export_to = "../../src/types/bindings/")]`: `IconSetRef`, `Scheme`, `IconQuery`, `IconAnswer`, `IconSetInfo`, `IconSetState`, `ImportReport`, `IconSource`; the role enum `IconRole` with `folder`, `folderNoAccess`, `file`, `image`, `video`, `audio`, `pdf`, `text`, `encryptedFolder`, `lockedFolder`, `lockedFile`, `damaged`; the fallback table role → symbol of data-model.md as one `match` (`lucide:folder`, `lucide:folder-lock`, `lucide:file`, `lucide:image`, `lucide:film`, `lucide:music`, `lucide:file-text` for `pdf` and `text`, `lucide:folder-key`, `lucide:folder-lock`, `lucide:file-lock`, `lucide:file-warning`)
- [x] T015 [P] `src-tauri/src/files/icons/svg.rs`: the sanitizer of research R9 on `quick-xml` with an allow list of elements (`svg`, `g`, `defs`, `title`, `desc`, `symbol`, `use`, `path`, `rect`, `circle`, `ellipse`, `line`, `polyline`, `polygon`, `linearGradient`, `radialGradient`, `stop`, `clipPath`, `mask`, `pattern`, `filter` with the `fe*` primitives, `image` only with a `data:` PNG or WebP) and of attributes (presentation attributes; `href` only as `#id`); everything else is dropped with its content; DOCTYPE, entities and processing instructions are an error; limits on depth and size; the output is written anew, not patched text; `pub fn sanitize_svg(bytes: &[u8]) -> Result<Vec<u8>, SvgError>` (T005 passes)
- [x] T016 [P] `src-tauri/src/files/icons/raster.rs`: `pub fn check_raster(name: &str, bytes: &[u8]) -> Result<RasterKind, RasterError>` with `image` (features `png`, `webp`), `Limits` of 512 × 512 and bounded memory, the format must match the extension, the original bytes are kept (T006 passes)
- [x] T017 `src-tauri/src/files/icons/pack.rs`: open a pack from bytes (`zip` 8, in memory, never to disk); the limits of data-model.md "at most 2000 images, 256 KiB per image, 5 MiB unpacked, ratio at most 200, 8 MiB packed"; every path through `haex_bundle::format::path_rule_violation` and `PathChecker`; directories, symlinks and non-regular entries refused; check `sha256`; `OpenSet` (manifest tables, entry index, a bounded cache of sanitized images, `Arc`); `pub fn pack_dir(dir: &Path) -> Result<Vec<u8>, PackError>` (fixed timestamps, sorted entries, deflate) used by the import and by the packer of T024; first check whether `haex_bundle::archive::read_archive` reads a plain ZIP without manifest and signature (note the result in a comment); if not, use `zip` with own accounting (T007 passes)
- [x] T018 [P] `src-tauri/src/files/icons/manifest.rs`: `IconTheme` from the JSON of the pack: maps for `fileNames`, `fileExtensions`, `folderNames` (keys in lower case), `file`, `folder`, definitions with `iconPath` normalized (`./../icons/x.svg` → `icons/x.svg`), the block `light`, `x-holzi.roles` and `x-holzi.origin`; ignored keys listed in T008; extension lookup helper `longest_suffix_first(name)` (T008 passes)
- [x] T019 [P] `src-tauri/src/files/icons/role.rs`: `pub fn role_of(query: &IconQuery) -> IconRole` using `files::kind::viewer_kind` for files and `EncryptedInfo` and `no_access` for the states (T009 passes)
- [x] T020 `src-tauri/src/files/icons/resolve.rs`: `pub fn resolve(set: &OpenSet, scheme: Scheme, query: &IconQuery) -> IconAnswer` in the order of research R7 (state first, then name, extension, role, theme default, fallback); the Lucide set is a `Set` without images, so every role answers `builtin` (T010 passes)
- [x] T021 `src-tauri/src/files/icons/store.rs`: read and write `haex_file_icon_sets` (insert, list, delete, by id) as `Caller::Internal`; `selection()` reads `files.iconSet` and returns an `IconSetState` from data-model.md (`ready`, `syncing`: row missing and no marker, `missing`: row missing and marker, `damaged`: pack fails the check, `unknown`: `builtin:<name>` not known to this app); the open sets are kept as a shared object under the key (id, sha256)
- [x] T022 `src-tauri/src/files/icons/commands.rs`: `files_icons_resolve` (at most 500 queries per call; an unknown, missing or damaged set answers like Lucide), `files_icon_image(set, id)` (no `scheme`: the `id` is the path of the image in the pack, which `files_icons_resolve` chose for the scheme; raw bytes through `tauri::ipc::Response`, an SVG only after `sanitize_svg`, an unknown `id` is `InvalidInput`), `file_icon_set_selection` (the choice and its state, for the group in the settings), `file_icon_sets_list` (bundled first, then custom by name, with state and `removable`), `file_icon_set_licenses` (name, author, license, source, text from `origin` and `LICENSE` of the bundled packs, plus the texts of Lucide as constants); register all in `src-tauri/src/lib.rs` (`generate_handler!`)
- [x] T023 Generate the TypeScript bindings: `nix develop --command scripts/with-nix-host-bridge.sh pnpm generate:ts-types`, then `git checkout` any unrelated binding churn and strip trailing whitespace as the script does; new files `IconSetRef.ts`, `Scheme.ts`, `IconQuery.ts`, `IconAnswer.ts`, `IconSetInfo.ts`, `IconSetState.ts` appear in `src/types/bindings/`
- [x] T024 `scripts/icon-sets/prepare.ts` (runs with `node` in the devShell, no new dependency): reads the pinned source and writes the source directory; for Phosphor (`phosphor-icons/core` 2.1.1, weight `regular`) takes the icons of `src-tauri/icon-sets/phosphor.icons.txt` (file-type icons `file-*`, `folder`, `folder-lock`, `folder-simple`, `folder-simple-lock`, `file`, `file-lock`, `file-x`, `file-dashed`; never a `*-logo` icon), writes two copies of each with `currentColor` replaced by a light and a dark grey (colors constants in the script; the main part of the manifest is the light-stroke version for the dark scheme, the block `light` the dark-stroke version), builds `icon-theme.json` from `src-tauri/icon-sets/phosphor.mapping.json` and `x-holzi` (roles, origin MIT) and copies the MIT text as `LICENSE`; for Material (VSIX 5.39.0, `extension/dist/material-icons.json` and `extension/icons/`) copies all 1251 icons, normalizes `./../icons/x.svg` to `icons/x.svg`, drops `folderNamesExpanded`, `rootFolder*`, `rootFolderNamesExpanded`, `languageIds`, `highContrast`, `hidesExplorerArrows`, and writes `LICENSE` (MIT), the Apache-2.0 text and `NOTICE` ("contains icons from Material Design Icons (Pictogrammers) and Material Symbols (Google), Apache-2.0; the logos and marks of companies, products, languages and tools belong to their owners and only mark the file type") and `origin`
- [x] T025 Run `prepare.ts` and check in the result: `src-tauri/icon-sets/phosphor/` and `src-tauri/icon-sets/material/` (source directories), `phosphor.icons.txt`, `phosphor.mapping.json` (hand-written first version: `pdf`, `doc`, `docx`, `odt`, `xls`, `xlsx`, `ods`, `ppt`, `pptx`, `zip`, `7z`, `tar`, `gz`, `png`, `jpg`, `jpeg`, `rs`, `ts`, `py` with their `file-*` icons; languages without an icon of their own (`go`, `kt`, `java`, `rb`, `php`, `sh`) to `file-code`; the roles for image, video, audio, pdf, text, folder, folder without access, the lock states and damaged), and `src-tauri/icon-sets/SOURCES.md` (per set: source address, version, commit SHA or checksum, SPDX license); no local paths in any of them (constitution)
- [x] T026 The packer and the embedding: a command (an `#[ignore]`d test `build_bundled_packs` or an example target, decide when writing it) that calls `pack_dir` for `phosphor/` and `material/` and writes `src-tauri/icon-sets/phosphor.zip` and `material.zip`; `src-tauri/src/files/icons/builtin.rs` embeds both with `include_bytes!` and lists the three built-in sets (`lucide` without a pack); a normal test compares the files of each source directory with the files in the checked-in ZIP (the same names and bytes; not the ZIP bytes, which depend on the version of the compression library); measure and write the sizes of both ZIPs into `SOURCES.md`; every bundled pack passes `pack.rs` and `sanitize_svg` on each of its SVG (the same checks as the import); the Material pack holds 1251 images and the Phosphor pack the images of its list

**Checkpoint**: `cargo test -j 4 --lib files::icons` and `identity::migrations_icon_sets` are green; `files_icons_resolve` answers for Lucide, Phosphor and Material.

---

## Phase 3: User Story 2 - Dateitypen erkennbar machen (Priority: P1)

**Goal**: A set with many file types shows `Cargo.toml`, `main.rs`, `foto.png` and `src` differently; what a set does not know falls back to the default set; states are never hidden.

**Independent Test**: With the Phosphor and the Material pack resolve a list of typical names; the counts of SC-002 hold and the states win over every name or extension.

### Tests for User Story 2

- [x] T027 [P] [US2] `src-tauri/src/files/icons/bundled_tests.rs`: with the Material pack at least 50 common folder names (`src`, `docs`, `images`, `config`, `test`, …) resolve to a folder image of their own and at least 100 common names and extensions (`readme`, `license`, `json`, `yaml`, `zip`, `rs`, `ts`, `vue`, `py`, …) resolve to an image other than the `file` default (SC-002); with the Phosphor pack at least 40 common extensions (`pdf`, `docx`, `xlsx`, `pptx`, `zip`, `png`, `mp3`, `mp4`, `rs`, `ts`, `vue`, `py`, `md`, `csv`, …) resolve to an image other than the `file` default; the states (`encryptedFolder`, `lockedFolder`, `lockedFile`, `damaged`, `folderNoAccess`) resolve for both packs (own image or the Lucide fallback) and an entry in a state never takes a name or extension image (for example a locked file named `main.rs`)
- [x] T028 [P] [US2] `src-tauri/src/files/icons/bundled_scheme_tests.rs`: for Material the 25 names and 31 extensions of its block `light` give the `*_light.svg` image when `scheme = light` and the main image when `scheme = dark`; Phosphor gives different image ids for `light` and `dark`; a pack without a block `light` gives the same image in both schemes

### Implementation for User Story 2

- [x] T029 [US2] Extend `src-tauri/icon-sets/phosphor.mapping.json` to the coverage of T027 (at least 40 extensions: office formats, archives, images, audio, video, the common languages and `md`, `csv`, `ini`, `txt`, `svg`, `html`, `css`, `js`, `jsx`, `tsx`, `sql`, `vue`, `c`, `cpp`, `cs`), rerun `prepare.ts` and the packer of T026, update `SOURCES.md`
- [x] T030 [US2] Fix every failure of T027 and T028 in `resolve.rs`, `manifest.rs` or the content of the packs (for example a Material image that does not pass the sanitizer is a finding: record it in `SOURCES.md` and sanitize it at import, never skip it silently); count how many Material images need a `<style>` and write the number into research R9 (the open task of R9)

**Checkpoint**: US2 holds on its own against the packs; no UI needed.

---

## Phase 4: User Story 1 - Ein anderes Icon-Set wählen (Priority: P1) 🎯 MVP (PR B)

**Goal**: The user picks a set in the settings; the lists show it at once; the choice survives a restart.

**Independent Test**: quickstart §2 steps 1–3 and 6 (`files-icon-sets`).

### Tests for User Story 1

- [x] T031 [P] [US1] `scripts/check-file-icons.ts` (run with `node --test`): the pure helpers of `src/lib/files/fileIcons.ts`: the cache key `(set, scheme, name, isDir, noAccess, encrypted)` is stable; queries of one frame are grouped into one call of at most 500; duplicates are asked once; the memory is bounded (oldest entries leave first, the `blob:` URL of a leaving image is revoked); it is emptied when the set or the scheme changes; add `check:file-icons` to `package.json` next to `check:files`
- [x] T032 [P] [US1] `scripts/e2e/scenarios/files-icon-sets.test.ts` (steps 1–3 and 6 of quickstart §2): the group "Icon-Set für Dateien" lists Lucide (chosen), Phosphor and Material with a preview; choosing Phosphor shows `img` elements (not `svg` elements) in the open folder listing within a second, "Gespeichert." appears, there is no save button; restart keeps the choice; in a folder with an encrypted unreadable file and a folder without access the state images appear under Phosphor too; helpers from `scripts/e2e/lib/settings.ts`; `data-testid` values `settings-icon-set`, `settings-icon-set-option-<id>` (Done for the folder without access, skipped where the process is root; the encrypted unreadable file is covered by `bundled_tests.rs` and the state images of `files-encrypted-two-devices`, not by this scenario.)

### Implementation for User Story 1

- [x] T033 [P] [US1] `src/composables/useAppliedScheme.ts`: a `Ref<'light' | 'dark'>` kept by `onColorSchemeApplied` of `useColorScheme.ts` (gap 5 of the code report: today only the class `dark` exists)
- [x] T034 [P] [US1] `src/lib/files/fileIcons.ts`: the pure part of T031 (key, grouping, bounded memory with revoke callback); no Vue imports so that `node --test` loads it
- [x] T035 [US1] `src/composables/useFileIcons.ts`: reads the choice (`get_pref` vault scope `files.iconSet`, default `builtin:lucide`), asks `files_icons_resolve` once per frame for the rows that appeared, fetches the bytes of an image with `files_icon_image` (`invoke<ArrayBuffer>`, one `blob:` URL per `(set, id, scheme)` with the MIME of the answer), returns for an entry a reactive `{ kind: 'image', url } | { kind: 'builtin', name } | null` (`null` while pending: the row shows an empty box of the same size); reloads on `onVaultTablesChanged(['preferences', 'haex_file_icon_sets'])` and on a scheme change and empties the memory
- [x] T036 [US1] `src/components/files/EntryIcon.vue`: ask `useFileIcons` instead of the fixed `lucide:*` choice; `<img :src class="size-full object-contain" draggable="false" alt="">` for an image and the existing `<Icon :name>` for a builtin symbol; keep thumbnails for images in the grid, the `title` of the state and `data-encrypted`; `FolderView.vue` (list `size-5`, grid `size-24`) and `SearchResults.vue` need no change beyond the component
- [x] T037 [US1] `src/lib/files/encrypted.ts`: `encryptedLook` keeps only the state text key (`stateKey`) and the shown name key; the `icon` field and the `lucide:*` choices leave (they moved to the fallback table of T014); update the callers and `scripts/check-files-viewer.ts` or the check that covers `encrypted.ts` if it asserts the icon
- [x] T038 [P] [US1] `src/components/settings/FileIconSetPreview.vue`: draws a sample folder with the typical entries (folder, image, video, music, pdf, text, code, archive) through the same `useFileIcons` machinery and `files_icons_resolve` for a given set (not only the chosen one); the sample queries are a constant
- [x] T039 [US1] `src/components/settings/FileIconSetSetting.vue` and `src/components/settings/AppearanceView.vue`: a `SettingsGroup` in `radio` mode with one `SettingsOptionRow` per set from `file_icon_sets_list` (name, author and license, the preview of T038); choosing saves at once through `set_pref` (like `ColorSchemeSetting.vue`: `savedFlash` "Gespeichert.", `opError`, no save button); "Zurücksetzen" sets `builtin:lucide` (reuse `AppearanceResetButton.vue`'s dialog pattern); an `unknown` state shows "braucht eine neuere App-Version" (the other states come in US4); add the keys to `settingKeys` of `general.appearance` in `src/lib/settings/registry.ts`
- [x] T040 [US1] `src/i18n/locales/de.json` and `en.json`: texts of the group (`settings.iconSet.*`: label, description, saved, reset, the states, the preview title); the term is "Icon-Set", never "Theme" or "Layout"

**Checkpoint**: T031 and T032 pass; a user can switch between Lucide, Phosphor and Material.

---

## Phase 5: User Story 3 - Ein eigenes Set importieren (Priority: P2) (PR C)

**Goal**: A folder, an archive or a VSIX becomes a custom set; scripts are removed; problems are listed.

**Independent Test**: quickstart §2 steps 4–5 and the Rust tests of the fixtures.

### Tests for User Story 3

- [ ] T041 [P] [US3] Fixtures under `src-tauri/tests/fixtures/icon_sets/`: `theme-basic/` (valid, with a block `light`), `theme-vsix/` (layout `extension/package.json` with `contributes.iconThemes`), `evil.svg` (script, `onload`, `foreignObject`, external `href`, `url(http://…)`), `bad-paths.zip` (`../x.svg` and an absolute path), `bomb.zip` (ratio over 200), `outside-ref/` (manifest path leaves the folder), `corrupt.png`, `big.png` (over 512 px), `no-manifest/` (no `icon-theme.json`); generate the binary ones with a documented script or a test helper, no secrets, no local paths
- [ ] T042 [P] [US3] `src-tauri/src/files/icons/import_tests.rs`: a folder and the same content as a ZIP give the same report; a VSIX finds its theme through `package.json`; name, author and license come from `package.json`, else from the manifest name or the file name; `evil.svg` is stored sanitized (the stored bytes contain none of the dropped parts); a bad or absolute path, a symlink and a directory entry are refused with the reason; the bomb is refused before anything is written; the outside reference is reported as not taken and the rest is taken; a corrupt or too large image is left out with file name and reason and the roles on it fall back (US3 scenario 4); no manifest or no usable image writes nothing and says why in one sentence; "same name exists" is reported in `sameName`; at most 10 custom sets (the eleventh is refused); the preview writes nothing (row count unchanged) and the import writes exactly one row in one transaction; `replace` writes the new row and then deletes the old one; each limit of data-model.md is hit by one test
- [ ] T043 [P] [US3] `src-tauri/src/files/icons/commands_import_tests.rs`: an `IconSource::Folder` on a platform with `folder_pick = false` is `InvalidInput`; the two steps read the source twice and the second check wins when the source changed in between; the answer never carries a path of the source beyond its name

### Implementation for User Story 3

- [ ] T044 [US3] `src-tauri/src/files/icons/import.rs`: read a folder (`walkdir`, no symlinks, no escape), a ZIP (through `pack.rs`) or a VSIX (a ZIP with `extension/`), find the manifest (`icon-theme.json` at the root or via `package.json` `contributes.iconThemes[0].path`), take name, author, license and source from `package.json` or the manifest, check every image (`check_raster`, `sanitize_svg`), build the `ImportReport` with `skipped` (file and reason), normalize the manifest, write the pack with `pack_dir`/`pack.rs` and the row through `store.rs`; the limits "at most 2000 images, 256 KiB per image, 5 MiB unpacked, 10 custom sets" (T042 passes)
- [ ] T045 [US3] `src-tauri/src/files/icons/commands.rs` (split into `commands_import.rs` if it passes 400 lines): `file_icon_set_import_preview(source)` and `file_icon_set_import(source, replace)` after the pattern of `extension_install_preview` / `extension_install` (`src-tauri/src/extensions/commands/install.rs`); `IconSource::File` takes a `PickedFile` (`files/picked.rs`, `content://` on Android); register in `lib.rs`; regenerate bindings (`ImportReport.ts`, `IconSource.ts`) as in T023
- [ ] T046 [US3] `src/components/settings/FileIconSetImport.vue`: "Eigenes Icon-Set hinzufügen" with `usePickedFile` (no file filter on Android) and, only when `useDeviceCapabilities().folderPick !== false`, "Ordner wählen"; step 1 shows the report (name, number of images, size, the skipped files with reason, the notice "Du bist für die Lizenz dieses Sets verantwortlich" and its author and license if it names them, and, when `sameName` is set, a confirmation to replace); step 2 imports; errors show their one sentence; wire it into `FileIconSetSetting.vue`
- [ ] T047 [P] [US3] `src/i18n/locales/de.json` and `en.json`: texts of the import (`settings.iconSet.import.*`: choose file, choose folder, report, skipped, license notice, replace, errors); plus the data-testids `settings-icon-set-import`, `settings-icon-set-import-confirm`
- [ ] T048 [US3] Extend `scripts/e2e/scenarios/files-icon-sets.test.ts` with steps 4 and 5 of quickstart §2 (import the fixture folder through the file input like `BackgroundSetting`, see the report and the preview, import, see the set in the list with a preview, the script in the evil SVG has no effect and the stored image is the clean one, remove with confirmation, the choice falls back to Lucide)

**Checkpoint**: T042, T043 and T048 pass; an imported VS Code theme is chosen and shown.

---

## Phase 6: User Story 4 - Auswahl und eigene Sets auf allen Geräten (Priority: P2) (PR D)

**Goal**: The choice and the custom sets reach every device of the vault; a set never shows half; a missing or broken set falls back.

**Independent Test**: quickstart §3 and the Rust sync test.

### Tests for User Story 4

- [ ] T049 [P] [US4] `src-tauri/src/sync/icon_set_sync_tests.rs` (pattern of `large_preference_tests.rs` and `inbound_tests.rs`): a row of `haex_file_icon_sets` with a pack of about 1 MiB on replica A arrives on replica B whole or not at all (apply it in parts, the row appears only after the last part); a delete on A leaves a marker and removes the row on B; two devices that write `files.iconSet` at the same time end with the same value on both; a pack changed in transit (flipped byte) is `damaged` on B and the choice shows the fallback
- [ ] T050 [P] [US4] `src-tauri/src/files/icons/store_tests.rs`: the four states of data-model.md for `selection()`: `ready`, `syncing` (row missing, no marker), `missing` (row missing, marker present), `damaged` (wrong `sha256` or unreadable pack), plus `unknown` (`builtin:future`); in each non-`ready` state `files_icons_resolve` answers with Lucide
- [ ] T051 [P] [US4] `scripts/e2e/scenarios/files-icon-sets-two-devices.test.ts` after `appearance-sync-two-devices.test.ts` (quickstart §3): laptop and phone of one vault; import and choose on the laptop, the phone shows the set after the sync and, until then, Lucide with "wird synchronisiert"; both devices choose different sets at the same time and end equal; removing on the laptop removes it on the phone

### Implementation for User Story 4

- [ ] T052 [US4] `src/components/settings/FileIconSetSetting.vue`: show the states `syncing` ("wird synchronisiert"), `missing` ("Set wurde entfernt"), `damaged` ("Set beschädigt" with "Entfernen") and `unknown` next to the chosen set, from `file_icon_sets_list`; keep the Lucide fallback in the lists meanwhile; texts in `de.json` and `en.json`
- [ ] T053 [US4] Check that a delete of a custom set which is the chosen one on another device ends in `builtin:lucide` everywhere: `file_icon_set_remove` (US5, T058) writes the preference in the same call, and the receiving devices show `missing` until it arrives; if the order of the two changes can leave a device in `missing` for good, make `selection()` treat `missing` as the fallback without writing, and cover it in T050

**Checkpoint**: T049–T051 pass; the choice and the sets are the same on both devices.

---

## Phase 7: User Story 5 - Eigene Sets entfernen und Herkunft einsehen (Priority: P3)

**Goal**: A custom set can be removed; the origin of every set is visible; one page lists the licenses.

**Independent Test**: remove a set (gone on all devices); open "Lizenzen" and find every bundled set with author, SPDX, link and text.

### Tests for User Story 5

- [ ] T054 [P] [US5] `src-tauri/src/files/icons/commands_remove_tests.rs`: `file_icon_set_remove` deletes the row and, if it was chosen, writes `builtin:lucide` in the same call; a bundled set cannot be removed (`removable = false` in the list and an error for a `builtin` id); an unknown id is `InvalidInput`
- [ ] T055 [P] [US5] `src-tauri/src/files/icons/licenses_tests.rs` (SC-008): `file_icon_set_licenses` lists Lucide, Phosphor and Material; every entry has author, an SPDX id from the allowed list (`MIT`, `ISC`, `Apache-2.0`, `CC0-1.0`), a source address and a non-empty text; the Material entry also carries the Apache-2.0 text and the note on marks; Lucide carries both texts (ISC and MIT); no entry has a copyleft license; no entry names a local path

### Implementation for User Story 5

- [ ] T056 [US5] `src-tauri/src/files/icons/commands.rs`: `file_icon_set_remove(id)`: one `DELETE` statement (a delete of one row, no child rows), then `files.iconSet` to `builtin:lucide` when the removed set was chosen; register in `lib.rs`
- [ ] T057 [P] [US5] `src/components/settings/LicensesView.vue`: a page "Lizenzen" that lists the sets of `file_icon_set_licenses` (name, author, SPDX, link, the full text in a closable block, the note on marks for Material); register it as a sub view of "Allgemein" in `src/lib/settings/registry.ts` and in `SETTINGS_VIEWS` of `src/components/wm/appRoutes.ts`; texts in `de.json` and `en.json`
- [ ] T058 [US5] `src/components/settings/FileIconSetSetting.vue`: per custom set the details (name, author, license, source as far as the set names them) and "Entfernen" with a `ShadcnAlertDialog` confirmation (pattern of `AppearanceResetButton.vue`); no "Entfernen" for bundled sets; texts in `de.json` and `en.json`
- [ ] T059 [US5] Extend `scripts/e2e/scenarios/files-icon-sets.test.ts` with the licenses page (every bundled set with author and SPDX) and the details of a custom set

**Checkpoint**: T054, T055 and T059 pass.

---

## Phase 8: Polish & Cross-Cutting

- [ ] T060 [P] FR-016 guard: a test that the Tauri commands of `files/icons/` and the key `files.iconSet` are not reachable from the extension bridge and not offered as an agent action (the existing guard tests of the bridge and of `settingsActions`); `settings.get` must not return `files.iconSet` and no `settings.*` action may set it; if the bridge can read arbitrary preference keys, restrict `files.iconSet` there (the open question of research R4)
- [ ] T061 [P] SC-004: measure on the target machine and write the numbers into research R13: resolving 200 visible entries (goal under 20 ms), opening the Material pack (goal under 100 ms), opening a folder of 1000 entries with Material against Lucide (difference under 200 ms); if a goal is missed, fix the cause in `pack.rs` or `useFileIcons.ts`, not the goal
- [ ] T062 [P] Check every new Rust file in `src-tauri/src/files/icons/` and `migrations_icon_sets.rs` and every new Vue file against the 500-line rule (`wc -l`); a file over it needs a documented reason and a split plan (constitution); `migrations.rs` (649 lines, documented exception) only gets the registration
- [ ] T063 [P] Mark the tasks done in this file; set the status of the 053 row in `plans/README.md`
- [ ] T064 Full local parity before each PR: `cargo fmt --check`, `pnpm lint:rust` (clippy, both feature sets, `-D warnings`), the targeted `cargo test -j 4` of `files::icons`, `identity::migrations_icon_sets`, `sync::icon_set_sync_tests` and `storage::preferences_commands`, `pnpm check:file-icons`, `pnpm check:files`, `pnpm check:settings`, `pnpm typecheck`, `pnpm typecheck:scripts`, `pnpm lint`, `pnpm format:check`, `pnpm check:templates`, and the e2e scenarios `files-icon-sets` and `files-icon-sets-two-devices` (the second needs the multi-device rig; say so in the PR if it was skipped); run them in the devShell
- [ ] T065 Manual quickstart §4 by the operator: Android (import of an archive through the file picker, "Ordner wählen" hidden, the group usable, images shown), the license page against `SOURCES.md`, and the performance check; open until done, like T081 of spec 024
- [ ] T066 Tell the operator once at the end: holzi has no LICENSE file (AGPL-3.0-or-later is only in `src-tauri/Cargo.toml`), and the Material pack ships with the logos it contains under the risk the operator accepted (FR-017)

---

## Dependencies & Execution Order

- Phase 1 (docs) is PR #443. Phase 2 blocks everything else: T011 before T014–T022; T015, T016, T018, T019 are independent; T017 needs T015 and T016 (images in a pack); T020 needs T018 and T019; T021 needs T011, T014 and T017; T022 needs T020 and T021; T024 → T025 → T026 need T017 (`pack_dir`) and T018.
- US2 (Phase 3) needs Phase 2 only. US1 needs Phase 2 and the packs of T026; it can be developed in parallel with US2.
- US3 needs Phase 2 (`pack.rs`, `svg.rs`, `raster.rs`, `store.rs`) and the settings group of US1 (T039) for the dialog. US5 needs Phase 2 and US1's group (T039) for the details and the remove button. US4 needs Phase 2, US1 and `file_icon_set_remove` of US5 (T056) for the e2e (T051, T053).
- Within a story: tests first (they fail), then implementation, then UI, then e2e.
- Order of PRs: A (Phase 2 + US2), B (US1 + T057 of US5), C (US3 + T056, T058, T059 of US5), D (US4 + Polish), each on top of the merged one. After PR #443 merges, branch each PR from `main` (a stacked PR merged after its base lands goes into the dead base branch).

### Parallel Opportunities

- Phase 2 tests T004–T010 together (seven files); T012 and T013 while T011 is written; T015, T016, T018 and T019 together (four files).
- US2: T027 and T028 together.
- US1: T031 and T032 together; T033, T034 and T038 together (three files).
- US3: T041, T042 and T043 together; T047 while T045 is written.
- US4: T049, T050 and T051 together.
- US5: T054 and T055 together; T057 and T058 together.
- Polish: T060–T063 together.

---

## Implementation Strategy

### MVP First

1. Phase 2 and US2 (PR A): stop and run `cargo test -j 4 --lib files::icons`; the backend resolves for Lucide, Phosphor and Material.
2. US1 (PR B): the user can choose a bundled set and the lists show it. This alone delivers the wish "exchangeable icons with two or three sets shipped".
3. US3 adds the user's own sets, US4 the same choice and sets on every device, US5 removing and the licenses. Each can ship alone after US1 (US4 after US3).

### Notes

- The packs are built from the checked-in source directories by the same Rust code the import uses; a change to a bundled image is a diff in `src-tauri/icon-sets/<set>/`, and the checked-in ZIP is a reproducible result of it (T026).
- The migration number `0036` and the trigger version are checked against `main` when T011 is written; parallel branches may take them.
- A set is immutable: replacing it is a new row and a delete of the old one; there is no update path and no merge of two versions of one set (data-model.md).
- Commit messages: Conventional Commits, no agent attribution (constitution); merge, not squash.
