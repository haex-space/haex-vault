# Implementation Plan: A coloured file icon set from theSVG

**Branch**: `103-file-icon-set-thesvg` (worktree branch `feat/icon-set-brand-logos`) | **Date**: 2026-11-28 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/103-file-icon-set-thesvg/spec.md`

## Summary

A fourth built-in file icon set, "theSVG", built from the fill look of thesvg's own UI icons (MIT),
coloured by kind at build time. It follows Phosphor and Material exactly: a source directory built by
`prepare.ts` from a pinned clone, packed with `pack_dir`, embedded with `include_bytes!`, listed by
`infos()`, shown on the existing "Lizenzen" page through the pack's `LICENSE` and `NOTICE`.

Approach (reasons and alternatives in [research.md](./research.md)):

- **UI icons, not logos** (R1): original MIT drawings, no trademark question, the roles a set needs exist.
- **Fill look, coloured** (R3): `currentColor` → a fixed colour per kind (Material's palette); a luminance
  range check keeps every image readable on both schemes, so there are no light images.
- **Mapping like Phosphor's** (R4): a checked-in extension/name → slug list (about 100) and a role table.
- **No second sanitiser** (R5): the Rust pack checks stay the only ones.
- **One enum variant** (R6): `BuiltinSet::Thesvg`, i18n label, regenerated binding; spec 053 gets only a
  line in its list of sets (R7).
- **`prepare.ts` split** (R8) so no file passes 500 lines.

## Technical Context

**Language/Version**: Rust (Tauri 2.x as `src-tauri/Cargo.toml`); TypeScript run with `node` (as `prepare.ts`); Vue 3 / Nuxt 4 for the label

**Primary Dependencies**: none new (Node standard library in scripts; `zip`, `quick-xml`, `image` already in Rust)

**Storage**: none new; pack `src-tauri/icon-sets/thesvg.zip` + source directory `thesvg/`; the choice stays the vault preference `files.iconSet`

**Testing**: `nix develop --command cargo test -j 4` targeted (`files::icons`, `storage::preferences_commands`, `sync::icon_set_sync`); `node --test scripts/check-thesvg-icons.ts`; e2e `files-icon-sets` (four sets)

**Target Platform**: desktop and Android as the other sets; iOS deferred

**Project Type**: Desktop/mobile app (Tauri + Nuxt SPA) with a build-time script

**Performance Goals**: a 1000-entry folder opens no slower than with Material (SC-004); pack of about 25 images

**Constraints**: no new dependency; no local path in checked-in files; images only as images (spec 053 FR-010); files under 500 lines

**Scale/Scope**: about 25 images, about 100 mapped extensions/names

## Constitution Check

_GATE: before Phase 0; re-checked after Phase 1._

| Principle / MUST                                       | Status                                                                                                                                |
| ------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------- |
| Changes in a dedicated worktree on a topic branch      | met (`.worktrees/icon-set-brand-logos`, `feat/icon-set-brand-logos`)                                                                  |
| Primary landing follows the speckit workflow           | met (specify, plan, tasks, implement)                                                                                                 |
| Look for existing code before creating named artefacts | met; `graphify-out/` is not in the worktree, so the search was `grep` over `files/icons/`, `prepare.ts`, settings, i18n (table below) |
| Tests in their own files                               | `thesvg_tests.rs` next to `builtin.rs`; `scripts/check-thesvg-icons.ts`                                                               |
| 500-line limit                                         | `prepare.ts` (417 lines) is split first (R8)                                                                                          |
| One implementation per rule                            | sanitiser stays `svg.rs`; the license rule stays spec 053 FR-017; resolution untouched                                                |
| No new dependency, no speculative abstraction          | met: one variant, no command, no page                                                                                                 |
| Errors not swallowed; no `unwrap` on input             | build fails with the icon name; Rust gains a variant and table rows                                                                   |
| Input checked at trust boundaries                      | the clone is untrusted input: commit pin, slug, colour and image checks                                                               |
| ADR for core-principle decisions                       | none needed                                                                                                                           |
| Conventional Commits, no squash, no agent attribution  | applies to implementation                                                                                                             |
| No secrets, no local paths in the repository           | met                                                                                                                                   |
| Specs in English                                       | met                                                                                                                                   |

Re-checked after Phase 1: no violation; nothing under Complexity Tracking.

### Candidates checked

| Candidate                                                                                            | Result                                               |
| ---------------------------------------------------------------------------------------------------- | ---------------------------------------------------- |
| `files/icons/builtin.rs`, `mod.rs` (`BuiltinSet`, `infos()`, `licenses()`, `pack()`, `ALL`, `key()`) | one variant and one table row each                   |
| `files/icons/pack.rs` (`pack_dir`, limits, `license_text`)                                           | reused unchanged                                     |
| `scripts/icon-sets/prepare.ts` (`addStateIcons`, helpers, Phosphor mapping pattern)                  | helpers moved to `common.ts`; mapping pattern copied |
| `scripts/icon-sets/haex-states/`                                                                     | reused: the five state images join the pack          |
| `builtin_tests.rs`, `licenses_tests.rs`, `bundled_tests.rs`                                          | extended to the fourth set                           |
| `storage/preferences_commands_tests.rs`, `files/icons/store_tests.rs`, `sync/icon_set_sync_tests.rs` | one `builtin:thesvg` case each                       |
| `FileIconSetSetting.vue`, `fileIcons.ts`, `BuiltinSet.ts`, `LicensesView.vue`                        | label key and union member; page unchanged           |
| `scripts/check-file-icons.ts`, `package.json` (`check:file-icons`)                                   | pattern for `check-thesvg-icons.ts`                  |
| `scripts/e2e/scenarios/files-icon-sets.test.ts`                                                      | list of sets gains theSVG                            |

## Project Structure

### Documentation

```text
specs/103-file-icon-set-thesvg/
├── plan.md, research.md, data-model.md, quickstart.md, tasks.md
└── checklists/requirements.md
```

No `contracts/`: no new command, event or format; the interface change is the value `builtin:thesvg`
and the `BuiltinSet` union (data-model.md).

### Source Code

```text
src-tauri/
├── icon-sets/
│   ├── thesvg/                     # NEW: source directory (manifest, icons/, LICENSE, NOTICE)
│   ├── thesvg.zip                  # NEW: packed, include_bytes!
│   ├── thesvg.mapping.json         # NEW: extension/name -> slug
│   ├── thesvg.colors.json          # NEW: slug -> colour
│   ├── thesvg.source.json          # NEW: repository + pinned commit
│   └── SOURCES.md                  # + section "theSVG"
└── src/files/icons/
    ├── mod.rs                      # BuiltinSet::Thesvg, ALL, key
    ├── builtin.rs                  # THESVG_ZIP, pack(), zip_bytes(), display_name()
    ├── builtin_tests.rs            # third pack in the build and invariants tests
    ├── thesvg_tests.rs             # NEW: roles, extensions, states, both schemes, fallback
    └── licenses_tests.rs           # four sets
scripts/
├── icon-sets/common.ts             # NEW: helpers moved out of prepare.ts
├── icon-sets/thesvg.ts             # NEW: build of the set
├── icon-sets/prepare.ts            # dispatches `thesvg`
├── check-thesvg-icons.ts           # NEW: node --test
└── e2e/scenarios/files-icon-sets.test.ts
src/
├── lib/files/fileIcons.ts, types/bindings/BuiltinSet.ts, i18n/locales/{de,en}.json
package.json                        # + check:thesvg-icons
specs/053-file-icon-sets/spec.md    # FR-007 list gains theSVG
plans/README.md                     # row for 103
```

**Structure Decision**: everything extends the existing icon-set module; new files are the set's data,
its build script and its tests.

### Pull requests

1. **A – Docs**: this directory, the 053 list entry, the README row.
2. **B – The set**: build script split, thesvg build, data files, pack, Rust variant, tests, i18n, e2e
   list. One PR: the pack and the variant must land together.

## Complexity Tracking

No violations.
