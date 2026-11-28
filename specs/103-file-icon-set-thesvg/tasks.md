---
description: 'Task list for spec 103-file-icon-set-thesvg'
---

# Tasks: A coloured file icon set from theSVG

**Input**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md), [data-model.md](./data-model.md), [quickstart.md](./quickstart.md)

**Tests**: Included (constitution: one runnable check for non-trivial logic; SC-002, SC-003, SC-005).

**Organization**: by user story. The build helpers and the Rust variant are shared and sit in Phase 2. US1 (P1) is the MVP; US2 (the license text) is part of the same PR because a bundled set must not ship without it; US3 hardens the build.

**Shipping note**: PR A = Phase 1 (docs); PR B = Phase 2 + US1 + US2 + US3 + Polish.

**Rules for every task**: no new dependency; one implementation per rule (sanitising only in `svg.rs`, the license rule only in spec 053 FR-017); never edit `src-tauri/icon-sets/thesvg/` or `thesvg.zip` by hand; no local path in any checked-in file; files under 500 lines; run cargo only as `nix develop --command cargo … -j 4` and only targeted tests; no agent attribution in commits.

## Format: `[ID] [P?] [Story] Description`

---

## Phase 1: Setup (PR A, docs)

- [x] T001 In `specs/053-file-icon-sets/spec.md` add "theSVG" to the list of bundled sets in FR-007 and a Clarifications line "2026-11-28: a fourth bundled set, theSVG, is added by spec 103; FR-017 is unchanged (MIT)"; keep the file's language style
- [x] T002 [P] Add a row for 103 to `plans/README.md` after the 053 row: "103: Buntes Datei-Icon-Set aus theSVG (`specs/103-file-icon-set-thesvg/`)", priority P3, effort S, status "Spezifiziert und geplant 2026-11-28", dependency 053
- [x] T003 (committed as 316200710; the PR is opened when the whole work is ready) Run `nix develop --command pnpm exec prettier --write specs/103-file-icon-set-thesvg specs/053-file-icon-sets plans/README.md`, commit the docs (including `.specify/feature.json`) as `docs(103): …` and open PR A

---

## Phase 2: Foundational (PR B)

### Tests for the foundation

- [x] T004 [P] `scripts/check-thesvg-icons.ts` (`node --test`, pattern of `scripts/check-file-icons.ts`): `luminance('#42a5f5')` ≈ 0.347; `colorProblem` accepts every colour of research R3 and refuses `#000000`, `#ffffff`, `#fafafa`, `#101010` and a non-`#rrggbb` string, with the range "relative luminance between 0.10 and 0.65" (FR-005)
- [x] T005 [P] same file: `checkMapping(mapping, source)` — a slug missing in the source fails naming it; a duplicate key (after lower-casing) fails; every mapped or role slug needs a colour or the check fails naming the slug (data-model.md rules 1 to 3)

### Implementation of the foundation

- [x] T006 Extract the shared helpers of `scripts/icon-sets/prepare.ts` (`readJson`, `writeJson`, `fail`, `freshDir`, `isRecord`, `readString`, `readStringMap`, `mapValues`, `SETS_DIR`, `ROOT`, the `Roles`/`Origin` types, `STATE_IMAGES`, `STATES_DIR`, `addStateIcons`) into `scripts/icon-sets/common.ts` and import them in `prepare.ts`; behaviour unchanged (`node scripts/icon-sets/prepare.ts states` leaves `git status` clean)
- [x] T007 [P] `scripts/icon-sets/thesvg.ts`: the pure parts — `luminance`, `colorProblem` (T004), `checkMapping` (T005), `colorize(svg, color)` (replaces every `currentColor` with the colour, changes nothing else, fails when there is none); exported for the tests
- [x] T008 `src-tauri/src/files/icons/mod.rs`: add `BuiltinSet::Thesvg`, `ALL = [Lucide, Phosphor, Material, Thesvg]`, `key()` → `"thesvg"`; update the doc comment (default stays Material); regenerate `src/types/bindings/BuiltinSet.ts` with the project's ts-rs step (`"lucide" | "phosphor" | "material" | "thesvg"`)
- [x] T009 [P] `src/lib/files/fileIcons.ts`: add `'thesvg'` to the builtin `name` union; `scripts/check-file-icons.ts`: assert `selectionValue` gives `builtin:thesvg`

---

## Phase 3: User Story 1 - Choose a fourth, coloured icon set (P1) 🎯 MVP

**Goal**: "theSVG" is selectable and shows coloured images of every kind. **Independent test**: quickstart §3 and §4 steps 1 to 3.

### Tests for US1

- [x] T010 [P] [US1] `src-tauri/src/files/icons/thesvg_tests.rs` (declared next to `builtin.rs` with `#[path]`, helpers of `bundled_tests.rs`): `rs`, `zip`, `pdf`, `json`, `md`, `png`, `mp4`, `mp3`, `txt` and the name `readme` resolve to an `IconAnswer::Image` with a `.svg` id in both schemes; a folder resolves to the folder image; an unknown extension to the set's generic file image; the five states resolve to the `haex-*` images and are never taken by a name or extension
- [x] T011 [P] [US1] same file: at least 90 entries in `fileExtensions` plus `fileNames` (SC-001); every mapped value is an id of `iconDefinitions`; every path exists in the pack; every colour found in the images' `fill` attributes lies in the luminance range 0.10 to 0.65 except the state images; the manifest has no `light` block
- [x] T012 [P] [US1] `src-tauri/src/files/icons/builtin_tests.rs`: extend `build_bundled_packs`, `every_bundled_pack_passes_the_checks` and `every_bundled_pack_is_what_its_source_directory_packs_to` to `thesvg`; extend `the_list_names_the_three_sets_and_none_can_be_removed` to four sets (rename), theSVG last and not removable
- [x] T013 [P] [US1] `src-tauri/src/storage/preferences_commands_tests.rs` (line ~154): `("files.iconSet", "builtin:thesvg")` is accepted; `src-tauri/src/files/icons/store_tests.rs`: `builtin:thesvg` parses to `BuiltinSet::Thesvg`; `src-tauri/src/sync/icon_set_sync_tests.rs`: (no sync case: the existing two-device test already proves that a built-in choice reaches the second device; the value is validated by `validate_value`)
- [x] T014 [P] [US1] `scripts/e2e/scenarios/files-icon-sets.test.ts`: the list holds `builtin:thesvg` after Material (line ~55, the step texts and the names list at line ~249); choosing theSVG shows an image for a `.rs` file; the choice survives reopening; assertion messages carry what was seen (e2e runs only on main CI)

### Implementation for US1

- [x] T015 [US1] Create `src-tauri/icon-sets/thesvg.source.json` (`{"repository":"https://github.com/glincker/thesvg","commit":"a417e5a090a1258d727f5837f1413788b6337845"}`), `thesvg.colors.json` (the table of research R3, keyed by slug: `folder`, `file`, `image`, `video`, `audio-lines`, `pdf-file`, `file-text`, `file-code`, `archive-zip`, `json-file`, `markdown-file`, `config-file`, `env-file`, `log-file`, `lockfile`, `license-file`, `readme-file`, `presentation-file`, `spreadsheet`) and `thesvg.mapping.json` (extension or name → slug; seed from `phosphor.mapping.json`, map `file-pdf`→`pdf-file`, `file-doc`/`file-txt`→`file-text`, `file-xls`/`file-csv`→`spreadsheet`, `file-ppt`→`presentation-file`, `file-zip`→`archive-zip`, `file-md`→`markdown-file`, `file-ini`→`config-file`, `file-audio`→`audio-lines`, `file-video`→`video`, every `file-<language>`, `file-code`, `file-html`, `file-css`, `file-sql` → `file-code`, images → `image`; add names `readme`, `license`, `.env`, `package-lock.json`, `pnpm-lock.yaml`, `cargo.lock`, `dockerfile`, `.gitignore` → the matching slugs; about 100 entries; keys lower case)
- [x] T016 [US1] `scripts/icon-sets/thesvg.ts`: the build `prepareThesvg(clone)` — check the clone's HEAD equals `thesvg.source.json`'s commit; read `src/data/ui/looks/fill.json`; apply `checkMapping`; write each used slug as `icons/<slug>.svg` with `colorize`; write `src-tauri/icon-sets/thesvg/icon-theme.json` per data-model.md (`file`, `folder`, `fileExtensions`, `fileNames`, `x-haex.roles` from the role table plus `addStateIcons('thesvg')`, `x-haex.origin` = author `thesvg.org`, license `MIT`, source `https://github.com/glincker/thesvg`)
- [x] T017 [US1] `scripts/icon-sets/prepare.ts`: dispatch `thesvg <clone>` (clone path also from `ICON_SETS_THESVG`), update the usage text and header comment, add `thesvg` to the `states` command
- [x] T018 [US1] Run `nix develop --command node scripts/icon-sets/prepare.ts thesvg <clone at the pinned commit>` and commit `src-tauri/icon-sets/thesvg/`
- [x] T019 [US1] `src-tauri/src/files/icons/builtin.rs`: `THESVG_ZIP = include_bytes!("../../../icon-sets/thesvg.zip")`, `THESVG` lazy pack, `pack()`, `zip_bytes()`, `display_name()` → `"theSVG"`, doc comment; build the ZIP with the ignored test (`nix develop --command cargo test -j 4 files::icons::builtin::tests::build_bundled_packs -- --ignored`) and commit `thesvg.zip`
- [x] T020 [P] [US1] `src/i18n/locales/en.json` and `de.json`: `settings.iconSet.sets.thesvg` = "theSVG" next to `lucide`, `phosphor`, `material` (both files in step)
- [x] T021 [US1] Run T010–T013 (`nix develop --command cargo test --manifest-path src-tauri/Cargo.toml -j 4 files::icons`, `nix develop --command cargo test --manifest-path src-tauri/Cargo.toml -j 4 storage::preferences_commands`, and `nix develop --command cargo test --manifest-path src-tauri/Cargo.toml -j 4 sync::icon_set_sync`) and `node --test scripts/check-thesvg-icons.ts scripts/check-file-icons.ts`; fix findings

**Checkpoint**: theSVG can be chosen and works.

---

## Phase 4: User Story 2 - See the source and license (P2)

**Independent test**: quickstart §4 step 4.

- [x] T022 [P] [US2] `src-tauri/src/files/icons/licenses_tests.rs`: `the_page_lists_lucide_phosphor_and_material_in_full` → four sets with theSVG's text non-empty and containing "MIT License" and "thesvg.org"; `every_license_is_open_and_none_is_copyleft` includes theSVG (MIT); `the_texts_are_those_the_licenses_ask_for` checks the MIT permission sentence for theSVG; `nothing_names_a_path_of_this_machine` covers it
- [x] T023 [US2] `scripts/icon-sets/thesvg.ts`: copy `LICENSE` of the clone to `thesvg/LICENSE` and write `thesvg/NOTICE` (source address, pinned commit, "the drawings are by thesvg.org (MIT) and were coloured when this set was built"); rebuild (T018) and re-pack (T019)
- [x] T024 [US2] Run `nix develop --command cargo test -j 4 files::icons`; fix findings

---

## Phase 5: User Story 3 - Rebuild from the pinned source (P3)

**Independent test**: quickstart §1 and §2.

- [x] T025 [P] [US3] `scripts/check-thesvg-icons.ts`: `prepareThesvg` on a temporary fake clone (outside the repository) fails for a mapping slug that is missing (names it), for a HEAD at another commit (names both), for a colour out of range (names slug and colour), and for an SVG without `currentColor`; and succeeds on a valid fake clone
- [x] T026 [US3] (directory rebuilt twice with an identical hash; the Phosphor and Material ZIPs came out byte-identical when `build_bundled_packs` ran, so the packer is deterministic) Reproducibility: run the build and the ZIP build twice; `git status` shows no change and `sha256sum src-tauri/icon-sets/thesvg.zip` is equal (SC-003); record the commands and the result in the PR text
- [x] T027 [P] [US3] `package.json`: `check:file-icons` now also runs `scripts/check-thesvg-icons.ts`, so CI (`.github/workflows/ci.yml` runs `pnpm check:file-icons`) needs no change

---

## Phase 6: Polish

- [x] T028 [P] `src-tauri/icon-sets/SOURCES.md`: add "theSVG (`thesvg/`)" — source address, commit SHA, the command, license MIT (Copyright (c) 2025 thesvg.org), the look (fill), the colouring rule and table, the counts; add the new files to the header; no local path
- [ ] T029 (dark and light checked by rendering all images, 2026-11-28; only the phone is open) Look at the set on both colour schemes (quickstart §4 step 3) and on an Android phone (step 5); adjust `thesvg.colors.json` if a colour looks wrong, rebuild; the phone check stays open until a device is at hand
- [x] T030 Run ESLint and Prettier in the project's devShell, the project's typecheck, `node --test scripts/check-thesvg-icons.ts scripts/check-file-icons.ts`, and the three separate Cargo test invocations in T021; fix findings
- [x] T031 Tick this file, update the 103 row in `plans/README.md` ("umgesetzt …"), note the open phone check in the PR text
- [x] T032 Give the places of the side bar (home, downloads, pictures, documents, desktop, videos, music, public, templates, projects) their own coloured images in theSVG, matched by folder name like Material; test in `src-tauri/src/files/icons/thesvg_tests.rs`
- [x] T033 One mapping for every set (R7): `src-tauri/icon-sets/kinds.json` (extensions, names, folder names in four languages → kinds with parents), `thesvg.kinds.json` and `phosphor.kinds.json` (kind → image), `scripts/icon-sets/kinds.ts` used by `prepare.ts` (Phosphor, Material folder names) and `thesvg.ts`, tests in `scripts/check-icon-kinds.ts`; remove `thesvg.mapping.json` and `phosphor.mapping.json`; rebuild all packs

---

## Dependencies and order

- Phase 1 first; Phase 2 blocks the stories. T004–T005 before T007; T006 before T007 and T016; T008 before T009 and every Rust task.
- US1: tests T010–T014 can be written first (they fail until T015–T019 land); T015 → T016 → T018 → T019 are sequential; T020 is independent.
- US2 needs US1's pack (T023 rebuilds it); US3 needs T016.

## Parallel examples

- After T008: T004, T005, T009 and T010–T014 touch different files.
- During US1: T020 with T016.

## Implementation strategy

MVP = US1 + US2 (shippable only together): choose "theSVG", see coloured images, see the MIT text. US3 can follow in the same PR. Look at the result on both schemes (T029) before the PR leaves draft.
