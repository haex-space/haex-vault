# Research: Switching tabs by hand

R-numbers are cited by `plan.md`, `data-model.md` and the contracts. Spec 090 (the header bar, PR #947) is referred to by
number; this spec builds on it for the button's place and the inert header.

## R1 — Two independent parts, two pull requests

**Decision**: the work splits along what depends on spec 090.

- **Part A, the app tiles and the launcher** (User Story 6, FR-003a, FR-022): extract the launcher's grid into
  `AppTiles.vue` as a scrolling grid. It needs nothing from 090 and can merge first.
- **Part B, the tab button, the overview and the "+" tiles** (User Stories 1–5): needs 090's header, its fixed
  window-button area and its inert header controls. It starts when 090 is merged (or on top of its branch).

**Why**: Part A also fixes a problem the launcher has today with many apps and gives the "+" its tiles. Landing it alone
keeps Part B's PR about tabs only.

## R2 — Where the switcher's state lives, and system back

**Decision**: a new store module `src/stores/wmTabSwitcher.ts` holds one reactive map `windowId → mode`, where mode is
`'tabs'` (the overview) or `'apps'` (the app tiles) plus the place the apps tiles were opened from (`'button'` or
`'overview'`, for FR-011's way back). `Window.vue`, `TabButton.vue` and `TabSwitcher.vue` read and change it through
`open(windowId, mode)`, `toggle`, `close`, `back`.

`wmNavigation.ts` already keeps the window manager's overlays (`overlays.launcher | windows | workspaces`) so that the
Android back gesture (`wm.system.back`, `resolveSystemBack`) closes them. The switcher joins that: `createWmNavigation`
takes two small dependencies, `tabSwitcherOpen(): boolean` and `tabSwitcherBack(): void`; `overlayOpen` in
`systemBack` includes `tabSwitcherOpen()`, and the `closeOverlay` branch handles the topmost overlay only: a launcher
or an overview of windows on top closes first and the switcher stays; only when the switcher is the topmost does it call
`tabSwitcherBack()`, which is `back` of the focused window's switcher: one step, so app tiles opened from the overview lead back to the overview, as Escape does
(FR-009). A launcher or an overview of windows on top still closes first, as today. This is FR-009's "back action
where there is one", and `check-wm-navigation.ts` gets a case for it.

**Why a store module**: `windowManager.ts` is at 501 lines (spec 090 also had to make room there), and the state is
UI-only and never persisted (FR-018). A module of its own follows `wmLayoutHandlers.ts` and `wmTabHeader.ts` (090).

**Not an action**: opening the overview is a purely local UI state, like `onRootPointerDown` in `Window.vue`; it is marked
`action-exempt:` with the reason. Switching, opening and closing tabs go through the existing `wm.tab.*` actions
(spec 020 FR-024), which is the single path agents, shortcuts and tests use.

## R3 — The button lives in the window-button group

**Decision**: `src/components/wm/TabButton.vue` is rendered by `Window.vue` inside the same slot as
`WmWindowControls` (the header's fixed end part in 090), before the minimize button. 090 measures its window-button
slot as `fixed.windowButtons`, so the button is part of the fixed part with no change to `headerLayout.ts` (spec Edge
Cases: the app's controls give way first, FR-001). If 090 measures `WmWindowControls` alone, T019 makes it measure the
slot, and measures it again when the button's width changes ("+", a number, "99+"). The button stops `pointerdown` and
`dblclick` like the window buttons, and keeps the header's 32 px control height.

Face by tab count (`tabButtonFace` in `src/lib/wm/tabSwitcher.ts`, pure): 1 tab → `"+"`, 2–99 → the number, 100 and up →
`"99+"`. The attention mark is a small filled dot **and** the accessible name ends in "– a tab needs attention"; the
dot is shown when any tab other than the active one has `TabRuntime.attention` (read with `wm.runtimeFor`, no store
change). Nothing in the host clears the mark when a tab is shown: the app clears its own (`setTabAttention(false)`; the
chat when its prompt is answered, an extension with `extension_tab_attention {active: false}`), and this spec leaves
that as it is. On a compact window the button stays (the window buttons there are only "close", so there is room).

## R4 — The overlay: placement, inert, focus

**Decision**: `src/components/wm/TabSwitcher.vue` is a sibling of the tab-panel container in `Window.vue`, absolutely
positioned over it (`inset-0`), mounted only while open (`v-if`). While it is open:

- the tab-panel container gets `inert` (FR-010: no pointer or keyboard input reaches the app or an extension frame;
  `ExtensionFrame.vue` already uses `inert` for its own dialogs), and the app's header controls are inert too (090's
  `Header.vue` takes an `inert` prop for its app areas and its overflow menu button, not for the window buttons or the tab button);
- focus moves to the active tab's tile (overview) or to the first tile (apps), and returns to the
  tab button when it closes (FR-009, FR-015);
- the window's `@pointerdown` focus and the title-bar drag are unchanged (the header is outside the overlay).

The overlay uses the page's own colours (`bg-background`) so it reads as part of the window, not as a dialog.

## R5 — The shared grid: `AppTiles.vue`

**Decision**: extract the launcher's `v-for` of app buttons into `src/components/wm/AppTiles.vue` (the candidate the
graph returns; the launcher is the **extended** candidate, not duplicated). The grid **scrolls vertically** (the
operator's decision, spec Clarifications): COSMIC's app library does the same, and a scrolling grid needs no new gesture,
no library and no page state.

Props: `apps` (what `wm.apps()` returns), `markOpen` (label an app that may be open only once and already is),
`pinMenu` (the launcher's right-click/long-press pin menu). Emits: `pick(appId)`. The launcher's assistant and lock
tiles stay in `Launcher.vue` in a fixed strip below the scrolling grid (FR-022).

**Look** (spec FR-003, after COSMIC's app library): the icon large (a 56 px box for `iconUrl` images and Iconify icons),
the name below it, centred, up to two lines (`line-clamp-2`), no frame, a soft hover background, the attention dot at the
icon's corner, the "open" label (with `markOpen`) or the unavailable reason as a small third line. Kept from today's
tile: `data-app-id`, the `open-chat` test id, `launcher-menu-pin`, the disabled tile with its reason. The tile buttons are
the same elements, so the existing E2E helpers (`openChat`, `openLauncher`) keep working; `openLauncher` waits for
`lock-instance`, which is visible at every scroll position because it is outside the grid.

**Columns**: `src/lib/wm/tileGrid.ts` (pure): `gridColumns(width, {compact})` is
`clamp(floor((width + GAP) / (MIN_TILE_WIDTH + GAP)), 1, 7)` of the grid's width alone (`GAP` 12, `MIN_TILE_WIDTH` 112) and at most 2 when compact, so a wide window shows as many columns as COSMIC's seven and a 360 px window (the
smallest an app allows) never gets tiles below `MIN_TILE_WIDTH`. The CSS grid uses the same number (`grid-template-columns: repeat(n, minmax(0, 1fr))`
from a `ResizeObserver` on the container: the window's width, not the screen's, as for 090's header); the keyboard needs
it to move by a row. Rows are not computed: the grid scrolls.

**Scroll**: the grid is the scroll container (`overflow-y: auto`, `overscroll-behavior: contain`); the overlay and the
launcher's drawer give it a bounded height (the overlay: the whole content area; the drawer: its content region above
the fixed strip). It always opens at the top. A tile focused by the keyboard is scrolled into view
(`scrollIntoView({block: 'nearest'})`). No scroll bar when everything fits. Smooth scrolling is not used
(`scroll-behavior: auto`), so reduced motion needs no special case.

## R6 — No new gesture next to the pin menu

**Decision**: the grid scrolls natively, so a touch that moves scrolls and never produces a click, and a long press
without movement still opens the launcher's pin menu as today. A first draft had pages with a swipe (like the password
app); the operator chose scrolling after seeing COSMIC's app library, which removes the one place where two gesture
systems would have shared an element. The emulator check stays as a plain regression check in the launcher task (a long
press opens the menu, a scroll that starts on a tile does not), not as a risk.

## R7 — The overview: `TabOverview.vue`

**Decision**: tiles from `window.tabs` in order, each with `wm.tabDisplayInfo(tab)` (icon, title, attention) and the
tab's place (the current location of `wm.historyOf(tabId)`, shown as a path). A close button per tile
calls `wm.tab.close` (guards ask first, FR-012; a failed action shows `toast.error`, a declined close nothing); the "+" tile calls `open(windowId, 'apps')` with the place
`'overview'`.

Grid: CSS grid with the same `tileGrid` columns (so the arrow keys know the column count); vertical scroll inside the
overlay. Keyboard (pure `moveFocus(index, key, columns, count, rtl)` in `tileGrid.ts`, shared with the app tiles): arrows move by one or by a
row (left and right swap in a right-to-left layout), Home and End jump, Enter picks, Delete closes the tab, Escape closes.
The "+" tile is the last grid item. Roving `tabindex`.

**Live** (FR-013): the tiles are a `computed` of the window's tabs, so a tab opened, closed or renamed elsewhere
changes them. When the count falls to one the store module closes the switcher (a `watch` in `TabSwitcher.vue`).

**Active tile marked** (FR-006): a thicker ring **and** a check icon, not colour alone (spec 040's rule).

**Reduced motion**: no open animation under `prefers-reduced-motion`.

## R8 — Candidates evaluated (graphify-first)

| Candidate                                        | Verdict                                                                                                                                                       |
| ------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `wm/Launcher.vue` (the tile grid)                | **Match**: extracted into `AppTiles.vue`, the launcher is rewritten to use it (R5)                                                                            |
| COSMIC's app library (the operator's screenshot) | Visual and behavioural reference: large icons, name below, a scrolling grid; its group bar is a later spec                                                    |
| `wm/WindowOverview.vue`                          | Same idea (rows of windows, close button, compact touch target) for **windows**; different data and layout, so a new tile component, copying its compact rule |
| `wm/NewTabMenu.vue`                              | Removed by spec 090; its marking of an app that is already open (`isOpenElsewhere`) moves into `AppTiles.vue`'s `markOpen`                                    |
| `UiDrawerModal` (haex-ui)                        | Stays the launcher's container; the overlay for the tab switcher is not a drawer                                                                              |

## R9 — Tests

| Layer              | Where                                                                                               | What                                                                                                                 |
| ------------------ | --------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------- |
| Pure logic         | `scripts/check-wm-tab-switcher.ts` (in `check:wm-navigation`)                                       | `tabButtonFace` (1, 2, 99, 100), mode transitions (open, toggle, back to overview, count to one), attention rule     |
| Pure logic         | `scripts/check-wm-tile-grid.ts` (in `check:wm-state`)                                               | `gridColumns` (at least 1, compact at most 2, wide capped at 7, never below `MIN_TILE_WIDTH`), `moveFocus` incl. rtl |
| Store              | `scripts/check-wm-tab-switcher-store.ts` (in `check:wm-state`)                                      | the prune of a closed window's entry (`check-wm-state.ts` is over 500 lines, so not there)                           |
| System back        | `scripts/check-wm-navigation.ts` (existing)                                                         | `closeOverlay` when a switcher is open                                                                               |
| E2E                | new `tab-switcher.test.ts`, `launcher-grid.test.ts`                                                 | SC-001, SC-003, SC-004, SC-005, SC-006, SC-007                                                                       |
| E2E, moved         | `compact-tab-close`, `chat-multi-instance` (the only users of `tab-close` and `wm-new-tab` on main) | scenarios that used the tab strip now use the button and the overview                                                |
| E2E, extended      | `android-back-gesture`                                                                              | the gesture closes an open overlay first (T027)                                                                      |
| By hand (operator) | quickstart                                                                                          | scroll feel on a phone, long press on a tile (R6), 50 apps                                                           |

E2E does not run on pull requests: before merge `gh workflow run ci.yml --ref <branch>` (feedback). Scenarios that
need 50 apps build them as installed extensions from the existing fixture bundle (the `probe`), or by a test-only
app-list override in the store if installing 50 is too slow; the task decides on a measurement.

## R10 — Build and process constraints

- Frontend only, no Rust: no cargo scope issue. `pnpm typecheck:scripts` before every push; `pnpm check:*` all after a
  change to `src/lib/wm/` or a composable (Node sandbox rules: no DOM globals, no `instanceof Element`).
- No new dependency. `pnpm knip` after the extraction.
- Every file at most 500 lines; `Launcher.vue` shrinks, `AppTiles.vue` and `TabOverview.vue` stay small.
- Texts in `de.json` and `en.json` in lockstep: the button's names ("New tab", "Show N tabs", "– a tab needs
  attention"), the overlay titles, the "+" tile, the close label, the empty state
  is not needed (a window always has a tab).
- Branches `--no-track` from `origin/main`; Conventional Commits, no agent trailer.
