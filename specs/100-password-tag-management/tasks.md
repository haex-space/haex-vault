---
description: 'Task list for tag management by selection, with merge (spec 100)'
---

# Tasks: Tag management by selection, with merge

**Input**: Design documents from `/specs/100-password-tag-management/`

**Prerequisites**: [plan.md](plan.md), [spec.md](spec.md), [research.md](research.md), [data-model.md](data-model.md), [contracts/tauri-commands.md](contracts/tauri-commands.md), [quickstart.md](quickstart.md)

**Tests**: Included. A merge rewrites many synced rows, and the repository rules require tests with behaviour changes. Rust tests live in sibling `*_tests.rs` files, never inline. Pure frontend logic is checked with the Node check scripts. The e2e rig runs on `main` CI only, so scenarios carry diagnostics (they name what they saw).

**Organization**: Grouped by user story. US1 (one dropdown, edit the chosen tag, counts per app, Wallet entry) is frontend only and ships on its own. US2 (merge) needs the backend and builds on US1's screen. US3 is the narrow-window and keyboard proof for both.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: can run in parallel (different files, no dependency on an unfinished task)
- **[Story]**: US1 to US3 as in spec.md; Polish tasks carry none
- Paths are relative to the repository root, inside the worktree `.worktrees/tag-management`

## Commands used by the verification tasks

- Everything runs through the devShell: `nix develop --command <command>` (node, pnpm and cargo exist only there).
- Rust: `nix develop --command scripts/with-nix-host-bridge.sh cargo test --manifest-path src-tauri/Cargo.toml <filter> -j 4` (targeted filters only; a full build has crashed the editor before).
- Frontend: `pnpm typecheck`, `pnpm typecheck:scripts`, `pnpm lint`, `pnpm check:passwords`, `pnpm check:wallet`, `pnpm check:templates`, `pnpm format:check`.
- After `cargo test` regenerates ts-rs bindings: `sed -i -E 's/[[:space:]]+$//' src/types/bindings/*.ts` (script `generate:ts-types` does both).

## Names used below

| Name             | Value                                                                                                             |
| ---------------- | ----------------------------------------------------------------------------------------------------------------- |
| `TagPicker`      | `src/components/passwords/TagPicker.vue`, props `tags` (display tags), `exclude?`, `v-model` tag id               |
| `TagUsage`       | `src/components/passwords/TagUsage.vue`, prop `usage` (`{ login, pass }`): the two apps' registry icons + numbers |
| `TagMergeDialog` | `src/components/passwords/TagMergeDialog.vue`, props `source`, `v-model:open`                                     |
| `tagUsageOf`     | `tagUsageOf(ids, headers) -> { login, pass }` in `src/lib/passwords/scope.ts`, same cut as `loginTagsOf`          |
| `merge_tags`     | `tags::merge_tags(tx, source_id, target_id)` in `src-tauri/src/passwords/tags.rs`                                 |
| `move_links`     | private `tags::move_links(tx, from, to)`, the loop `reconcile_tags` has today                                     |
| Test ids         | `passwords-tag-picker`, `passwords-tag-picker-<id>`, `passwords-tag-merge`, `…-target`, `wallet-tags`             |

---

## Phase 1: User Story 1 - Pick one tag and edit it (Priority: P1) 🎯 MVP

**Goal**: "Tags verwalten" (in the password manager and in Wallet) shows one searchable dropdown; the chosen tag appears with name, color, the count per app (password manager icon + number, Wallet icon + number, taken from the app registry), rename and delete; no list of all tags; a name needs at least one character.

**Independent Test**: With 60 tags, open the manager, type a few letters, pick a tag, rename it and change its color; only the dropdown and the one tag are shown; a tag on 5 logins and 5 passes shows 5 and 5; the same manager opens from Wallet (quickstart M1, M2, M7, M8).

- [x] T001 [P] [US1] Add `tagUsageOf(ids: readonly string[], headers)` to `src/lib/passwords/scope.ts` next to `loginTagsOf`: returns `{ login, pass }`, the number of headers in `inScope(headers, 'login')` and `inScope(headers, 'pass')` that carry any of `ids`. Reuse `inScope`; `loginTagsOf` may call it for its `itemCount` so the two cannot disagree. Cases in `scripts/check-wallet-scope.ts` (same `HEADERS` fixture): a tag on logins only, on passes only, on both, on none (`{0,0}`), a group of two ids counting an entry that carries both once, and `loginTagsOf` still passes unchanged.
- [x] T002 [P] [US1] Create `src/components/passwords/TagUsage.vue`: two badges, the icon of the password manager with `login` and the icon of Wallet with `pass`. Take each icon from the registry, not by name: `getAppDefinition('system.passwords')` and `getAppDefinition('system.wallet')` from `src/lib/wm/apps.ts`, drawn with `WmTabIcon` (`src/components/wm/TabIcon.vue`, props `icon` and `icon-url`), so they are the very images of dock, launcher and tabs (`/app-icons/passwords.svg`, `/app-icons/wallet.svg`), with the Iconify icon of the entry as fallback; no icon name is written in this component. Size by class (about `size-4`). Each badge an element with `aria-label` from i18n (`passwords.tags.usage.login`/`pass`, plural by count, e.g. "Passwortmanager: 5 Einträge"), a number of 0 in `text-muted-foreground`. Both always shown, never one combined number (FR-015). Test ids `passwords-tag-usage-login` and `…-pass`. Add to `scripts/check-wallet-scope.ts` a check that `getAppDefinition('system.passwords')` and `getAppDefinition('system.wallet')` both exist and carry an `iconUrl`, so a renamed app id breaks the check instead of showing no icon.
- [x] T003 [P] [US1] Create `src/components/passwords/TagPicker.vue`: a `ShadcnPopover` holding a `ShadcnCommand` (input + list), the trigger shows the chosen tag (color dot, name) or the placeholder. Items show a color dot, the name and `TagUsage`; item `value` is the tag name so the Command filter (`sensitivity: 'base'`, research R3) does the matching, no own filter code; names are unique because the list is `displayTags` (R3). Props `tags`, optional `exclude: string` (tag id left out), `v-model` the chosen tag id or `null`; `data-testid` goes to the trigger, items get `passwords-tag-picker-<id>`. Keyboard: trigger opens on Enter/Space, arrows move, Enter picks, Esc closes. No match shows `passwords.tags.noMatch`. Depends on T002.
- [x] T004 [P] [US1] Add the German and English texts in `src/i18n/locales/de.json` and `src/i18n/locales/en.json` under `passwords.tags`: `choose`, `search`, `noMatch`, `selectOne`, and `usage.login`/`usage.pass` (plural: "keine Einträge | {count} Eintrag | {count} Einträge"). Both files get identical keys.
- [x] T005 [US1] Rewrite `src/components/passwords/TagManager.vue`: keep the dialog shell, the `none` state (no dropdown when `store.displayTags.length === 0`), and the existing rename, color and delete logic and error handling (`exists | empty | too_long` inline under the name field, every other failure `toast.error`). Replace the `v-for` list over `store.tags` by `TagPicker` over `store.displayTags` plus the editor of the selected tag: name with the rename controls, `PasswordsColorSelect`, `TagUsage` (from `tagUsageOf(tag.ids, store.headers)`), delete button and its alert dialog. State is `selectedId: string | null`; a `watch` on `store.displayTags` sets it to `null` when the tag no longer exists (edge case 1); after delete it is `null`. With no selection nothing of the editor is rendered. Rename, color and delete act on the tag's `id` (the smallest id of its group). Keep the test ids `passwords-tag-name-<id>`, `passwords-tag-color-<id>`, `passwords-tag-rename-<id>`, `passwords-tag-delete-<id>`; the old row id `passwords-tag-<id>` becomes the editor's id. Update the file's doc comment (spec 100). Depends on T001, T003, T004.
- [x] T006 [US1] Open the manager from Wallet (research R8): in `src/lib/wallet/header.ts` add `{ kind: 'tags' }` to `WalletHeaderCommand`, a `button('tags', t('passwords.tags.manage'), 'lucide:tags', 35)` in `end` (test id `wallet-tags`) and the case `'tags'` in `walletHeaderCommand`; in `src/composables/useWalletHeader.ts` return `tagManagerOpen` and set it on the command (like `capturing`); in `src/components/apps/WalletApp.vue` mount `<PasswordsTagManager v-model:open="tagManagerOpen" />` next to the capture dialog. Extend `scripts/check-wallet-header.ts`: the list header's `end` ids are `['filters', 'tags', 'capture', 'import', 'new']`, the control's test id is `wallet-tags`, its command is `{ kind: 'tags' }`, and it leaves with the other actions when a pass is open. Depends on T005.
- [x] T007 [US1] Update `scripts/e2e/scenarios/passwords-narrow-window.test.ts` (lines around 108-122): the dialog is now identified by `passwords-tag-picker`; select the tag first, then wait for `passwords-tag-<id>`. Keep the Escape-closes-the-dialog and 360 px assertions. Depends on T005.
- [x] T008 [US1] Create `scripts/e2e/scenarios/passwords-tags.test.ts` (shape of `passwords-organize.test.ts`, helpers from `scripts/e2e/lib/passwords.ts`): create **500** tags, open the manager, assert there is no per-tag row (only the picker is in the DOM), type the start of a name, pick, rename to a case variant of another tag and read the inline hint, rename to `''` and `'   '` and read the `empty` hint with the tag unchanged (FR-017), rename properly, change the color, read the values back through `overview`, delete with confirmation and assert the dropdown no longer offers the tag and the selection is empty. Then a tag on 5 logins and 5 passes: the manager shows 5 and 5 (read from the two `TagUsage` test ids), the password manager's tag filter shows 5 entries, and the manager opened through `wallet-tags` shows the same numbers. Every assertion message names what it saw. Depends on T005, T006.

**Checkpoint**: US1 works alone: `pnpm typecheck`, `pnpm lint`, `pnpm check:templates`, `pnpm check:wallet` pass; M1, M2, M7, M8 pass by hand.

---

## Phase 2: User Story 2 - Merge one tag into another (Priority: P1)

**Goal**: A selected tag can be merged into another; entries (logins and passes) keep their tag, no duplicates, the source is gone; only the user can do it; the confirmation names the counts per app and the affected agent profiles.

**Independent Test**: Entries tagged "Job", "Arbeit" and both; merge "Job" into "Arbeit"; every entry has "Arbeit" once and "Job" exists nowhere (quickstart M3).

### Backend

- [x] T009 [US2] In `src-tauri/src/passwords/tags.rs` extract the inner loop of `reconcile_tags` (move the links of `other` to `keep`, no second link when one exists, a delete marker per removed row, then delete the `other` tag row) into `fn move_links(tx, from: &str, to: &str) -> Result<()>` (it moves the links only; the callers delete the tag row) and call it from `reconcile_tags`. No behaviour change: the three `reconcile_*` tests in `src-tauri/src/passwords/tags_tests.rs` stay green unchanged (`cargo test passwords::tags`).
- [x] T010 [US2] Add `pub fn merge_tags(tx, source_id, target_id) -> Result<()>` in `src-tauri/src/passwords/tags.rs`: `sourceId != targetId` else `InvalidInput { reason: "same" }`; both rows must exist else `PasswordsNotFound` (check both before writing, so a refusal changes nothing); then `move_links(source, target)`. The target keeps `name` and `color` (data-model.md). Add the `ponytail:` comment of research R2: a change made on another device before it synced (a link to the source, or the opposite merge) is decided per row by the sync order; upgrade path is a synced merge record applied at open. Depends on T009.
- [x] T011 [US2] Add tests to `src-tauri/src/passwords/tags_tests.rs` (use `item_with`, `count`, `reason_of`, `tag_names_of`): merge moves every link and the source is gone; an entry with both tags has the target once and its link count is 1; the target keeps its name and color while the source's color is not carried; a source with no links is removed; a source with 200 links merges in one call and the target has them all; `same` refused and nothing changed; a missing source or target gives `PasswordsNotFound` and nothing changed; a second merge of the same source gives `PasswordsNotFound`; every moved or dropped link and the source row left a marker in `haex_deleted_rows` (as `reconcile_moves_a_link_…` asserts). Also lock FR-017: `rename_tag` with `""` and `"   "` gives `empty`, `get_or_create`/`bulk_set` with `"   "` gives `empty` and creates no row, and `push_tag` of the import drops a blank name (extend the existing `a_tag_name_must_be_neither_empty_nor_long` and the import tests where they already cover a case; add only what is missing). Depends on T010.
- [x] T012 [P] [US2] Create `src-tauri/src/agent_profiles/tag_references.rs` with `profiles_naming_tag(q, tag_name) -> haex_crdt::Result<Vec<String>>`: list the profiles with `store::profiles(q)`, read each one's rows with `store::rights_rows(q, profile_id)` (column `scope_tags`, `GrantRow.tags`), decode them with `grant_rows::rights_of`, and return the sorted unique names of the profiles whose `passwords.tags` or `default_tag` equals the name by `passwords::ids::fold`. Declare the module (and `#[cfg(test)] mod tag_references_tests;`) in `src-tauri/src/agent_profiles/mod.rs`. Create `src-tauri/src/agent_profiles/tag_references_tests.rs`: profile naming the tag in its list, one naming it as default tag, a case or umlaut variant matches, a profile with `tags: None` (all entries) does not, a profile without password rights does not, no profile gives an empty list.
- [x] T013 [US2] In `src-tauri/src/passwords/service/organize.rs` add `merge_tags(&self, caller, source_id, target_id)` (first line `require_user(caller)?`, then `self.db().write(…tags::merge_tags…)`) and `tag_agent_profiles(&self, tag_id) -> Result<Vec<String>>` (looks the tag name up by id, `PasswordsNotFound` when missing, then `profiles_naming_tag`). Add a test next to the existing service tests for `rename_tag`/`set_tags` (find them with `rg "require_user|Caller::" src-tauri/src/passwords --glob '*_tests.rs'`; if none covers the refusal, create `src-tauri/src/passwords/service/organize_tests.rs`): a caller that is not `Caller::User` is refused with the same error as `rename_tag` and nothing changes (FR-009). Depends on T010, T012.
- [x] T014 [US2] In `src-tauri/src/passwords/commands/organize.rs` add `MergeTagsArgs { source_id, target_id }` (derive like `RenameTagArgs`: `Deserialize, TS`, `export_to = "../../src/types/bindings/"`, `camelCase`), `passwords_merge_tags` and `passwords_tag_agent_profiles` (args `TagIdArgs`), then register both in `src-tauri/src/lib.rs` next to `passwords_delete_tag` (the `use` list near line 191 and the `invoke_handler` list near line 1108). Generate and commit `src/types/bindings/MergeTagsArgs.ts` (`cargo test --lib export_bindings_mergetagsargs` through the host bridge; the full `generate:ts-types` builds every test target and is not needed for one type). Depends on T013.

### Frontend

- [x] T015 [P] [US2] In `src/composables/usePasswords.ts` add `mergeTagsAsync(sourceId, targetId)` (`invoke<null>('passwords_merge_tags', { args: { sourceId, targetId } })`) and `tagAgentProfilesAsync(tagId)` (`invoke<string[]>('passwords_tag_agent_profiles', { args: { tagId } })`) next to `deleteTagAsync`, and return them. Depends on T014.
- [x] T016 [P] [US2] Add the merge texts to `src/i18n/locales/de.json` and `src/i18n/locales/en.json` under `passwords.tags`: `merge` (button "Zusammenführen" / "Merge into another tag"), `mergeTarget` (picker label), `mergeTitle`, `mergeBody` with `{source}`, `{target}`, `{logins}`, `{passes}` (names both numbers: "…{logins} im Passwortmanager und {passes} in der Wallet bekommen „{target}“, „{source}“ verschwindet."), `mergeAgents` with `{profiles}` (the FR-010 warning: agents of these profiles that may read the target tag then also reach the entries that only carried the source), `mergeConfirm`.
- [x] T017 [US2] Create `src/components/passwords/TagMergeDialog.vue`: props `source` (a display tag), `v-model:open`; emits `merged(targetId)`. Inside: a `TagPicker` over `store.displayTags` with `exclude = source.id` for the target, and when a target is chosen the confirmation (`ShadcnAlertDialog` as in `TagManager.vue`) naming both tags and the source's numbers per app (`tagUsageOf(source.ids, store.headers)`), and, when non-empty, the profile names from `tagAgentProfilesAsync` for source and target (unique union, loaded when the target is chosen; a load failure is `toast.error` and the merge stays possible without the line). Confirm calls `mergeTagsAsync(source.id, target.id)`, then `store.quietReloadAsync()`, emits `merged(target.id)`; cancel changes nothing; a failure is `toast.error(errString(cause))`, no success text. Test ids `passwords-tag-merge-target`, `passwords-tag-merge-confirm`. Depends on T003, T015, T016.
- [x] T018 [US2] In `src/components/passwords/TagManager.vue` show the "Zusammenführen" button (`passwords-tag-merge`) in the editor only when `store.displayTags.length > 1` (spec US2 scenario 6), open `TagMergeDialog` for the selected tag, and on `merged` set `selectedId` to the target (scenario 5). Depends on T005, T017.
- [x] T019 [US2] Extend `scripts/e2e/scenarios/passwords-tags.test.ts`: entries tagged "Job" (one also "Arbeit") and "Arbeit", plus one Wallet pass tagged "Job"; merge "Job" into "Arbeit": the confirmation names both numbers (logins and passes), cancel first and read that nothing changed, then confirm; read back that each entry (and the pass) has "Arbeit" once, "Job" is gone from the picker and the tag filter, "Arbeit" is selected with the union counts per app, "Arbeit" kept its name and color. With one tag only, the merge button is absent. Depends on T018.
- [x] T020 [US2] Extend `scripts/e2e/scenarios/passwords-sync-two-devices.test.ts` (FR-008, SC-003): after the two devices share entries with the tags, merge on device 1; wait for device 2 to show the target with the union of both tags' entries, no source, and no entry with the target twice. Depends on T018.

**Checkpoint**: US2 works: `cargo test passwords::tags` and `cargo test agent_profiles::tag_references` pass; M3 and M4 pass by hand.

---

## Phase 3: User Story 3 - Works in a narrow window and on the phone (Priority: P2)

**Goal**: The same behavior at 360 px, on Android, and with the keyboard alone, in both apps.

**Independent Test**: Quickstart M6.

- [x] T021 [US3] Extend `scripts/e2e/scenarios/passwords-narrow-window.test.ts`: at 360 px the picker, the editor of a selected tag (with both usage badges) and the merge dialog fit the width (assert `scrollWidth <= clientWidth` of the dialog content) and no action is hidden (rename, color, delete and merge are reachable); in Wallet's narrow header `wallet-tags` is in the row or in the overflow panel. Depends on T018.
- [x] T022 [US3] Make the picker and the editor keyboard-complete and check it in `passwords-tags.test.ts`: open the picker, filter and pick, rename, merge and delete using only key events (Tab, arrows, Enter, Escape), asserting focus returns to the trigger after the popover closes. Fix what the check finds in `TagPicker.vue`/`TagManager.vue`. Depends on T018.
- [ ] T023 [US3] Run the manual quickstart M6 on the Android emulator or a phone (`holzi-emulator` devShell; touch: tap trigger, type, tap item) and record the result in the PR description. If no device is available say so in the PR; M6 stays open then.

### Amendment 2026-11-25: name the merged tag, show the merge as a picture, info behind a button

Requested while looking at the running app (US2 scenarios 8-11, FR-006, FR-018).

- [x] T028 [US2] Let a merge name its result: `tags::merge_tags(tx, source_id, target_id, name: Option<&str>)` in `src-tauri/src/passwords/tags.rs` validates the name like a rename (`validate_name`), allows the source's name (it goes), refuses the name of a tag that stays (`exists`), raises every refusal before the first write and renames the target in the same write; extract the shared rule as `name_taken(all, wanted, except)` and use it in `rename_tag` too. Pass `name` through `PasswordsService::merge_tags`, `MergeTagsArgs { name: Option<String> }` and `passwords_merge_tags`; regenerate `MergeTagsArgs.ts`.
- [x] T029 [US2] Tests in `src-tauri/src/passwords/tags_tests.rs` and `src-tauri/src/passwords/service/organize_tests.rs`: a named merge moves every entry to the new name (trimmed); the name of the source is free; an unchanged or missing name keeps the target's name and only a change of case is a change; `exists`, `empty`, `too_long` stop the merge with nothing written (no row, no marker).
- [x] T030 [US2] Rebuild `src/components/passwords/TagMergeDialog.vue` as a picture: the old tag (color, name, `TagUsage` of the source), an arrow (`lucide:arrow-right`, turned down on a narrow screen) and an input for the new name that starts as the target's name (test ids `passwords-tag-merge-from`, `passwords-tag-merge-name`); a refused name shows under the input, any other failure is a toast. Pass `name` through `mergeTagsAsync(source, target, name)` in `src/composables/usePasswords.ts`.
- [x] T031 [US2] Put the explanation (shared by both apps, who is affected, cannot be undone) behind an info button (`lucide:info`, `passwords-tag-merge-info`, text in a popover `passwords-tag-merge-info-text`); the agent-rights warning stays visible and now says that rights go by name. Texts `mergeBody`, `mergeAgents`, `mergeInfo`, `mergeName`, `mergeDescription` in `src/i18n/locales/de.json` and `en.json`.
- [x] T032 [US2] Update `scripts/e2e/scenarios/passwords-tags.test.ts` (`passwords-tags-merge`): the picture names the old tag and shows 2 and 1, the input starts as the target's name, the explanation is hidden until the info button is pressed, and a merge with the name "Beruf" renames the result on every entry.

---

## Phase 4: Polish & Cross-Cutting Concerns

- [x] T024 [P] Update `specs/034-password-manager/spec.md` FR-011: a short dated note that the manager works by selection, shows counts per app, opens from Wallet and offers merging (spec 100); and `specs/034-password-manager/contracts/tauri-commands.md`: the two new commands under the tag section, linking `specs/100-password-tag-management/contracts/tauri-commands.md`.
- [x] T025 Run `pnpm exec prettier --write` on every Markdown and JSON file the change touches (specs, both locale files), then `pnpm format:check`.
- [x] T026 Run the checks and report each result: `cargo fmt --check`, `cargo clippy` for the crate (targeted, `-j 4`), the Rust filters of T009-T013, `pnpm typecheck`, `pnpm typecheck:scripts`, `pnpm lint`, `pnpm check:passwords`, `pnpm check:wallet`, `pnpm check:templates`. Report what could not run.
- [ ] T027 Commit in Conventional Commits (`feat(passwords): …`, `test(passwords): …`, `docs(passwords): …`; no agent attribution), push the branch and open a PR to `main` (no squash). Leave the quickstart manual items M1-M8 listed as open in the PR description; the operator merges.

---

## Dependencies & Execution Order

- US1 (T001-T008): T001-T004 in parallel, then T005, then T006 and T007, then T008. No backend dependency; US1 can ship alone.
- US2 backend (T009-T014) is independent of US1 and can run in parallel with it; US2 frontend (T015-T020) needs the backend commands (T014) and US1's screen (T005, T003).
- US3 (T021-T023) needs US1 and US2 complete. Polish needs everything.
- T012 (reference read) is independent of T009-T011; T013 needs both.

## Parallel Opportunities

- T001, T002, T004 together; T003 after T002.
- T009-T011 (tags) in parallel with T012 (profile references) and with all of US1.
- T015 and T016 together once T014 is done.

## Implementation Strategy

1. **MVP**: US1 (T001-T008). The manager no longer grows with the tag count, the counts match what each app shows, and Wallet can manage tags.
2. **Merge**: backend first (T009-T014, cargo-tested without any UI), then the dialog (T015-T020).
3. **Proof**: US3 and Polish, then the PR.
