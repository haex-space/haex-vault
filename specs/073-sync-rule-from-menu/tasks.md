# Tasks: Create a sync rule from the file browser's context menu

**Input**: Design documents from `/specs/073-sync-rule-from-menu/`
**Prerequisites**: [plan.md](plan.md), [spec.md](spec.md), [research.md](research.md), [data-model.md](data-model.md),
[contracts/ui.md](contracts/ui.md), [quickstart.md](quickstart.md)

**Tests**: Pure helpers and the menus are tested by the node checks (`scripts/check-files-*.ts`), written before the
code they check. One E2E scenario covers the flow. The change is frontend only: no Rust, no migration, no binding.

**Format**: `- [ ] T### [P?] [Story?] Description with file path`. `[P]` = a different file, no dependency on an
unfinished task.

## Phase 1: Setup

- [x] T001 Confirm the prerequisites on this branch: `git merge-base --is-ancestor origin/main HEAD` after
      `git fetch`; `src/types/bindings/SyncRuleDraft.ts` has `sourceKind` and `src/components/files/FolderPicker.vue`
      has `allowFiles` (spec 074 is merged, FR-004a holds, no flag is needed); `.specify/feature.json` stays
      uncommitted.

## Phase 2: Foundational (blocks all stories)

**Purpose**: the pieces every story uses: the command in the menus, the source place of an entry, the dialog that
takes it, the texts.

- [x] T002 [P] Write the checks of `pickedFromEntry` in `scripts/check-files-sync-rules.ts` first: a device folder
      and a device file keep their path and take `kind` `folder` / `file`; a storage folder key `Fotos/Urlaub/` becomes
      `/Fotos/Urlaub`, a storage file key `Fotos/a.jpg` becomes `/Fotos/a.jpg`, the storage root (`''`, `/`) becomes
      `/`; `encrypted` and `source` pass through unchanged.
- [x] T003 In `src/lib/files/syncRules.ts` add `pickedFromEntry(source, path, kind, encrypted)` returning the dialog's
      `PickedFolder` (research R3, data-model.md); use structural types, no bindings with nested imports
      (`typecheck:scripts`). Run `pnpm check:files` until T002 passes.
- [x] T004 [P] In `src/components/files/SyncRuleDialog.vue` add the optional prop `initialSource?: PickedFolder | null`
      (research R4): when the dialog opens without a rule, `source` starts as `initialSource` (else `null`); the
      target stays empty, direction, delete mode and interval keep their defaults, the automatic name follows the
      source as today. Extend the header comment. The list's dialog passes nothing and works as before (FR-010).
- [x] T005 [P] Write the menu cases in `scripts/check-files-clipboard.ts` first: with `syncRule: true` one entry's
      menu has `syncRule` right after `pin` (and before `select`), not disabled, no shortcut; without the flag it has
      none; with `syncRule: true` and two entries it has none; the open folder's menu has it after `pin`, and alone
      behind a separator when `pin` is not set; without the flag none.
- [x] T006 In `src/lib/files/menus.ts` add the command `'syncRule'` to `FilesCommand`, the flag `syncRule?: boolean`
      to `EntriesMenuInput` and `FolderMenuInput`, and the items as in contracts/ui.md and research R8 (one entry
      only; `folderMenu` appends a separator only when neither `pin` nor an earlier item needs it). Run
      `pnpm check:files` until T005 passes.
- [x] T007 [P] Add the keys `files.actions.syncRule` ("Create sync rule…" / "Sync-Regel erstellen…") and
      `files.syncRules.createdFromMenu` ("Sync rule “{name}” created." / "Sync-Regel „{name}“ angelegt.") to
      `src/i18n/locales/en.json` and `de.json`, next to `files.actions.pin` and in the `files.syncRules` group.

**Checkpoint**: `pnpm check:files` passes; nothing offers the entry yet.

## Phase 3: User Story 1 - Start a sync rule from a folder entry (Priority: P1) MVP

**Goal**: Right-click a folder, choose the entry, the dialog opens with that folder as source, save, stay in the
folder, see the confirmation and the rule in the list.

**Independent Test**: [quickstart.md](quickstart.md) rows US1.1 to US1.5.

- [x] T008 [P] [US1] Write the checks of `syncRuleOffer` in `scripts/check-files-registry.ts` first (the cases of
      research R2 for a folder entry): this device, an own cloud storage and `peer:<id>` offer; `share:<id>` and
      `device:<id>` do not; an entry that is holzi's own, a symlink or without access does not; a readable encrypted
      folder does, a not readable one (`otherVault`, `newerFormat`, `damaged`) does not.
- [x] T009 [US1] In `src/lib/files/registry.ts` add `syncRuleOffer(input)` next to `shareableSource` (research R2,
      data-model.md) using `isOwnStorage` / `isPeerStorage`; run `pnpm check:files` until T008 passes.
- [x] T010 [US1] In `src/composables/useFilesManage.ts` compute `syncRule` in `menuFor` for one entry
      (`syncRuleOffer` on the entry, with the panel's `source`), add the option
      `syncRule?: (place: PickedFolder) => void` and `case 'syncRule'` in `run` (it builds the place with
      `pickedFromEntry` from the first entry and calls the option). Import `PickedFolder` as a type from
      `~/components/files/FolderPicker.vue`.
- [x] T011 [US1] In `src/components/files/Panel.vue` pass the option to `useFilesManage` (it sets a `starting`
      ref to the place and opens the dialog), mount `<FilesSyncRuleDialog>` next to `FilesShareDialog` with
      `v-model:open`, `:sources="context.sources.value"`, `:initial-source="starting"`, and on `@saved` show
      `toast.success(t('files.syncRules.createdFromMenu', { name }))` and `useSyncRulesStore().loadAsync()`
      (research R5; the dialog's name is the rule's name, so the dialog's `saved` event carries it: add `name` to the
      event in T004 if the panel cannot read it otherwise). The panel stays on its folder.
- [x] T012 [US1] Verify the selection bar and the keyboard map take no `syncRule` command: read
      `src/components/files/ActionBar.vue` and `src/composables/useFilesKeyboard.ts`; neither lists menu items
      generically (research R7). If one does, exclude `syncRule` there.
- [x] T013 [US1] E2E scenario `scripts/e2e/scenarios/files-sync-rule-menu.test.ts`, modelled on
      `scripts/e2e/scenarios/files-pins.test.ts` (`contextMenu` helper, folders from `deviceFiles`): right-click a
      folder (`files-entry-<name>`), click `files-menu-syncRule`, see `files-sync-rule-dialog` with the folder as
      the source, pick a target folder, save; the file browser still shows the same folder; the rule is in the list
      (`sync_rules_list` through `instance.invoke`, as `files-sync-file-rule` does). Cancel a second dialog: the list
      does not grow.

**Checkpoint**: US1 works end to end for a folder entry on this device.

## Phase 4: User Story 3 - Start a rule for a single file (Priority: P1)

**Goal**: The same entry on a file opens the dialog as a file rule (spec 074).

**Independent Test**: [quickstart.md](quickstart.md) row US3.

- [x] T014 [US3] Extend the checks of T008 and T005 for a file entry (`syncRuleOffer` is true for a plain file,
      false for a file of a share or inside a not readable encrypted folder), then make `menuFor` and the `run` case
      pass `kind: 'file'` for a file entry (T010 already branches on the entry's kind; adjust if it assumed folders).
- [x] T015 [US3] Extend the E2E scenario of T013: right-click a file, the dialog shows the file as the source and no
      delete mode (`files-sync-rule-delete-mode` is absent), save with a target folder, `sync_rules_list` shows a rule
      with `sourceKind` `file`.

**Checkpoint**: US1 and US3 work.

## Phase 5: User Story 2 - Start a rule for the open folder (Priority: P2)

**Goal**: The entry in the empty area's menu opens the dialog with the open folder, in the split view with the
right panel's folder when used there.

**Independent Test**: [quickstart.md](quickstart.md) rows US2.1 to US2.3.

- [x] T016 [P] [US2] In `src/composables/useFilesFolderFacts.ts` expose whether the open folder is encrypted but not
      readable (`folderEncryptedState`) from the same `statAsync` entry (`entry.encrypted.state !== 'readable'`), reset with
      the other facts.
- [x] T017 [US2] In `useFilesManage.ts` compute `syncRule` for `folderMenu` (`path` is not `null`, `syncRuleOffer`
      with the facts of the open folder: `folderOwned`, `folderEncryptedState`; not a symlink and with access by definition)
      and let `case 'syncRule'` without an entry use the open folder (`pickedFromEntry(source, path, 'folder',
folderEncrypted)`), as `pin` does.
- [x] T018 [US2] Extend the E2E scenario: right-click the empty area (`files-list` or the container the pin scenario
      uses), the entry is offered, the dialog gets the open folder; in a second panel of the split view the dialog gets
      that panel's folder (model: `files-split.test.ts`).

**Checkpoint**: US1, US2, US3 work.

## Phase 6: User Story 4 - No entry where the entry cannot be a place (Priority: P2)

**Goal**: No entry in holzi's own places, in a received share, in a not readable encrypted folder, or on a selection
of several entries.

**Independent Test**: [quickstart.md](quickstart.md) rows US4.1 to US4.3.

- [x] T019 [P] [US4] Cover the remaining cases of `syncRuleOffer` and the menus in `scripts/check-files-registry.ts`
      and `scripts/check-files-clipboard.ts`: holzi's own place (entry and open folder), a contact's share,
      a not readable encrypted open folder, a selection of several entries. Fix the code of T009 to T017 where a case
      fails.
- [x] T020 [US4] Extend the E2E scenario: select two entries, open the menu, `files-menu-syncRule` is absent; open the
      menu inside holzi's own place (as `files-protected` does, if such a scenario exists, else the settings place the
      other scenarios use) and see it absent.

**Checkpoint**: All four stories hold.

## Phase 7: Polish and cross-cutting

- [x] T021 Focus (FR-009, research R6): by hand and then in code, check that after the dialog closes the focus is back
      in the list when the menu was opened with Shift+F10. If `UiDrawerModal` does not return it, return the focus in
      `Panel.vue` when the dialog's `open` turns `false`.
- [x] T022 [P] Docs: add the menu entry to `specs/061-sync-rules/contracts/` only if it lists the file browser's
      entry points (read it first; skipped: the 061 documents name no file browser entry point), and run
      `python3 scripts/ci/check-docs.py`.
- [x] T023 Run the local gates on the finished change: `pnpm typecheck`, `pnpm typecheck:scripts`, `pnpm lint`, every
      `pnpm check:*`, Prettier on the changed files, `python3 scripts/ci/check-docs.py`. No Rust changed, so no
      cargo run is needed.
- [x] T024 Before the pull request: `git fetch origin`, merge `main` if it moved, push `073-sync-rule-from-menu`
      (as `haexmas`, back to `haexhub` afterwards).
- [x] T025 Dispatch the CI on the branch (`gh workflow run ci.yml --ref 073-sync-rule-from-menu`, as `haexmas`; E2E runs
      on `main` and dispatch only) and compare failures with the ones `main` shows.
- [x] T026 Pull request, only after the operator agrees (as `haex-space` through `gh auth switch`, back afterwards;
      REST fallback `gh api -X POST repos/haex-space/haex-vault/pulls` when `gh pr create` fails on GraphQL).
- [ ] T027 **Operator**: the hand check of [quickstart.md](quickstart.md) with a build of this branch (mouse, long
      press on Android, Shift+F10). The agent cannot tick this one.

## Dependencies and order

- Phase 2 first. T002 → T003, T005 → T006, T004 and T007 any time.
- US1 (T008 → T009 → T010 → T011 → T013) is the MVP; T012 anywhere after T010.
- US3 (T014, T015) builds on T010 and the scenario of T013.
- US2 (T016 → T017 → T018) builds on T010; T016 is independent of US1.
- US4 (T019, T020) after T017.
- Polish after the stories; T024 to T027 in order.

## Parallel opportunities

T002, T004, T005, T007 touch different files; T016 and T008 do too.

## Implementation strategy

MVP is Phase 2 plus US1. US3 follows at once (small). US2 and US4 close the spec. One pull request for all, since the
change is small and the entry should not exist half-way.
