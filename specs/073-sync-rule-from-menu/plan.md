# Implementation Plan: Create a sync rule from the file browser's context menu

**Branch**: `073-sync-rule-from-menu` | **Date**: 2026-11-10 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/073-sync-rule-from-menu/spec.md`

## Summary

The context menus of the file browser get one entry, "Sync-Regel erstellen…", for one folder entry, one file entry
and the open folder. It opens the existing sync rule dialog with that place as the source and nothing else filled in.
Spec 074 is merged, so a file is a valid source and the entry covers files from the start.

The change is frontend only. The engine, the commands, the stored rules and the bindings do not change:

- `lib/files/menus.ts` learns the command `syncRule` and a `syncRule` flag in both menu inputs;
- two pure functions decide when the entry is offered (`syncRuleOffer`) and turn an entry into the dialog's source place
  (`pickedFromEntry`), tested by the existing node checks;
- `useFilesManage` computes the flag and runs the command through an option, like `share`;
- `Panel.vue` mounts one sync rule dialog per panel, so the split view acts on the panel whose menu was opened;
- `SyncRuleDialog.vue` takes an optional `initialSource`; the list's dialog does not pass it and works as before.

Research R1 to R8 leaves no open question.

## Technical Context

**Language/Version**: TypeScript 5 with Vue 3 / Nuxt 3

**Primary Dependencies**: `vue-sonner` (toast), the existing sync rule dialog and store. No new dependency.

**Storage**: N/A (no stored data changes, see [data-model.md](data-model.md))

**Testing**: the node-based checks `pnpm check:files` (`check-files-clipboard.ts` for menus, `check-files-sync-rules.ts`
for the helpers), `pnpm typecheck`, `pnpm typecheck:scripts`, `pnpm lint`, the WebDriver E2E scenario (runs on `main`
only, so the branch is dispatched by hand before the merge)

**Target Platform**: Desktop (Linux, macOS, Windows) and Android, one code base; touch reaches the entry by a long press

**Project Type**: Desktop and mobile app (Tauri backend, Nuxt frontend)

**Performance Goals**: The menu decision is a pure function of data the panel already holds; no extra call per menu
opening.

**Constraints**: Creating a rule from the list works as before (FR-010); the same checks, rejections and saved values as
the list's dialog (SC-003); no change to sources, transfers, or the protection of holzi's own data (spec 044 FR-037)

**Scale/Scope**: 7 existing frontend files change (`menus.ts`, `registry.ts`, `syncRules.ts`, `useFilesManage.ts`,
`useFilesFolderFacts.ts`, `Panel.vue`, `SyncRuleDialog.vue`), the two locale files, two check scripts, one E2E scenario.
No Rust.

## Constitution Check

Checked against `.specify/memory/constitution.md` (v1.4.0): no secrets, no local absolute paths in committed config
(the spec's paths are examples in prose), no cross-repo reference, no instruction file changes, local work does not
depend on a relay. **Pass.** Re-checked after the design: the design adds no external source and no stored value.

## Project Structure

### Documentation (this feature)

```text
specs/073-sync-rule-from-menu/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/ui.md
├── checklists/requirements.md
└── tasks.md            # by speckit-tasks
```

### Source Code (repository root)

```text
src/
├── lib/files/
│   ├── menus.ts                  # command 'syncRule', flag in EntriesMenuInput / FolderMenuInput, item order
│   ├── registry.ts               # syncRuleOffer (next to shareableSource)
│   └── syncRules.ts              # pickedFromEntry
├── composables/
│   ├── useFilesFolderFacts.ts    # the open folder's encryption state (readable or not)
│   └── useFilesManage.ts         # the flag in menuFor, case 'syncRule', option syncRule
├── components/files/
│   ├── Panel.vue                 # the dialog, the toast, the store reload
│   └── SyncRuleDialog.vue        # prop initialSource
└── i18n/locales/{en,de}.json     # files.actions.syncRule, files.syncRules.createdFromMenu
scripts/
├── check-files-clipboard.ts      # menu cases
├── check-files-sync-rules.ts     # pickedFromEntry
└── e2e/scenarios/files-sync-rule-menu.test.ts
```

**Structure Decision**: Everything sits where the neighbouring commands (`pin`, `share`) already live; no new module.

## Complexity Tracking

No violations.
