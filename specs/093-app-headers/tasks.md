# Tasks: The headers of Contacts, Passwords, Wallet and the Marketplace

**Input**: Design documents from `specs/093-app-headers/`

**Prerequisites**: [plan.md](plan.md), [spec.md](spec.md), [research.md](research.md), [data-model.md](data-model.md),
[contracts/header-dot.md](contracts/header-dot.md), [quickstart.md](quickstart.md); #986 on `main` (checked menu
entries, overflow panel on the window)

**Tests**: requested by the constitution (tests in `*_tests.rs` next to Rust modules, a check script for each piece of
TypeScript logic) and by the spec's independent tests. Test tasks come before the code they test.

**Gate**: R10 (group headings for Wallet's menu, the tags' colour) needs the operator's decision before Phase 1 is
finalised (a `heading` entry would belong to step 0) and before Phases 2 and 4.

**Order**: one phase = one pull request, in this order: Phase 1 (the dot) first, then Phases 2–5 (one app each) which
are independent of each other once Phase 1 is on `main`.

| Step (= PR) | Phase               |
| ----------- | ------------------- |
| 0           | 1 The dot (FR-008)  |
| 1           | 2 Passwords (US1)   |
| 2           | 3 Contacts (US2)    |
| 3           | 4 Wallet (US3)      |
| 4           | 5 Marketplace (US4) |

**Rules for every task**:

- Run `graphify query "<what it does>"` before writing a new named function, component or module (spaex).
- At most 500 lines per file; Rust tests in `header_tests.rs`; no new dependency.
- Rust only as in the 10 GB scope: `CARGO_PROFILE_DEV_DEBUG=line-tables-only CARGO_INCREMENTAL=0 … -j 1`,
  `env -u CC -u CXX -u AR`. After a `cargo test` that touches ts-rs types:
  `sed -i -E "s/[[:space:]]+$//" src/types/bindings/*.ts`.
- Commits: English, short, no agent trailer. Branch off `origin/main` with `--no-track`. Push and call `gh` with
  `GH_TOKEN="$(gh auth token --user haex-space)"` (never `gh auth switch`: other sessions share the active account). Run all `check:*` after touching a composable (Node sandbox has no DOM).
- Verify before ticking: run the named check or E2E, never tick on reading.

## Phase 1: The dot (step 0, FR-008)

**Goal**: a `menu` control with `active` shows a dot; the ⋯ button shows it when such a control sits in the panel.

- [ ] T001 [P] Test first: add to `src-tauri/src/extensions/bridge/header_tests.rs` the cases of contracts/header-dot.md (`active` accepted on a menu, a non-boolean refused, ignored on a button)
- [ ] T002 [P] Test first: extend `scripts/check-wm-header.ts`: a pure helper `overflowActive(description, states)` is true iff an overflowed control is an `active` menu; and `headerChecks` warns for `active` on a non-menu
- [ ] T003 Add `active: Option<bool>` (`#[ts(optional)]`, skip if none) to `WmHeaderControl::Menu` in `src-tauri/src/extensions/bridge/header.rs` and `flag(object, "active", own)?` in the `menu` arm of `header_validate.rs`; run T001; regenerate bindings and strip trailing spaces
- [ ] T004 Add `overflowActive` to `src/lib/wm/headerLayout.ts` (or a sibling if the file nears 500 lines) and the dev warning to `src/lib/wm/headerChecks.ts`; run T002
- [ ] T005 Draw the dot in `src/components/wm/HeaderControl.vue` (over the menu icon in the row, absolutely positioned so it adds no width; next to the heading of the group in the overflow panel), with the accessible suffix from i18n `wm.header.active` (de "aktiv", en "active") in `src/i18n/locales/de.json` and `en.json` next to the other `wm.header.*` keys
- [ ] T005a In `HeaderControl.vue`: give each entry row of a menu in the overflow panel `data-testid="<menu test id>-<entry id>"` (the row form has it, the panel form has none today), and check that the dropdown content of a menu in the row scrolls (`max-h` + `overflow-y-auto`, as the old tag filter and Wallet filters had) so a long menu never grows past the window (spec edge case); add both to `check-wm-header.ts` where it can be asserted, else to the `window-header` scenario (T007)
- [ ] T006 Pass the aggregate from `src/components/wm/Header.vue` to `src/components/wm/HeaderOverflow.vue` and draw the dot on the ⋯ button, with the `wm.header.active` suffix on its `aria-label` while the aggregate is on
- [ ] T007 Extend `scripts/e2e/scenarios/window-header.test.ts`: the Settings or Files pilot cannot set `active` without a filter, so use the dev-only extension fixture of `extension-header` (menu with `active: true`): dot in the row, dot on ⋯ when narrowed, none when `active: false`
- [ ] T008 Add `active?: boolean` to the menu control in `vault-sdk` (`src/types/header.ts`, test in `src/api/__tests__/header.test.ts`): into PR #112 if still open, else a follow-up PR; record the pinned reference in spec 090's SDK note when merged
- [ ] T009 Run `pnpm typecheck`, `typecheck:scripts`, `lint`, `format:check`, `check:wm-state`, `check:extensions`, `check:templates`; `cargo test --lib bridge::header`; push, open the PR, dispatch E2E (`gh workflow run ci.yml --ref <branch>`)

**Checkpoint**: Phase 1 merged; the dot is available to every app.

## Phase 2: User Story 1 — Passwords (step 1, Priority P1)

**Goal**: the password manager's first row is the window header (spec US1, FR-001–FR-011).

**Independent test**: quickstart steps 1–6 in Passwords.

- [ ] T010 [P] [US1] Test first: `scripts/check-passwords-header.ts` for the pure builder (list: sidebar keep, search `q` order 10 shrink, `tag-filter`/`menu`/`new` in order; entry open: sidebar and `menu` only; tag filter absent without tags; the picked tag is found as `TagFilter.vue` did (`tag.ids.includes(?tag=)`) and `active` iff one is picked; checked entries "all tags"/picked tag; every place but `/entry/…` has the full set (trash, generator, import included, as the old `entryOpen` rule); no repeated id and no `headerProblems` (entry ids share the namespace of control ids, data-model.md); relative `.ts` imports only so Node loads it; test ids `passwords-sidebar-toggle`, `passwords-search`, `passwords-tag-filter`, `passwords-new`, `passwords-menu`); no `package.json` entry (the `check:passwords` glob runs it)
- [ ] T011 [US1] Create `src/lib/passwords/header.ts`: `passwordsHeader(state, t)` per data-model.md; menu entry ids chosen so `<menu>-<entry>` is readable (`passwords-new-entry`, `passwords-new-folder`, `passwords-menu-generator`, `-import`, `-tags`, `-settings`); run T010
- [ ] T012 [US1] In `src/components/apps/PasswordsApp.vue`: call `useWmHeader` with the builder; keep `query` bound to `?q=` with the history-restore watch and Escape-clears behaviour of the old `Toolbar.vue`; handlers for `sidebar` (`toggle`), `q` (`input`), `tag-filter` (`click`, the entry id as `value` → `router.push`), `new` (`click`: entry → `/entry/new`, folder → dialog), `menu` (`click`: generator, import, tags, settings); a menu sends `click` with the entry id, `select` is for `choice` only
- [ ] T013 [US1] Mount `PasswordsClipboardSetting`, `PasswordsTagManager` and `PasswordsFolderDialog` in `PasswordsApp.vue` with their open flags there (R2, FR-009); keep the "focus stays in the editor after New → Entry" behaviour of `keepEditorFocus` (check how the header menu returns focus; adapt, do not drop it)
- [ ] T013a [US1] Keep the keyboard working (spec edge case "Keyboard"). Two things looked up inside the app root now live in the header: `onEscape` in `PasswordsApp.vue` returns focus to `passwords-sidebar-toggle` through `root.value.querySelector` (as `SettingsApp.vue` does it, go through `root.value?.closest('[data-wm-window-id]')`; Contacts and Wallet have the same line in `ContactsApp.vue` and `WalletApp.vue`), and the focus-search shortcut: `focusSearch` in `src/composables/usePasswordsListKeys.ts` looks for `passwords-search` inside `[data-passwords-app]`, but the field is now in the window header, outside the app root; query from the window element (`[data-wm-window-id]`, `Window.vue`) instead; when the search has collapsed into the overflow panel its field exists only while the panel is open (`HeaderOverflow.vue` draws the controls in its sheet over the window only then), so the shortcut opens the panel first (`WM_OVERFLOW_OPEN`) and then focuses the field; cover both the row and the panel case with a check or an E2E step; look for the same pattern in Contacts, Wallet and Marketplace when their phases start
- [ ] T014 [US1] Delete `src/components/passwords/Toolbar.vue` and `TagFilter.vue`; remove the `<PasswordsToolbar>` row and its `h-13` slot from `PasswordsApp.vue`; fix any other reference found by `rg "PasswordsToolbar|PasswordsTagFilter"`
- [ ] T015 [US1] Adapt E2E selectors for menu entries in `scripts/e2e/lib/passwords.ts` and the scenarios using them (`rg "passwords-(new-folder-menu|open-generator|open-import|manage-tags|settings|tag-filter-)" scripts` finds them: today `passwords-basic`, `passwords-narrow-window`, `passwords-custom-fields`, `passwords-autofill-aliases`, `passwords-tabs`, `passwords-passkeys`, `passwords-organize`, `fields-basic`; `passwords-new-entry` and the control ids keep their names); a control narrowed into the panel is opened through `clickHeaderControl` (`scripts/e2e/lib/header.ts`); the search is the first control to leave the row and the rig's screen is 800 px wide, so a scenario that types into `passwords-search` makes sure it is in the row (or the helper learns to open the panel for a field)
- [ ] T016 [US1] Extend `passwords-narrow-window.test.ts`: header holds every control at 1000 px (SC-003), nothing missing at 460 px (SC-002), the dot on the tag filter and on ⋯ with a tag picked, a dialog opened from the menu survives a resize, and the app's top edge sits at the header's bottom edge (SC-001, FR-011)
- [ ] T017 [US1] Run `check:passwords`, `check:wm-state`, `check:wm-navigation`, `typecheck`, `typecheck:scripts`, `lint`, `format:check`, `check:templates`; hand check SC-005 and quickstart; push, PR, dispatch E2E; separate own failures from known flakes by SHA and logs

**Checkpoint**: Passwords is a working increment.

## Phase 3: User Story 2 — Contacts (step 2, Priority P2)

**Goal**: spec US2.

**Independent test**: quickstart steps 1, 2, 4, 6 in Contacts; "new contact" in a group starts in that group.

- [ ] T018 [P] [US2] Test first: `scripts/check-contacts-header.ts` (list: sidebar keep, `q`, the menu `add` with entries `new` (new contact) and `import-menu` (read a card) — the control is not called `new`, an entry id and a control id must not repeat; a contact or `/import` open: sidebar only; test ids `contacts-sidebar-toggle`, `contacts-search`, `contacts-add`, entries `contacts-add-new`, `contacts-add-import-menu` (today `contacts-new`, `contacts-import-menu`); no `headerProblems`); no `package.json` entry (the `check:contacts` glob runs it)
- [ ] T019 [US2] Create `src/lib/contacts/header.ts`; if the sidebar toggle repeats verbatim from Passwords, extract `sidebarToggle(...)` into `src/lib/wm/headerParts.ts` now and use it in both (plan, graphify note) and rerun `check:passwords`
- [ ] T020 [US2] In `src/components/apps/ContactsApp.vue`: `useWmHeader`, handlers (the new-contact-in-group logic and the card reader as in the old `Toolbar.vue`), keep `check-contacts-search.ts` and `check-contacts-menu-focus.ts` passing (adapt only what the move forces); delete `src/components/contacts/Toolbar.vue`
- [ ] T021 [US2] Adapt `scripts/e2e/lib/contacts.ts` and the scenarios that use `contacts-new`, `contacts-import-menu`, `contacts-add` or `contacts-search` (`rg` them: today `contacts-basic`, `agent-contacts`, `contacts-share`, `android-contacts-desktop`); add a narrow/wide header step
- [ ] T022 [US2] Run the checks of T017 plus `check:contacts`; push, PR, dispatch E2E

## Phase 4: User Story 3 — Wallet (step 3, Priority P2)

**Goal**: spec US3.

**Independent test**: quickstart steps 1–5 in Wallet (kind filter, sort, hide expired).

- [ ] T023 [P] [US3] Test first: `scripts/check-wallet-header.ts` (list: sidebar keep, `q`, `filters` menu with sort group, kind group, hide expired — each current entry `checked`; entry ids `sort-date`, `sort-title`, `kind-all`, `kind-<kind>`, `hide-expired` so the test ids become `wallet-filters-<entry>` (today `wallet-sort-*`, `wallet-kind-*`, `wallet-hide-expired`); the group headings follow R10; `active` iff kind ≠ all or hide expired is on or sort ≠ default; `new` button; a pass, the trash or any other place but `/` and `/folder/…` open: sidebar only (the old `passOpen` rule); test ids `wallet-sidebar-toggle`, `wallet-search`, `wallet-filters`, `wallet-new`, entries `wallet-filters-<entry>`; no `headerProblems`); no `package.json` entry (the `check:wallet` glob runs it)
- [ ] T024 [US3] Create `src/lib/wallet/header.ts`; read the filter state from the wallet store the old `ListFilters.vue` used (state unchanged, R1)
- [ ] T025 [US3] In `src/components/apps/WalletApp.vue`: `useWmHeader`, handlers (new pass in the open folder via `walletNewPassLocation`, as the old `Toolbar.vue`); delete `src/components/wallet/Toolbar.vue` and `ListFilters.vue`
- [ ] T026 [US3] Adapt `scripts/e2e/lib/wallet.ts`, `wallet-basic.test.ts`, `wallet-organize.test.ts` selectors; add a narrow/wide header step with the dot
- [ ] T027 [US3] Run the checks of T017 plus `check:wallet`; hand check SC-005; push, PR, dispatch E2E

## Phase 5: User Story 4 — Marketplace (step 4, Priority P3)

**Goal**: spec US4.

**Independent test**: quickstart steps 1–3 in the Marketplace with two marketplaces switched on.

- [ ] T028 [US4] Confirm by hand that the controls leave the header on a detail page (R7: the grid is a route component and unmounts; if it does not, give the detail page an empty description)
- [ ] T029 [P] [US4] Test first: `scripts/check-marketplace-header.ts` (search `q`; `category` menu with `Name (count)` labels, current checked, `active` iff not "all"; `source` menu only with >1 enabled marketplace, `active` iff not "all"; entry ids `all-categories`, `all-sources`, `cat:<slug>`, `src:<id>` (the two "all" entries must not repeat an id; no `headerProblems`); `refresh` button disabled while loading; no sidebar; test ids `marketplace-search`, `marketplace-category`, `marketplace-source-filter`, `marketplace-refresh`); no `package.json` entry (the `check:marketplace` glob runs it)
- [ ] T030 [US4] Create `src/lib/marketplace/header.ts`; in `CatalogGrid.vue` call `useWmHeader`, keep the search text and Escape-clears behaviour; delete `src/components/marketplace/CatalogToolbar.vue`
- [ ] T031 [US4] Adapt `scripts/e2e/lib/marketplace.ts` and the scenarios that use the category, source, refresh or search controls (today `marketplace-catalog`, `-manage`, `-update`, `-install`; the category and source were selects, now menus whose entries are `<menu>-<entry>`); add a narrow/wide header step
- [ ] T032 [US4] Run the checks of T017 plus `check:marketplace`; push, PR, dispatch E2E

## Phase 6: Polish (after the four apps)

- [ ] T033 [P] Update `specs/090-app-header-bar/` notes: "Contacts, passwords, … left to follow" now points to spec 093; spec 093 status Implemented once all four PRs are merged
- [ ] T034 [P] Add the terms _Header description_, _Header control_, _Overflow panel_, _dot (active menu)_ to `CONTEXT.md` if not already added by spec 090's polish; no duplicates
- [ ] T035 [P] Run `mcp__knip__knip-run`; remove what the deleted toolbars left unused (i18n keys, store helpers)
- [ ] T036 Chat remains: note in the final PR that its header is a separate spec

## Dependencies & order

- Phase 1 before 2–5. Phases 2–5 are independent; T019 couples Phase 3 to Phase 2 only through the optional shared
  `sidebarToggle` helper.
- Within a phase: test task → builder → app wiring → deletion → E2E adaptation → full run.
- MVP: Phases 1–2 (the mechanism and the biggest app).

## Parallel examples

- Phase 1: T001 and T002 run in parallel (Rust test, Node check).
- After Phase 1: Phases 2–5 can be worked in separate worktrees (one `target` per worktree costs disk: mind the
  cleanup memory; use `mv` of a merged target instead of rebuilding).
