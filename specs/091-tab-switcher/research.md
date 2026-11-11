# Research: Switching tabs by hand

R-numbers are cited by `plan.md`, `data-model.md` and the contracts. Spec 090 (the header bar, PR #947) is referred to by
number; this spec builds on it for the button's place and the inert header.

## R1 — Two independent parts, two pull requests

**Decision**: the work splits along what depends on spec 090.

- **Part A, the app tiles and the launcher** (User Story 6, FR-003a, FR-022): extract the launcher's grid into
  `AppTiles.vue` with pages and swipe. It needs nothing from 090 and can merge first.
- **Part B, the tab button, the overview and the "+" tiles** (User Stories 1–5): needs 090's header, its fixed
  window-button area and its inert header controls. It starts when 090 is merged (or on top of its branch).

**Why**: Part A also fixes a problem the launcher has today with many apps, and it carries the riskiest piece (swipe
versus click versus long-press). Landing it alone keeps Part B's PR about tabs only.

## R2 — Where the switcher's state lives, and system back

**Decision**: a new store module `src/stores/wmTabSwitcher.ts` holds one reactive map `windowId → mode`, where mode is
`'tabs'` (the overview) or `'apps'` (the app tiles) plus the place the apps tiles were opened from (`'button'` or
`'overview'`, for FR-011's way back). `Window.vue`, `TabButton.vue` and `TabSwitcher.vue` read and change it through
`open(windowId, mode)`, `toggle`, `close`, `back`.

`wmNavigation.ts` already keeps the window manager's overlays (`overlays.launcher | windows | workspaces`) so that the
Android back gesture (`wm.system.back`, `resolveSystemBack`) closes them. The switcher joins that: `createWmNavigation`
takes two small dependencies, `tabSwitcherOpen(): boolean` and `closeTabSwitchers(): void`; `overlayOpen` in
`systemBack` includes `tabSwitcherOpen()`, and the `closeOverlay` branch calls `closeTabSwitchers()`. This is FR-009's
"back action where there is one", and `check-wm-navigation.ts` gets a case for it.

**Why a store module**: `windowManager.ts` is at 501 lines (spec 090 also had to make room there), and the state is
UI-only and never persisted (FR-018). A module of its own follows `wmLayoutHandlers.ts` and `wmTabHeader.ts` (090).

**Not an action**: opening the overview is a purely local UI state, like `onRootPointerDown` in `Window.vue`; it is marked
`action-exempt:` with the reason. Switching, opening and closing tabs go through the existing `wm.tab.*` actions
(spec 020 FR-024), which is the single path agents, shortcuts and tests use.

## R3 — The button lives in the window-button group

**Decision**: `src/components/wm/TabButton.vue` is rendered by `Window.vue` inside the same slot as
`WmWindowControls` (the header's fixed end part in 090), before the minimize button. The header's layout function
measures that slot as `fixed.windowButtons`, so the button is part of the fixed part with no change to
`headerLayout.ts` (spec Edge Cases: the app's controls give way first, FR-001).

Face by tab count (`tabButtonFace` in `src/lib/wm/tabSwitcher.ts`, pure): 1 tab → `"+"`, 2–99 → the number, 100 and up →
`"99+"`. The attention mark is a small filled dot **and** the accessible name ends in "– a tab needs attention"; the
dot is shown when any tab other than the active one has `TabRuntime.attention` (read with `wm.runtimeFor`, no store
change). On a compact window the button stays (the window buttons there are only "close", so there is room).

## R4 — The overlay: placement, inert, focus

**Decision**: `src/components/wm/TabSwitcher.vue` is a sibling of the tab-panel container in `Window.vue`, absolutely
positioned over it (`inset-0`), mounted only while open (`v-if`). While it is open:

- the tab-panel container gets `inert` (FR-010: no pointer or keyboard input reaches the app or an extension frame;
  `ExtensionFrame.vue` already uses `inert` for its own dialogs), and the app's header controls are inert too (090's
  `Header.vue` takes an `inert` prop for its app areas, not for the window buttons or the tab button);
- focus moves to the active tab's tile (overview) or to the first tile of the shown page (apps), and returns to the
  tab button when it closes (FR-009, FR-015);
- the window's `@pointerdown` focus and the title-bar drag are unchanged (the header is outside the overlay).

The overlay uses the page's own colours (`bg-background`) so it reads as part of the window, not as a dialog.

## R5 — The shared grid: `AppTiles.vue`

**Decision**: extract the launcher's `v-for` of app buttons into `src/components/wm/AppTiles.vue` (the candidate the
graph returns; the launcher is the **extended** candidate, not duplicated).

Props: `apps` (what `wm.apps()` returns), `rows` (a number, or `'fit'` to measure the container's height), `markOpen`
(label an app that may be open only once and already is), `pinMenu` (the launcher's right-click/long-press pin menu).
Emits: `pick(appId)`. Slots: none; the launcher's assistant and lock tiles stay in `Launcher.vue` in a fixed strip
below the pages (FR-022).

Kept from today's tile: `data-app-id`, the `open-chat` test id, `launcher-menu-pin`, the attention dot, the
disabled tile with its reason, the icon as `iconUrl` or Iconify name. The launcher's tile buttons are the same
elements, so the existing E2E helpers (`openChat`, `openLauncher`) keep working; `openLauncher` waits for
`lock-instance`, which stays visible on every page because it is outside them.

**Page size**: `src/lib/wm/appPages.ts` (pure): `tileGrid(width, height, {compact, rows})` → `{columns, rows, perPage}`:
columns = 2 when compact, else `clamp(floor((width + gap) / (minTile + gap)), 3, 6)`; rows = the given number, or
`max(1, floor((height + gap) / (tileHeight + gap)))` for `'fit'`. A `ResizeObserver` on the container feeds it (the
window's size, not the screen's, as for 090's header). `pagesOf(count, perPage)` and `clampPage(page, pages)` keep the
current page valid when apps are installed or removed (spec Edge Cases). Launcher: `rows` is 3 (4 compact) because the
drawer has no fixed height to fit; the tab overlay uses `'fit'`.

**Pages**: `swiper` (already a dependency; `passwords/EntryTabsSwiper.vue` is the pattern), loaded with
`defineAsyncComponent` so the launcher and the "+" do not pay for it until a second page exists. One slide per page.
When everything fits on one page no Swiper is mounted at all (no indicator, no swipe: spec Edge Cases). Slides that are
not shown get `inert` and `aria-hidden` so the keyboard and screen readers never land on a hidden tile.

**Not reused from `EntryTabsSwiper.vue`**: its `keepWheel`/`NO_SWIPE` guard exists for form controls inside slides.
App tiles have no inputs; the guard is not copied (a short `ponytail:` note says so).

## R6 — Swipe versus click versus long-press

**Decision**: Swiper's `threshold` (8 px, as the password app) separates a swipe from a press, with `preventClicks` on,
so a swipe never picks a tile (spec: Edge Cases). The launcher's tiles also have a context menu (right click, long
press). On a touch screen a long press that moves into a swipe must not open the menu: the plan keeps Reka's
long-press behaviour and relies on the movement cancel of the touch handling; the emulator task verifies it on the Android
emulator and, if the menu still opens, disables the long-press menu while a drag is in progress.

**Why flagged**: this is the one place where two gesture systems (Swiper's and the context menu's) share an element.
Everything else in the spec is ordinary layout.

## R7 — The overview: `TabOverview.vue`

**Decision**: tiles from `window.tabs` in order, each with `wm.tabDisplayInfo(tab)` (icon, title, attention) and the
tab's place (the current location of `wm.historyOf(tabId)`, shown as a path). A close button per tile
calls `wm.tab.close` (guards ask first, FR-012); the "+" tile calls `open(windowId, 'apps')` with the place
`'overview'`.

Grid: CSS grid with the same `tileGrid` columns (so the arrow keys know the column count); vertical scroll inside the
overlay. Keyboard (pure `moveFocus(index, key, columns, count, rtl)` in `tabSwitcher.ts`): arrows move by one or by a
row (left and right swap in a right-to-left layout), Home and End jump, Enter picks, Delete closes the tab, Escape closes.
The "+" tile is the last grid item. Roving `tabindex`.

**Live** (FR-013): the tiles are a `computed` of the window's tabs, so a tab opened, closed or renamed elsewhere
changes them. When the count falls to one the store module closes the switcher (a `watch` in `TabSwitcher.vue`).

**Active tile marked** (FR-006): a thicker ring **and** a check icon, not colour alone (spec 040's rule).

**Reduced motion**: no open or page animation under `prefers-reduced-motion`.

## R8 — Candidates evaluated (graphify-first)

| Candidate                                   | Verdict                                                                                                          |
| ------------------------------------------- | ---------------------------------------------------------------------------------------------------------------- |
| `wm/Launcher.vue` (the tile grid)           | **Match**: extracted into `AppTiles.vue`, the launcher is rewritten to use it (R5)                                |
| `passwords/EntryTabsSwiper.vue`             | Pattern only (async Swiper, reduced motion, threshold); different slide content, copied options not components   |
| `wm/WindowOverview.vue`                     | Same idea (rows of windows, close button, compact touch target) for **windows**; different data and layout, so a new tile component, copying its compact rule |
| `wm/NewTabMenu.vue`                         | Removed by spec 090; its marking of an app that is already open (`isOpenElsewhere`) moves into `AppTiles.vue`'s `markOpen` |
| `UiDrawerModal` (haex-ui)                   | Stays the launcher's container; the overlay for the tab switcher is not a drawer                                   |

## R9 — Tests

| Layer                | Where                                                                  | What                                                                                              |
| -------------------- | ---------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------- |
| Pure logic           | `scripts/check-wm-tab-switcher.ts` (in `check:wm-navigation`)          | `tabButtonFace` (1, 2, 99, 100), mode transitions (open, toggle, back to overview, count to one), `moveFocus` incl. rtl, attention rule |
| Pure logic           | `scripts/check-wm-app-pages.ts` (in `check:wm-state`)                  | `tileGrid`, `pagesOf`, `clampPage` (50 apps, apps removed, compact, one page, zero apps)           |
| System back          | `scripts/check-wm-navigation.ts` (existing)                            | `closeOverlay` when a switcher is open                                                            |
| E2E                  | new `tab-switcher.test.ts`, `launcher-pages.test.ts`                   | SC-001, SC-003, SC-004, SC-005, SC-006, SC-007                                                     |
| E2E, moved           | `compact-tab-close`, `passwords-tabs`, `tab-content-isolation`, `android-phone-layout`, `android-back-gesture` | scenarios that used the tab strip now use the button and the overview                      |
| By hand (operator)   | quickstart                                                             | swipe feel on a phone, long-press versus swipe (R6), 50 apps                                      |

E2E does not run on pull requests: before merge `gh workflow run ci.yml --ref <branch>` (feedback). Scenarios that
need 50 apps build them as installed extensions from the existing fixture bundle (the `probe`), or by a test-only
app-list override in the store if installing 50 is too slow; the task decides on a measurement.

## R10 — Build and process constraints

- Frontend only, no Rust: no cargo scope issue. `pnpm typecheck:scripts` before every push; `pnpm check:*` all after a
  change to `src/lib/wm/` or a composable (Node sandbox rules: no DOM globals, no `instanceof Element`).
- No new dependency (`swiper` is present, loaded on demand). `pnpm knip` after the extraction.
- Every file at most 500 lines; `Launcher.vue` shrinks, `AppTiles.vue` and `TabOverview.vue` stay small.
- Texts in `de.json` and `en.json` in lockstep: the button's names ("New tab", "Show N tabs", "– a tab needs
  attention"), the overlay titles, the "+" tile, the close label, the page indicator ("Page X of Y"), the empty state
  is not needed (a window always has a tab).
- Branches `--no-track` from `origin/main`; Conventional Commits, no agent trailer.
