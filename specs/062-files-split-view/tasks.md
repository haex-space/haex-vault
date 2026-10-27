---
description: 'Task list for spec 062-files-split-view'
---

# Tasks: Dateibrowser mit zwei Panels (Split-Ansicht)

**Input**: Design documents from `/specs/062-files-split-view/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md),
[data-model.md](./data-model.md), [contracts/split-location.md](./contracts/split-location.md),
[contracts/panel-context.md](./contracts/panel-context.md),
[contracts/shortcuts-and-menu.md](./contracts/shortcuts-and-menu.md), [quickstart.md](./quickstart.md)

**Tests**: Included. The constitution requires tests with every behaviour change. Pure parts are
checked in `scripts/check-*.ts` (run by `node --test`, no vue); behaviour in the app goes into the E2E
scenario `scripts/e2e/scenarios/files-split.test.ts` (scenarios are found in that directory by file
name; style of `files-keyboard.test.ts`). No Rust code changes in this spec, so no cargo runs are needed.

**Organization**: Grouped by user story in priority order (US1 P1; US2, US3 P2; US4, US5 P3). Phases 2
and 3 block all stories. US1 is the MVP.

**Shipping note** (plan.md "Slicing"): PR A = Phase 1 + Phase 2; PR B = Phase 3 (pure refactor, no visible
change); PR C = US1; PR D = US2 + US3; PR E = US4 + US5 + Phase 9. Each PR is reviewed and merged before
the next one starts on top of `main`; B only depends on A, C on B.

**Rules for every task**: before authoring a new named function, component or module consult graphify
(`graphify query` on `graphify-out/`) and extend a candidate if one fits; keep every hand-maintained file
under 500 lines (`windowManager.ts` is at 504 today and MUST NOT grow, `useFilesManage.ts` at 465 gets
only the small change of T038, new logic goes to new files); every new UI text in `src/i18n/locales/de.json`
and `en.json` (vue-i18n refuses `<…>` in messages); never "Layout" (say "Sitzung"/"Panel"), never bare
"Relay"; `UiButton` in a form needs `type="button"`; all state of the split lives in the tab location
(contracts/split-location.md), no new store; every write to the tab location goes through the frame
(`useFilesSplit`), never `useTabRouter()` from a panel; no agent attribution in commits or PR text.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependency on an incomplete task)
- **[Story]**: User story from spec.md (US1–US5)

---

## Phase 1: Setup (PR A, docs)

- [x] T001 Commit spec, plan, research, data model, contracts, quickstart, checklist and this file in `specs/062-files-split-view/` and `.specify/feature.json` (`"feature_directory": "specs/062-files-split-view"`)
- [x] T002 [P] Add a row to the table in `plans/README.md` after the 061 row (or the last row): "062: Dateibrowser mit zwei Panels (`specs/062-files-split-view/`)", priority P2, effort M, status "Spezifiziert, geplant und in Aufgaben zerlegt 2026-10-26; Lieferungen PR A–E", dependencies "044, 048, 056 im Einsatz"
- [x] T003 [P] In `specs/044-file-browser/spec.md` add a note under FR-017b: "Geändert durch Spec 062: In der Split-Ansicht ist F5 „Kopieren ins andere Panel“ und Neuladen nur noch Strg+R."
- [x] T004 [P] In `specs/020-tab-navigation/contracts/tab-navigation-contract.md` §2 add `swapHistory(next: TabHistory): TabHistory | null` to the router description and the store's `setHistory(tabId, history)` with the rules of `contracts/split-location.md` "Anschluss im Window Manager"; in the same file's FR-011 remark note that spec 022 stores histories per tab (check the actual wording first)

---

## Phase 2: Foundational A (PR A, blocks all user stories)

**Purpose**: The window manager seam for swapping a tab's history and the pure core of the split. No
behaviour visible to the user yet.

### Window manager seam (research R2)

- [x] T005 In `src/lib/wm/tabNavigation.ts` add `setTabHistory(histories: TabHistories, tabId: string, history: TabHistory): boolean`: refuses (returns `false`, changes nothing) when the tab has no history entry in `histories`, when `history.entries.length` is not within 1 and `MAX_HISTORY_ENTRIES` or `history.index` is out of bounds ("`1 ≤ entries.length ≤ MAX_HISTORY_ENTRIES`, `0 ≤ index < entries.length`" from `lib/wm/navigation.ts`); otherwise stores a copy and returns `true`. Pure, relative imports only
- [x] T006 [P] In `scripts/check-wm-nav-store.ts` (it already tests `tabNavigation.ts`; `check-wm-navigation.ts` tests `navigation.ts`) add cases for `setTabHistory`: valid history replaces; empty entries, 51 entries, negative index, index = length, unknown tab → `false` and the map is unchanged; the stored history is a copy (mutating the argument afterwards does not change the map)
- [x] T007 In `src/stores/wmNavigation.ts` add and return `setHistory(tabId, history)`: calls `setTabHistory`, and on success `lastDirection.delete(tabId)` and `deps.clearTitleOverride(tabId)`; no other behaviour
- [x] T008 In `src/stores/windowManager.ts` return `setHistory` with a `session.saveSoon()` like the three existing wrappers (`navigate`, `goTab`, `skipCurrent`, lines 441–455); the file is at 504 lines and MUST NOT grow: fold the four wrappers into one small helper (`const saving = <A extends unknown[], R>(fn: (...args: A) => R) => (...args: A): R => { const r = fn(...args); session.saveSoon(); return r }`) and use it for all four
- [x] T009 [P] `setHistory` of `wmNavigation.ts` (last direction, title override) cannot be loaded under `node --test` (it uses the Nuxt aliases), so it has no unit case; the refusal and the copy are covered by T006, the rest by the history scenarios of the E2E task for `files-split.test.ts`
- [x] T010 In `src/composables/useTabRouter.ts` add `swapHistory(next: TabHistory): TabHistory | null` to `TabRouter`: returns the tab's current history (a deep copy) and calls `wm.setHistory(tabId, next)`; `null` and no change when the store refuses `next`; the `INERT_ROUTER` returns `null` and does nothing

### Pure core of the split (research R1, R2, R4, R9; data-model.md)

- [x] T011 Create `src/lib/files/split.ts` (relative imports only, no Nuxt auto-imports; module doc comment citing spec 062 and `contracts/split-location.md`): types `SplitSide = 'left' | 'right'` and `SplitState { on: boolean; side: SplitSide; active: FilesPlace; other: FilesPlace | null }`; constants `SPLIT_PANEL_MIN_WIDTH = 360` and `fitsSplit(width: number): boolean` (`width >= 2 * SPLIT_PANEL_MIN_WIDTH`, so 720 px); `readSplit(location: TabLocation): SplitState` per `contracts/split-location.md` (`split` only `'1'` is on; `side` only `'r'` is right; `o.src` ∈ `device` | `storage:<id>` | `peer:<id>` | `share:<id>` with `<id>` matching `[A-Za-z0-9-]{1,64}`, anything else drops `other`; `o.p`, `o.q`, `o.t`, `o.s`, `o.d` strings; `other` is `null` without a valid `o.src`); `splitQuery(state: SplitState): Record<string, string>` (only `split`, `side`, `o.*`, omitted when default/empty); `withSplit(location: TabLocation, state: SplitState): TabLocation` (keeps the plain keys of `location`, replaces `split`, `side`, `o.*`); `stripSplit(location)` (removes those keys)
- [x] T012 In `src/lib/files/split.ts` add the transitions as pure functions returning a new `SplitState`: `enableSplit(state)` (`on = true`; `other` = remembered `other` or `active` per FR-005), `disableSplit(state)` (`on = false`, `other` remembered, `side` unchanged), `swapActive(state)` (exchanges `active` and `other`, flips `side`; the panels keep their place on the surface; no-op when `other` is `null`)
- [x] T013 [P] Create `scripts/check-files-split.ts` (run by `check:files`): `readSplit`/`withSplit` round trip for an off tab, an on tab with `side=r`, an on tab without `o.*`; unknown values (`split=2`, `side=x`, `o.src=storage:a/b`, `o.src=nope`) are dropped; `fitsSplit(719) = false`, `fitsSplit(720) = true`; `enableSplit` with and without a remembered `other`; `disableSplit` keeps `other` and `side`; `swapActive` twice is the identity; `withSplit` leaves the plain keys (`p`, `q`, `open`) untouched and `stripSplit(withSplit(l, s))` equals `l`; no `o.open` is ever produced (data-model.md: only the active panel has `open`)
- [x] T014 In `src/lib/files/registry.ts` leave `KEYS`/`PASSED` as they are (so `parseFilesPlace` drops `split`, `side`, `o.*`) and add to `scripts/check-files-registry.ts` the cases: `parseFilesPlace` of a location with `split=1&side=r&o.src=device&o.p=/x` returns only the plain place (no extra fields); `filesLocation(place)` never emits `split`, `side` or `o.*`
- [x] T015 In `src/lib/files/shortcuts.ts` add `split: boolean` to `FilesKeyEvent` and the commands `'switchPanel' | 'copyToOther' | 'moveToOther'` to `FilesKeyCommand`; with `split` and the focus in the list (`inList`, not in a dialog, no text selected): `Tab` without Shift → `switchPanel`, `F5` → `copyToOther`, `F6` → `moveToOther`, `Ctrl+R`/`Cmd+R` → `reload`, and `F5` is no longer `reload`; `Shift+Tab` stays unhandled (browser default); without `split` nothing changes (contracts/shortcuts-and-menu.md)
- [x] T016 [P] In `scripts/check-files-shortcuts.ts` add cases for T015: with and without `split` for `Tab`, `Shift+Tab`, `F5`, `F6`, `Ctrl+R`, `Cmd+R` on macOS; none of them act in a text field, in a dialog, on selected text or outside the list
- [x] T017 In `src/lib/files/menus.ts` add the ids `copyToOther` and `moveToOther` to `FilesCommand`; `entriesMenu` gets an option `split: boolean`; with `split` they appear between "Ausschneiden/Kopieren" and "Einfügen" with the shortcuts F5 and F6; `copyToOther` is disabled when no entry is selected or `readOnlyTarget` is set, `moveToOther` additionally when `readOnly` (the open place is read only or an entry is holzi's own, the rule of "cut"); a new option `readOnlyTarget: boolean` carries the other panel's write protection; without `split` the menu is unchanged
- [x] T018 [P] In `scripts/check-files-clipboard.ts` (it imports `entriesMenu`) add cases for T017: the two entries exist only with `split`; the disabled rules above; the existing menus are byte-for-byte unchanged without `split`
- [x] T019 In `src/lib/files/clipboard.ts` change `pasteRefusal(op, folder, items, target, sameSource = true)`: `intoItself` (`isWithin`) and `nothing` (a cut into the same folder) apply only when `sameSource` is `true`, because the paths of two sources are different namespaces (storage A `/` → storage B `/` is not "the same folder", `/backup` → `/backup/2026` of another storage is not "into itself"); `holziOwned` applies always; the default keeps every existing caller unchanged (research R7)
- [x] T020 In `scripts/check-files-clipboard.ts` add cases for `pasteRefusal`: different sources, equal folder strings and a cut → `null`; different sources, a folder item and a target below its path → `null`; same source → as before (`nothing`, `intoItself`); `holziOwned` still refuses across sources; `sameSource` omitted behaves as before
- [x] T021 [P] Add the texts to `src/i18n/locales/de.json` and `en.json` (keys by the existing `files.*` naming; check `scripts` that compare the two locales): `files.split.toggle` ("Zwei Panels" / "Two panels"), `files.split.tooNarrow` ("Das Fenster ist zu schmal für zwei Panels" / "The window is too narrow for two panels"), `files.split.panelLeft`/`panelRight` (aria labels), `files.actions.copyToOther`, `files.actions.moveToOther` ("In das andere Panel kopieren/verschieben" / "Copy/Move to other panel"), `files.error.sameFolder` ("Quelle und Ziel sind derselbe Ordner." / "Source and target are the same folder."), `files.error.noOtherPanel`

**Checkpoint**: `pnpm check:files`, `pnpm check:wm-navigation` (covers `check-wm-navigation.ts` and `check-wm-nav-store.ts`), `pnpm typecheck` and `pnpm lint` green; the app behaves as before.

---

## Phase 3: Foundational B (PR B, blocks all user stories)

**Purpose**: `FilesApp.vue` (492 lines) becomes the frame, its panel part becomes `Panel.vue`. **No
visible change**: every existing `files-*` e2e scenario and every `data-testid` stays as it is.

- [x] T022 Create `src/composables/useFilesSplit.ts` with the frame state for one panel only (no split yet): reads `readSplit(router.route)`/the plain place via `parseFilesPlace`, provides `place`, `go(place, { replace })` and `setQuery(patch, { push })` that write through `useTabRouter()` and keep `split`/`side`/`o.*` (`withSplit`) — with a single panel they are always empty; exports the `FILES_PANEL_KEY` injection key and the `FilesPanelContext` type of `contracts/panel-context.md` (`side`, `place`, `active`, `split`, `go`, `setQuery`, `register`, `other`, `activate`, `switchPanel`, `sources`, `reloadSources`, `tabId`); for now `split` is `false`, `other()` is `null`, `activate`/`switchPanel` are no-ops
- [x] T023 [P] Create `src/components/files/PathBar.vue` (auto-imported `FilesPathBar`): the up button, the path crumbs and the reload button out of `src/components/files/Toolbar.vue` (lines of `crumbs`, `canGoUp`, `rootLabel`, `@go`, `@up`, `@reload`) with the same `data-testid`s; props and emits as the toolbar had them
- [x] T024 Change `src/components/files/Toolbar.vue`: keep the sidebar button, back/forward (`router.back()`/`router.forward()`), view toggle, sort and hidden toggle; the path bar becomes a default slot filled by the frame with `<FilesPathBar>` so the single-panel layout stays identical; file ≤ 200 lines
- [x] T025 Create `src/components/files/Panel.vue` (auto-imported `FilesPanel`): move into it from `src/components/apps/FilesApp.vue` everything that belongs to one place: `useFilesFolder`, `useFilesTabShares`, `useFilesAllAccess`, `useFilesSearch`, `shown`, `onSearch`/`onFilter`, `go`/`goUp`/`openHit`/`activate`, `useFilesManage`, `openEntry`/`closeViewer`/`showSibling`, `useFilesKeyboard` (the panel root handles key events originating inside the panel), the start-folder `watchEffect` and `openPeerStartAsync`, the storage-tab reload on `tabActive`, and the template parts `FilesSearchBar`, `FilesForeignHint`, `FilesAllFilesAccess`, `FilesSearchResults`, `FilesDropTarget` + `FilesMenu` + `FilesFolderView`, `FilesActionBar`, `FilesViewer` and the dialogs (`FilesNameDialog`, `FilesShareDialog`, `FilesEncryptedNotice`, `FilesOpenSystemNotice`, `FilesDeleteDialog`). The panel reads its place and writes only through the injected `FilesPanelContext` (T022), never `useTabRouter()`; it takes `sources` from the context. Keep the existing `data-testid`s and `data-files-list`; add `data-files-panel` ("left"|"right") and `data-active`. `FolderView` gets the prop `takesFocus` (default `true`) that gates its `takeFocus()`; the panel passes `context.active` (always `true` while there is one panel), so an inactive panel never takes the focus by itself (research R4). File ≤ 500 lines (if not, move the dialogs into `PanelDialogs.vue`)
- [x] T026 Rewrite `src/components/apps/FilesApp.vue` as the frame (≤ 200 lines): `sourcesAsync` (`useAsyncState`) and `reloadSources`, `useFilesSplit` (provide the context), `FilesToolbar` with `FilesPathBar` for the one panel, the sidebar (`useSidebarFrame`, `onSidebarPick`, `onPickStorage`, `openStorageSettings`, `storageName`) writing through `context.go`, one `<FilesPanel side="left">`, `<FilesTransferBar :tab-id>`, and the tab clean-up (`releaseTabAsync`, `openFrames`, `clearFilesThumbnails`, `clearFileIcons`). The frame owns the viewer host element (covering the panel area; an element ref handed to the panels through the context, not an id, as several file browser tabs are mounted at once) and the panel teleports its viewer there with `<Teleport defer :to="viewerHost">`; it also handles active-panel shortcuts while focus is on shared toolbar/frame content outside either panel, delegating to the active panel's keyboard handler without reprocessing events that originated inside a panel (FR-016)
- [ ] T027 [P] Run the whole existing file browser suite unchanged against the refactor (`pnpm check:files`, `typecheck` and `lint` ran locally; the e2e scenarios ran only in CI, as the local rig is blocked by WebKitWebDriver 2.54.1 vs the host's 2.54.2, see memory "e2e build gotchas"): `pnpm check:files`, `pnpm typecheck`, `pnpm lint`, and `pnpm test:e2e --grep files-` (basic, manage, search, keyboard, restore, storage, media, large-folder, encrypted); fix regressions in the refactor, not in the tests

**Checkpoint**: no visible change; PR B is reviewable as "pure move" (compare with `git diff --color-moved`).

---

## Phase 4: User Story 1 - Zwei Ordner nebeneinander und dazwischen kopieren (Priority: P1) 🎯 MVP

**Goal**: Switch the split on, two independent panels, active panel with Tab, F5 copies and F6 moves into the
other panel, each panel with its own history (spec US1, FR-001–FR-008, FR-010–FR-013, FR-015, FR-018,
FR-020, FR-024).

**Independent Test**: quickstart M1–M5, M9, M14, M14a (with the e2e scenario of T043).

### Split state and the two panels (PR C)

- [x] T028 [US1] Extend `src/composables/useFilesSplit.ts` to the full frame state of `data-model.md`: `SplitState` from the location on mount (`readSplit`), kept in the frame afterwards; `shown` = `on && available && fits` (T040/T041 wire `available` and `fits`; here both are `true` for the desktop); a watcher on the tab route writes the mirror (`withSplit`) back with `router.replace` whenever the route's `split`, `side` or `o.*` differ from the state (after Back/Forward, session restore, an outside push; before a panel swap, refresh `active` from the plain tab location; skip when `locationsEqual`); when mounting an enabled split with `other === null`, initialize the second place from the active place; `toggle()` uses `enableSplit`/`disableSplit` and writes with `replace`; `lane: TabHistory | null` kept in memory and reset to a fresh one-entry history when the split turns on and dropped when it turns off
- [x] T029 [P] [US1] Extend `scripts/check-files-split.ts` with a pure model of the swap used by T030 (a function `swapLanes(tabHistory, lane, newLocation)` in `src/lib/files/split.ts` returning `{ tab, lane }`): the incoming history's current entry takes `newLocation`; the outgoing history becomes the lane unchanged; a missing lane gives a one-entry history at `newLocation`; the sizes stay within `MAX_HISTORY_ENTRIES`; add and test it, then use it in T030 _Done: `swapLanes` became `incomingHistory(lane, location)` in `lib/files/split.ts`: the outgoing history is what `router.swapHistory` returns, so only the incoming one needs building._
- [x] T030 [US1] In `src/composables/useFilesSplit.ts` implement `activate(side)`: if the side already is the active one do nothing; else `swapActive` on the state, build the new location (`withSplit` on the new active place), `const previous = router.swapHistory(lane ?? createHistory(newLocation))` with the current entry of the incoming history replaced by the new location, store `previous` as the new `lane`, and write the mirror; `go(place, options)` of an inactive panel first calls `activate`, then navigates; `switchPanel()` activates the other side and calls the other handle's `focus()`
- [x] T031 [US1] In `src/composables/useFilesSplit.ts` add `register(handle)`/`other()` (the `PanelHandle` of `data-model.md`: `side`, `target()`, `reload()`, `focus()`) so a panel finds the other one; unregister on unmount
- [x] T032 [US1] In `src/components/apps/FilesApp.vue` render the two panels: `<FilesPanel side="left">` and `<FilesPanel side="right">` side by side in the panel area when `shown`, each a flex child with `min-w-0`; with `shown` false only the active one (the other stays mounted and hidden with `hidden`, so its folder, selection and watcher stay); add the split toggle to `Toolbar.vue` (`UiButton` `variant="ghost"` `size="icon"`, icon `lucide:columns-2`, `aria-pressed`, `data-testid="files-split-toggle"`, label `files.split.toggle`; no keyboard shortcut for the toggle)
- [x] T033 [US1] In `src/components/files/Panel.vue` mark the active panel: 2 px ring in the primary colour on the panel root and the path bar in the text colour instead of muted (not by colour alone, FR-004); `pointerdown` and `focusin` on the panel root call `context.activate()`; the root is `role="region"` with `aria-label` `files.split.panelLeft`/`panelRight` while two panels show; the panel's `FilesPathBar` is shown inside the panel header when `context.split` (in the single-panel layout it stays in the toolbar slot of T024)
- [x] T034 [US1] In `src/components/apps/FilesApp.vue` make the sidebar act on the active panel: `onSidebarPick` and `onPickStorage` call `context.go` of the active side; `current`/`current-storage` highlight the active panel's place
- [x] T035 [US1] In `src/composables/useFilesKeyboard.ts` pass `split` (from the context) into `resolveFilesShortcut` and handle the three new commands: `switchPanel` → `context.switchPanel()`, `copyToOther` → `runKey('copyToOther', selection)`, `moveToOther` → `runKey('moveToOther', selection)`; `preventDefault` for them; `reload` stays as it is

### Copy and move between the panels (PR C)

- [x] T036 [US1] In `src/composables/useFilesManage.ts` generalise the target source (R7): `startAsync(op, from, paths, to, onDone?, targetSource = source.value)` passes the target source to `transfers.startAsync`; `placeAsync(clip, target, targetOwned, targetSource = source.value, onDone?)` passes `onDone` on and calls `pasteRefusal(…, sameSource)` with `sameSource = JSON.stringify(clip.source) === JSON.stringify(targetSource)` (the comparison `onDrop` already uses; use one helper for both); export `placeAsync`; nothing else changes (file stays under 500 lines, 465 + ≈ 10)
- [x] T037 [US1] Create `src/composables/useFilesOtherPanel.ts` (≈ 80 lines): given the manage composable's `placeAsync`, `selection`, `source`, `path`, `readOnly`, `entryAt`/`shown`, `reload` and the context's `other()`, implement `copyToOther()`/`moveToOther()`: no other panel or no target path → toast `files.error.noOtherPanel`; no selection → nothing; build the clipboard `{ op, source, folder: path, items }` from the selection and call `placeAsync(clip, other.path, other.readOnly, other.source, onDone)` (the refusal rules of `pasteRefusal` apply with the sources compared, task on `clipboard.ts` above); a refusal `nothing` on a move into the same folder shows the toast `files.error.sameFolder` (FR-013); on success the transfer's `onDone` reloads this panel and `other().reload()`; a successful move clears the selection (as `remember` does); returns the three functions plus `canOther` for the menu
- [x] T038 [US1] Wire it in `src/components/files/Panel.vue` and `useFilesManage.ts`: `useFilesManage` accepts the option `otherTarget?: () => { readOnly: boolean } | null` and `split?: Ref<boolean>`, passes `split` and `readOnlyTarget` into `entriesMenu` (T017) and runs `copyToOther`/`moveToOther` in `run()` (`case 'copyToOther'`/`'moveToOther'` calling the T037 functions, which the panel passes in); the panel registers its `PanelHandle` (`target()` = `{ source, path, readOnly }` from its own state, `reload`, `focus`; both exist since T025) _Done: `focus()` of the handle uses `focusEntry` (the selection anchor, else the first entry), so `FolderView` needs no `focusLast()`._
- [x] T039 [P] [US1] Extend `src/components/files/ActionBar.vue`: with `split` two buttons "In das andere Panel kopieren/verschieben" next to the existing ones (`data-testid="files-bar-copy-other"`/`"files-bar-move-other"`), disabled by the same rules as the menu, each with the shortcut (F5/F6) in its tooltip
- [x] T040 [US1] Create `src/composables/useSplitFit.ts`: `useSplitFit(areaRef)` measures the panel area with a `ResizeObserver` (disconnect on unmount) and returns `fits = computed(() => fitsSplit(width))`; without a `ResizeObserver` (tests) it returns `true`; use it in the frame to set `shown`; the toolbar toggle is `disabled` with the tooltip `files.split.tooNarrow` whenever `!fits` and the frame shows only the active panel when `on && !fits` (FR-022) _Done: the toggle is disabled only while the split is off and too narrow (`!fits && !on`), so a split that became too narrow can still be turned off._
- [x] T041 [US1] In the frame read the platform from `useDeviceCapabilities()` (`capabilities.value?.platform`): `available = platform !== 'android' && platform !== 'ios'` (until the table arrives and in a plain browser preview: `true`); the toggle is not rendered when `!available`, and `split=1` in the location is then treated as off without touching it (FR-021). Document this beside the code _Done: `splitAvailable(platform)` is a pure function in `lib/files/split.ts` with its own check (T061 reuses it)._
- [x] T042 [US1] Create `scripts/e2e/lib/files-split.ts`: helpers that scope a selector to a panel with `[data-files-panel="left"|"right"]` (`panelExec(instance, side, script)`, `panelShows(instance, side, name)`, `panelClick(instance, side, testId)`, `activePanel(instance)` reading `data-active`), used by every split scenario (analysis D5)
- [x] T043 [US1] Create `scripts/e2e/scenarios/files-split.test.ts` (style of `files-keyboard.test.ts`; use `deviceFiles` from `../lib/extension-files.ts` and the panel helper of the task above for every selector (two panels repeat `data-testid`s such as `files-search` and `files-entry-<name>`) to prepare `split-a` with `eins.txt`, `zwei.txt`, `drei.png`, a sub-folder `unter/` and an empty `split-b`): US1 scenarios — toggle on shows two panels with the second at the first's place and the first active; independent navigation/selection; F5 copies, F6 moves into the other panel (check the files on disk); Tab switches the active panel and the list focus; F5/F6 without a selection do nothing; F5 with both panels in the same folder opens the conflict question, F6 shows the same-folder toast; two panels in the same folder show an external change in both (FR-018); `Ctrl+R` reloads and outside the split F5 still reloads (adapt `files-keyboard.test.ts` for that case); the first panel does not take the focus when the second one is the active one at start (`side=r` restored); each panel's own history: left two folders deep, right one, `Alt+Left` with right active goes back on the right only, `Tab`, `Alt+Left` goes back on the left only (US5 scenario 4) _Done: not yet in the scenario: F5 with both panels in the same folder (conflict question), `Ctrl+R`, and the restore with `side=r` (the last one belongs to US4, T055); `Ctrl+R` and the F5 rule are covered by `check-files-shortcuts.ts`._
- [ ] T044 [US1] Run `pnpm check:files`, `pnpm typecheck`, `pnpm lint` and `pnpm test:e2e --grep files-split`, then do quickstart M1–M5, M9, M14, M14a by hand

**Checkpoint**: PR C. US1 works on its own: two panels, copy/move by key and menu, own history per panel.

---

## Phase 5: User Story 2 - Zwischen Panels ziehen (Priority: P2)

**Goal**: Drag between the panels with the existing rules, refused targets refuse at hover (spec US2, FR-009,
FR-011).

**Independent Test**: quickstart M7, M8 (drag part).

- [ ] T045 [US2] In `src/components/files/FolderView.vue` add the prop `droppable: boolean` (default `true`): `onDragOver` sets `event.dataTransfer.dropEffect = 'none'` and does not set `dropOver` when `!droppable`, and `onDrop` ignores the drop; the file is currently about 620 lines, so move the drag handlers (`onDragStart`/`onDragOver`/`onDrop`) into `src/composables/useFilesDrag.ts` before adding `droppable`; keep T045 scoped to PR D
- [ ] T046 [US2] In `src/components/files/Panel.vue` pass `:droppable="!readOnly"` to `FilesFolderView` (`readOnly` = holzi's own folder or a share read only); for a folder row that is holzi's own (`entry.holziOwned`) the existing refusal stays (a toast), the row itself is not a hover target
- [ ] T047 [P] [US2] Extend `scripts/e2e/scenarios/files-split.test.ts` with the US2 scenarios: dragging a selection onto a folder of the other panel moves it (same source), with Ctrl (macOS Alt) copies; onto the other panel's empty area lands in its open folder; between device and a storage always copies; a read-only target refuses at hover (`dropEffect` none) and changes nothing; dragging files in from the system (`importDropped`) into either panel is covered by the existing `files-manage` test, so only assert that both `FilesDropTarget`s exist
- [ ] T048 [US2] `pnpm check:files`, `pnpm typecheck`, `pnpm lint`, `pnpm test:e2e --grep files-split`; quickstart M7, M8

**Checkpoint**: US1 + US2 work independently.

---

## Phase 6: User Story 3 - Panels mit verschiedenen Quellen (Priority: P2)

**Goal**: Each panel picks any source; copying between sources and read-only targets refuse cleanly; a failing
source breaks only its panel (spec US3, FR-010, FR-011, FR-014, FR-020).

**Independent Test**: quickstart M6, M8 (F5 part).

- [ ] T049 [US3] Verify in `src/components/apps/FilesApp.vue` and `useFilesSplit.ts` that `onPickStorage`/`onSidebarPick`/the storage settings link go to the **active** panel only and that two panels may have different `source` values (`/device` and `/storage/:id` in `o.src`); if the sidebar highlight or `useFilesTabShares` assumed one source per tab, fix it in the panel, not in the transfer code
- [ ] T050 [US3] In `src/composables/useFilesOtherPanel.ts` map the refusals of the other panel's target to the existing messages: `readOnly` (a share read only → `files.error.<existing code>` from `useFilesManage`'s read-only notice, check its exact key), holzi's own place (`holziOwned`), a failing source (`credentials`, `noAccess`: the panel already shows the error; `copyToOther` then shows that code through `errorText`); nothing is changed on refusal
- [ ] T051 [P] [US3] Extend `scripts/e2e/scenarios/files-split.test.ts` with the US3 scenarios against the local S3 server the rig already uses in `files-storage.test.ts` (reuse its setup helper): left device, right the storage, F5 uploads a folder, cancel leaves no half object, F5 back downloads; pick another source on the right in the sidebar and the left stays; a panel whose storage is down shows its error while the other stays usable (`files-storage-down.test.ts` pattern); F5 into a read-only target refuses with the reason and changes nothing
- [ ] T052 [US3] `pnpm test:e2e --grep files-split`, quickstart M6, M8

**Checkpoint**: US1–US3 work.

---

## Phase 7: User Story 4 - Split merkt sich Orte und Zustand (Priority: P3)

**Goal**: Off/on keeps the places; session restore brings the split back (spec US4, FR-005–FR-007).

**Independent Test**: quickstart M10, M11, M14b.

- [ ] T053 [US4] Verify in `useFilesSplit.ts`: `toggle()` off keeps `other` and `side` in the location (`o.*`, `side`), drops `lane`; on again restores `other` if its source still resolves (the panel shows the source's own error otherwise, FR-020) and starts the second panel with a fresh one-entry history; the survivor panel keeps its history (it is the tab's); no extra history entry is pushed (`replace`); with the right panel active when switching off, the surviving panel is the right one and sits where it stood (data-model.md "Ausschalten")
- [ ] T054 [US4] Session restore (spec 022): on mount `readSplit` reads `split`, `side`, `o.*` from the restored entry; the router's restored history is the active panel's, `lane` starts empty; check `src/lib/wm/session.ts` `parseTab`/`parseHistory` keep the extra query keys untouched (they are plain strings), and add a case to `scripts/check-wm-session.ts` that a history entry with `split=1&side=r&o.src=storage:x&o.p=/y` survives `parseWmSession` unchanged
- [ ] T055 [P] [US4] Extend `scripts/e2e/scenarios/files-restore.test.ts` (the existing restore scenario; read it first) or `files-split.test.ts` with US4: split on, panels in different folders (left with some steps), restart with session restore (the helper that scenario already uses): the tab is in the split with both places and the active side; `Alt+Left` goes back on the active panel; the other panel starts with an empty history; off/on keeps both places; a second tab stays unchanged
- [ ] T056 [US4] `pnpm check:wm-state` (covers `check-wm-session.ts`), `pnpm test:e2e --grep files-`, quickstart M10, M11, M14b

**Checkpoint**: US1–US4 work.

---

## Phase 8: User Story 5 - Gleichzeitige Nutzung ohne Überraschungen (Priority: P3)

**Goal**: Clipboard, viewer, search, history and narrow/mobile follow the rules (spec US5, FR-012, FR-014–FR-016,
FR-019, FR-021–FR-024).

**Independent Test**: quickstart M12, M13, M14, M14a, A1.

- [ ] T057 [US5] In `src/components/files/Panel.vue` make sure `Strg+C/X/V`, `Entf`, `F2` and the other shortcuts of FR-017b act on the panel with the focus (the `@keydown` is on the panel root, T025) and `Strg+V` pastes into the folder of that panel; the clipboard store stays global (`useFilesTransfersStore`), so copying in one panel and pasting in the other works
- [ ] T058 [US5] Viewer (FR-019): the viewer teleports to the frame's viewer host (T026) covering both panels; "previous/next" uses the siblings of the panel that opened it; closing restores the split; only the active panel can have `open` (an `open` in the route belongs to the active panel by contract); verify `files_open(tabId)` media URLs still end with the tab (spec 044 FR-016) and that switching the active panel while the viewer is open is impossible (the viewer covers the panel area)
- [ ] T059 [US5] Search and filter per panel (FR-016): `Strg+F` focuses the search bar of the panel with the focus; `q`/`t`/`s`/`d` of the inactive panel live in `o.*` and survive a panel switch (they are carried by `swapActive`); a search running in one panel does not slow the other (separate `useFilesSearch` instances); add a pure check in `scripts/check-files-split.ts` that `swapActive` carries `q`, `t`, `s`, `d`
- [ ] T060 [US5] Narrow window (FR-022): verified in T040; add the `ResizeObserver` path to the e2e by setting the window size through the rig helper (check `scripts/e2e/lib/` for a resize helper; if there is none, assert the pure `fitsSplit` check only and do M13 by hand)
- [ ] T061 [US5] Phones (FR-021): confirm T041 with a unit case in `scripts/check-files-split.ts` (`splitAvailable(platform)` as a pure function in `src/lib/files/split.ts`: false for `android` and `ios`, true otherwise and for `undefined`) and an assertion in the existing Android job `files-basic` (or the Android all-files scenario) that `files-split-toggle` does not exist
- [ ] T062 [P] [US5] Extend `scripts/e2e/scenarios/files-split.test.ts` with the US5 scenarios: `Ctrl+C` in one panel, `Tab`, `Ctrl+V` pastes into the other's folder; opening an image in the left panel shows the viewer over both and "next" stays in the left folder; `Ctrl+F` searches only in the panel with the focus; keyboard reachability (FR-024): from a list `Shift+Tab` reaches the toolbar and the sidebar toggle, `Tab` toggles between the lists
- [ ] T063 [US5] FR-023 (agents): `src/stores/filesActionHandlers.ts` `files.show` calls `wm.openApp(FILES_APP_ID, filesLocation(…))`, a location without `split`, `side` and `o.*`; check what `openApp` does with an existing file browser tab (reuse or new tab). If it reuses a tab in the split, the frame mirror (T028) must restore the split after the push; if it opens a new tab, nothing is needed. Add the case to `scripts/e2e/scenarios/files-agent.test.ts` (read it first): with a tab in the split, `files.show` for a folder or a file leaves the split on and shows the folder or the viewer in the active panel
- [ ] T064 [US5] `pnpm check:files`, `pnpm test:e2e --grep files-split`; quickstart M12, M13, M14, M14a and (with a device or emulator) A1

**Checkpoint**: all five stories work.

---

## Phase 9: Polish & Cross-Cutting (PR E)

- [ ] T065 [P] SC-006 (the other panel stays responsive): add a scenario to `scripts/e2e/scenarios/files-large-folder.test.ts` (or `files-split.test.ts`) that opens the 50 000-entry folder in one panel and measures a selection and a navigation in the other under 200 ms, with the CI-runner tolerance that scenario already uses (spec 044 FR-008 note); if it fails, apply research R10's fallback (one shared folder state per source and path) as a separate task
- [ ] T066 [P] Update `specs/044-file-browser/quickstart.md` or the user-facing help text of the file browser if it lists shortcuts (grep `F5` in `src/i18n/locales/*.json` for a shortcut hint and the context menu's shortcut labels) so that F5 is described as "Neuladen" outside the split
- [ ] T067 SC-007: check `.github/workflows/*.yml` for the jobs that run `pnpm check:files` and `pnpm typecheck` on Linux, Windows and macOS; if one desktop job lacks them, add the step (without new tooling); note in `specs/062-files-split-view/quickstart.md` that the E2E scenario `files-split` runs on Linux only and M13 and A1 stay manual
- [ ] T068 Set the status of `specs/062-files-split-view/spec.md` to "Implemented (manual M6, M13, A1 open)" or "Implemented" and tick the checklist items; update the row in `plans/README.md`; if the README of the file browser or the memory-style docs mention one panel, fix them
- [ ] T069 Final run as in CI: `pnpm lint:all`, `pnpm typecheck`, `pnpm check:files`, `pnpm check:wm-navigation`, `pnpm check:wm-state`, `pnpm test:e2e` for the affected scenarios (`files-`), and review the diff size per PR against the thresholds of the constitution (about 100 net lines is a review warning, approaching 1000 a strong prompt to split)

---

## Dependencies & Execution Order

- Phase 1 → Phase 2 (PR A) → Phase 3 (PR B) → US1 (PR C). US2 and US3 (PR D) depend on US1; US4 and US5 (PR E) depend on US1.
- Inside Phase 2: T005 → T006, T007 → T008 → T009, T010 (needs T008); T011 → T012 → T013, T014; T015 → T016; T017 → T018; T019 → T020 (before the F5/F6 tasks of US1); T021 any time. The seam (T005–T010) and the pure core (T011–T021) are independent of each other.
- Inside Phase 3: T022, T023 first (parallel), then T024, T025, then T026, then T027.
- Inside US1: T028 → T029 (`swapLanes`) → T030 → T031; T032 needs T028–T031; T033, T034, T035 after T032; T036 (needs T019) → T037 → T038; T039 after T038; T040, T041 any time after T032; T042 before T043; T043 last.
- US2 (T045 → T046) and US3 (T049, T050) are independent of each other once US1 is done.
- US4 and US5 touch different parts of `useFilesSplit.ts`/`Panel.vue` and can run in parallel after US1; T063 needs T028.

## Parallel Examples

- Phase 2: `T006` (WM pure checks), `T013` (split checks), `T016` (shortcut checks), `T021` (texts) are different files; `T018` and `T020` both edit `check-files-clipboard.ts`, so they run one after the other.
- Phase 3: `T023` (PathBar) while `T022` (frame composable) is written.
- After US1: US2 (`FolderView.vue`), US3 (`useFilesOtherPanel.ts`) and US4 (session) in parallel.

## Implementation Strategy

- **MVP**: Phases 1–4 (PR A, B, C): the split with copy and move by key and menu and a history per panel.
- Then PR D (drag and sources), then PR E (memory, integration, polish). Each PR keeps the app releasable;
  the split toggle is the only visible entry point, so a PR before US1 is invisible to users.
- Manual-only checks that cannot run in CI: M6 (real S3 store), M13 (window resize), A1 (Android device).
