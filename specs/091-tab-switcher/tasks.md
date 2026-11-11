# Tasks: Switching tabs by hand

**Input**: Design documents from `specs/091-tab-switcher/`

**Prerequisites**: [plan.md](plan.md), [spec.md](spec.md), [research.md](research.md), [data-model.md](data-model.md),
[contracts/](contracts/), [quickstart.md](quickstart.md)

**Tests**: requested by the constitution (a check script for each piece of TypeScript logic) and by the spec's
independent tests. Test tasks come before the code they test.

**Order**: the phases follow the plan's steps, not the spec's priority order. **Part A** (phases 1–3: the app tiles and
the launcher, User Story 6) needs nothing from spec 090 and lands first; it also builds the grid User Story 1 needs.
**Part B** (phases 4–7) needs spec 090 merged (its header, its fixed window-button area, its inert header controls).

| Step (= PR) | Phases                                                          |
| ----------- | --------------------------------------------------------------- |
| A           | 1 Setup, 2 Foundational A, 3 US6                                |
| B           | 4 Foundational B, 5 US1 + US2, 6 US3 + US4 + US5, 7 Clean-up    |

**Rules for every task**:

- Run `graphify query "<what the component does>"` before writing a new named component, module or function (spaex);
  the launcher grid is the match to extend (research R8).
- Keep each file at most 500 lines. `src/stores/windowManager.ts` is at 501: do not add to it.
- Before every push: `pnpm typecheck`, `pnpm typecheck:scripts`, `pnpm lint`, `pnpm format:check`, and **all**
  `pnpm check:*` after a change under `src/lib/wm/` or to a composable (Node sandbox: no DOM globals, no
  `instanceof Element`).
- No new dependency; `swiper` is loaded on demand only (`defineAsyncComponent`).
- Texts in `src/i18n/locales/de.json` and `en.json` in lockstep.
- Commits: Conventional Commits, English, short, **no agent trailer**; branches from `git switch -c <name> --no-track
  origin/main`; E2E does not run on pull requests, so before merging a step run `gh workflow run ci.yml --ref <branch>`.
- Mark known ceilings in code with `ponytail:` (the ceiling and the upgrade path), cite spec 091 FR numbers where a
  rule comes from.

## Format: `- [ ] [ID] [P?] [Story] Description with file path`

---

## Phase 1: Setup (Part A)

- [ ] T001 Find every E2E scenario and check script that uses the launcher grid or the old tab strip, so nothing is
      lost: `grep -rn "open-launcher\|openLauncher\|data-app-id\|open-chat\|launcher-menu-pin\|lock-instance"
      scripts/e2e`, and `grep -rln "tab-close\|wm-new-tab" scripts/e2e` (today `compact-tab-close.test.ts`,
      `tab-content-isolation.test.ts`, `chat-multi-instance.test.ts`); write the two lists at the end of
      `specs/091-tab-switcher/research.md` R9 as the moved-scenario list
- [ ] T002 Run `graphify query "grid of app tiles with swipe pages"` and `graphify query "overlay of tiles for open tabs"`;
      note the candidates and verdicts in `research.md` R8 if the graph returns anything new

---

## Phase 2: Foundational A

- [ ] T003 [P] Write `scripts/check-wm-app-pages.ts` for the pure module of T004: `tileGrid` (compact is 2 columns;
      wide windows 3–6 columns; `rows` fixed or `'fit'` from height; never fewer than one row or the minimum
      columns), `pagesOf` (0 apps is one empty page, 50 apps, exact multiples), `clampPage` (apps removed so the last
      page vanishes); add the file to `check:wm-state` in `package.json`; it fails until T004
- [ ] T004 Implement `src/lib/wm/appPages.ts` ([data-model.md](data-model.md) §TileGrid: `GAP` 12, `MIN_TILE_WIDTH` 112,
      `TILE_HEIGHT` 104; pure, relative `.ts` imports); make T003 pass

---

## Phase 3: User Story 6 - The launcher scales the same way (Priority: P3, Part A)

**Goal**: the launcher's app tiles are in pages with swipe, keys and an indicator; its own tiles stay in a fixed strip.

**Independent Test**: with 40 apps the launcher pages; swipe to the last app and open it; the lock tile is visible on
every page; with six apps it looks as today.

- [ ] T005 [US6] Create `src/components/wm/AppTiles.vue` by extracting the app tile (`v-for` of buttons, the attention
      dot, icon, disabled tile with `unavailableKey`, the `ShadcnContextMenu` pin menu) from
      `src/components/wm/Launcher.vue`; props `apps`, `rows` (`number | 'fit'`), `markOpen`, `pinMenu`, emits
      `pick(appId)` ([contracts/app-tiles.md](contracts/app-tiles.md)); with `markOpen` an app that may be open only
      once and already is gets an "open" label (move `isOpenElsewhere` from `NewTabMenu.vue`: read it from git history
      if spec 090 already removed the file); keep `data-app-id`, `open-chat` and `launcher-menu-pin`; one page only at
      this task
- [ ] T006 [US6] Add the pages to `AppTiles.vue`: a `ResizeObserver` on its container feeds `tileGrid`; more than one
      page mounts an async-loaded `swiper` (`defineAsyncComponent`, pattern of `passwords/EntryTabs.vue` and
      `EntryTabsSwiper.vue`: `threshold` 8, `Mousewheel` with `forceToAxis`, speed 0 under `prefers-reduced-motion`),
      one slide per page; hidden slides `inert` and `aria-hidden`; a page indicator of buttons "Page X of Y" with
      `aria-current`; `PageUp`/`PageDown` change the page, the arrow keys at a page's edge move to the neighbouring
      page; `clampPage` when the app list changes; opens on page 0; one page mounts no Swiper and no indicator
      (FR-003a); a swipe never emits `pick`
- [ ] T007 [US6] Rewrite `Launcher.vue` to use `<AppTiles :apps="wm.apps()" :rows="wm.compact ? 4 : 3" pin-menu
      @pick="launch">` inside the drawer, and keep the voice-assistant tile and the lock tile in a fixed strip below it
      (`launcher-assistant-toggle`, `lock-instance`); `launch` still opens a window through `wm.app.open`; the file
      shrinks
- [ ] T008 [US6] Check swipe versus click versus long-press on touch (research R6): on the Android emulator (set up as
      `specs/043-android-build/quickstart.md` describes; use an AVD with API 35, uninstall the app before each install, and
      delete the AVD and its images afterwards so the disk does not fill) swipe from a tile and
      long-press a tile; a swipe must not open the pin menu or the app. If the menu opens, disable the long-press
      menu while a drag is in progress and record the fix in `research.md` R6
- [ ] T009 [P] [US6] Add the page indicator texts ("Page X of Y", "Previous page", "Next page") to `de.json` and
      `en.json`
- [ ] T010 [US6] Create `scripts/e2e/scenarios/launcher-pages.test.ts`: get 40 apps (decide by measuring: install the
      `probe` fixture under several names as `extension-install-open.test.ts` does, or add a test-only app-list
      override; record the choice in `research.md` R9); the launcher shows an indicator and pages; paging by keys reaches
      the last app and opens it as a window; the assistant/lock tiles are visible on every page (`lock-instance`); with
      the six system apps there is no indicator; run the existing launcher users from T001 (`dock.test.ts`,
      `chat-multi-instance`, `openLauncher` flows) and confirm they pass
- [ ] T011 [US6] Run `pnpm check:wm-state`, `pnpm check:templates`, `pnpm knip` (nothing left unused after the
      extraction), the quickstart's Part A walk-through

**Checkpoint (end of Part A)**: open the PR for phases 1–3; `gh workflow run ci.yml --ref <branch>`.

---

## Phase 4: Foundational B (needs spec 090 merged)

- [ ] T012 [P] Write `scripts/check-wm-tab-switcher.ts` for the pure module of T013: `tabButtonFace` (1 → plus; 2, 99 →
      count; 100 → `99+`), the mode transitions of [data-model.md](data-model.md) (every row of the table, including
      "apps from overview goes back to the overview" and "count falls to one closes"), `moveFocus` (arrows by one and by
      a row, Home/End, no wrap, right-to-left swaps left and right, the "+" tile as the last index), the attention
      rule (an inactive tab only); add the file to `check:wm-navigation` (spec 090 also edits the `check:wm-*` lines of `package.json`: resolve a rebase
      conflict there by keeping both additions); it fails until T013
- [ ] T013 Implement `src/lib/wm/tabSwitcher.ts` (pure, relative `.ts` imports) and make T012 pass
- [ ] T014 Create `src/stores/wmTabSwitcher.ts`: the reactive `Map<windowId, {mode, appsFrom}>` with `open`, `toggle`,
      `close`, `back`, `closeAll`, `isAnyOpen`, built on T013's transitions; expose it from
      `src/stores/windowManager.ts` with the fewest lines possible **after first moving at least 10 lines of existing
      code out of it** so it ends at or below 500; the entry of a closed window is pruned (as `syncTabRuntime` does for
      tabs); extend `scripts/check-wm-state.ts` for the prune
- [ ] T015 Hook the system back gesture in: `createWmNavigation` in `src/stores/wmNavigation.ts` gets the dependencies
      `tabSwitcherOpen(): boolean` and `closeTabSwitchers(): void`; `systemBack` counts an open switcher as
      `overlayOpen` and the `closeOverlay` branch calls `closeTabSwitchers()`; add the case to
      `scripts/check-wm-navigation.ts` (`closeOverlay` while a switcher is open, then `back` as before)

---

## Phase 5: User Stories 1 and 2 - "+" and the tab overview (Priority: P1, Part B)

**Goal**: a window with one tab has a "+" that opens apps as tiles; with two or more the number opens the overview.

**Independent Test (US1)**: one tab, press "+", pick an app: it opens as a tab and the button shows 2. **(US2)** with
three tabs, press the number, pick another tile: that tab shows, the overview is gone.

- [ ] T016 [US1] Create `src/components/wm/TabButton.vue` ([contracts/tab-button.md](contracts/tab-button.md)): the "+" or
      the number from `tabButtonFace`, the attention dot (an inactive tab with `wm.runtimeFor(id).attention`), the
      accessible name, `aria-expanded`/`aria-controls`, the press calls `toggle`/`open` of T014; mark the press
      `action-exempt:` with the reason (research R2); 44 × 44 px when compact; `data-testid="tab-button"`
- [ ] T017 [P] [US2] Create `src/components/wm/TabOverview.vue` ([contracts/switcher-overlay.md](contracts/switcher-overlay.md)):
      a tile per tab in order from `wm.tabDisplayInfo` with the place from `wm.historyOf(tabId)`, the active tile with a
      ring and a check icon (not colour alone), the attention dot, the close button (wired in T022), the trailing "+"
      tile; the grid columns from `tileGrid`; vertical scroll; `data-testid` `tab-tile-<tabId>`, `tab-tile-new`;
      picking a tile calls `wm.tab.activate` through `useAction` and closes (the active tile only closes)
- [ ] T018 [US1] Create `src/components/wm/TabSwitcher.vue`: the overlay (`absolute inset-0 bg-background`, mounted only
      while open, `data-testid="tab-switcher"`, `data-mode`), mode `tabs` renders `TabOverview`, mode `apps` renders
      `<AppTiles rows="fit" mark-open @pick>` whose `pick` calls `wm.tab.new {windowId, appId}` through `useAction` and
      closes; Escape and the button close or go back as the transitions say; a `watch` closes it when the tab count
      falls to one; focus moves in on open (active tile, or the first tile of the page) and returns to the tab button;
      no animation under reduced motion
- [ ] T019 [US1] Wire `src/components/wm/Window.vue`: render `<WmTabButton>` in the window-button group before
      `<WmWindowControls>` (spec 090's fixed end part), render `<WmTabSwitcher>` over the content area, and make the
      tab-panel container `inert` while the switcher is open; **add** an `inert` prop to spec 090's
      `src/components/wm/Header.vue` for its app areas (spec 090 has none; not for the window buttons, not for the tab
      button) and pass it
- [ ] T020 [P] [US1] Add the texts to `de.json` and `en.json`: "New tab", "Show {n} tabs", "– a tab needs attention",
      the overlay titles ("Open tabs", "New tab: choose an app"), the "+" tile label, the "open" label, the close label
      of a tile
- [ ] T021 [US1] Create `scripts/e2e/scenarios/tab-switcher.test.ts` (part 1): a window with one tab shows the "+" and
      no count; press it, the app tiles show; pick `system.passwords` (multi-instance): a second tab opens and shows, the
      button shows 2, the overlay is closed; the "+" in a second window shows `system.settings` marked open and picking it
      shows the existing tab; Escape and a second press of the button close the tiles without a change
- [ ] T022 [US2] Extend `tab-switcher.test.ts` (part 2) (also covers FR-013, SC-002 and SC-005, see the end of this task): with three tabs press the number: the overlay has three tiles
      in tab order, the active one marked; pick another: it shows, the overview is gone, the number is still 3, the
      tab's own location and history are as they were; open the app tiles from the "+" tile and leave them with Escape:
      back in the overview; while the overview is open an app header control (spec 090) and the app content do not
      react, the window buttons and the tab button do; while the tab overview is open run `wm.tab.new` and `wm.tab.close` with
      `runAction` (as an agent would): the tiles follow (FR-013); create 50 tabs with `runAction wm.tab.new`, open the tab
      overview and reach, pick and close the last tile (SC-002; the 300 ms is checked by hand in T032); restart the app
      with the tab overview open and three tabs: all three tabs and the active one come back and the tab overview is
      closed (FR-018, SC-005)

**Checkpoint**: US1 and US2 work; a person can switch tabs by hand again.

---

## Phase 6: User Stories 3, 4 and 5 (Priority: P2, Part B)

### User Story 3 - Closing tabs from the overview

- [ ] T023 [US3] Wire the tile's close button and `Delete` in `TabOverview.vue` to `wm.tab.close` through `useAction`
      (every close guard asks first, declining leaves the tile); with two tabs closing one closes the overlay and the
      button becomes a "+"; the window never closes (FR-012); extend `tab-switcher.test.ts` (part 3): close two of four
      tabs, the number shows 2; close the active tab, another becomes active (neighbour, as `wm.tab.close` decides) and
      the overlay stays; close down to one, the overlay closes, the "+" shows; a tab with a close guard asks first

### User Story 4 - Attention is seen

- [ ] T024 [US4] Verify and extend `tab-switcher.test.ts` (part 4): with two tabs make the inactive one call
      `extension_tab_attention {active: true}` (the `probe` fixture, as in spec 017's attention scenario): the button
      shows the dot and its accessible name ends with the attention text; the overview's tile shows it; activating that
      tab clears both; the active tab asking for attention shows no dot

### User Story 5 - Keyboard, screen reader, phone

- [ ] T025 [US5] Keyboard and assistive support in `TabSwitcher.vue`, `TabOverview.vue` and `AppTiles.vue`: roving
      `tabindex`, `moveFocus` from T013 for the arrow keys (rtl aware), `Enter` picks, `Delete` closes, `Escape`
      closes, `PageUp`/`PageDown` in the app tiles; focus on open and back to the button on close; the shortcuts of
      spec 020 are not intercepted; extend the E2E scenario (part 5): reach the button with Tab, open, move, pick and
      close using the keyboard alone (SC-003)
- [ ] T026 [P] [US5] Compact layout: two columns, 44 × 44 px close buttons and button, no horizontal scroll, the page
      indicator hit targets full size; check in a narrow window in `tab-switcher.test.ts` (part 6) and in
      `scripts/e2e/scenarios/android-phone-layout.test.ts` where the Android scenario can show it
- [ ] T027 [US5] Android back gesture: extend `scripts/e2e/scenarios/android-back-gesture.test.ts` so the gesture closes
      an open overlay (`wm.system.back` → `closeOverlay`) before it goes back in a tab (FR-009)
- [ ] T028 [US5] Move the E2E scenarios that used the old tab strip (the list of T001: `compact-tab-close`,
      `tab-content-isolation`, `chat-multi-instance`, and any `passwords-tabs` step) to the button, the overview and
      the actions, each keeping its assertions: closing a single tab in a compact window now goes through the
      overview's close button

---

## Phase 7: Clean-up (Part B)

- [ ] T029 [P] Update the notes spec 090 added to `specs/015-workspace-shell/spec.md`, `specs/020-tab-navigation/spec.md`
      and `specs/040-active-tab-emphasis/spec.md` (FR-019): the tab strip stays gone; the attention mark returned
      through the tab button and the tiles; the active-tab emphasis is the marked active tile (spec 091)
- [ ] T030 [P] Run `pnpm knip`, `pnpm lint`, `pnpm format:check`, `pnpm typecheck`, `pnpm typecheck:scripts` and all
      `pnpm check:*`; fix what this spec left unused (nothing of other code)
- [ ] T031 [P] Add the terms to `CONTEXT.md` (section "Window Manager", German, in the file's style): **Tab-Button**,
      **Tab-Übersicht**, **App-Kacheln**; no other change
- [ ] T032 Hand check on a phone (quickstart step 8): swipe feel, 44 px targets, the back gesture, long-press versus
      swipe; SC-001 (two presses), SC-002 (50 tabs: the tab overview opens in under 300 ms) and SC-007 (50 apps) by hand once, and
      the cases without an automatic check: the smallest window an app allows (FR-014), a minimized window with the
      tab overview open, a tab whose app is not usable on this device
- [ ] T033 `gh workflow run ci.yml --ref <branch>` for the whole E2E set after Part B; fix or report each failure; note in
      the PR which scenarios were moved
- [ ] T034 Set the spec's status to `Implemented` and list the merged PRs (spec, Part A, Part B) in a short note at the
      end of `spec.md`

---

## Dependencies & Execution Order

```text
Phase 1 ─▶ Phase 2 (T003 test, T004) ─▶ Phase 3 US6 (T005–T011) ─▶ [Part A PR]
                                                  │
spec 090 merged ─▶ Phase 4 (T012 test, T013, T014, T015) ─▶ Phase 5 US1+US2 ─▶ Phase 6 US3, US4, US5 ─▶ Phase 7 ─▶ [Part B PR]
```

- T004 needs T003; T006 needs T005 and T004; T007 needs T006; T008 needs T006; T010 needs T007.
- Phase 5 needs Part A (`AppTiles`, `appPages`) and spec 090. T013 needs T012; T014 needs T013; T018 needs T014, T017 and
  T006; T019 needs T016, T018 and spec 090's `Header.vue`.
- T023 needs T017; T025 needs T018; T028 needs the new scenarios (T021, T022) to exist so a moved test has a
  replacement path.

## Parallel Opportunities

- Phase 2: T003 alone; Phase 3: T009 next to T005–T007.
- Phase 4: T012 next to nothing else (it is the test first); after T013, T014 and T015 are different files.
- Phase 5: T017 (overview) next to T016 (button); T020 (texts) next to T018–T019.
- Phase 6: T026 next to T025; T028 after the scenarios exist.
- Phase 7: T029, T030, T031 at the same time.

## Implementation Strategy

- **MVP = Part A**, then Part B's phases 4–5: after T022 a person can open tabs and switch tabs again. Phase 6 adds
  closing, attention, keyboard and phone polish; it should land in the same PR but can be reviewed per task.
- Part A is useful on its own (the launcher scales) and is the riskiest on touch, so it goes first and is tested on a
  device before Part B builds on it.
- A step that loses a scenario is not merged: moved scenarios keep their assertions (T028).

## Task count

34 tasks: Setup 2, Foundational A 2, US6 7, Foundational B 4, US1 + US2 7, US3 1, US4 1, US5 4, Clean-up 6.
