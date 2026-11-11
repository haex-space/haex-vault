# Contract: the app tiles (shared by the "+" and the launcher)

Implements FR-003, FR-003a, FR-022, SC-006, SC-007. Component: `src/components/wm/AppTiles.vue`; layout in
`src/lib/wm/appPages.ts`. Research R5, R6.

## Props and events

| Prop        | Type                 | Meaning                                                                                  |
| ----------- | -------------------- | ---------------------------------------------------------------------------------------- |
| `apps`      | `AppDefinition[]`    | what `wm.apps()` returns (system apps and installed extensions), in the launcher's order  |
| `rows`      | `number` \| `'fit'`  | fixed rows per page (launcher: 3, compact 4) or measure the container's height (the "+") |
| `markOpen`  | `boolean`            | label a single-instance app that already has a tab (the "+"); the launcher passes `false` |
| `pinMenu`   | `boolean`            | right click or long press opens the dock pin menu (the launcher)                          |

| Event  | Payload  | Meaning                                |
| ------ | -------- | -------------------------------------- |
| `pick` | `appId`  | a tile was pressed (never after a swipe) |

## Tile

As the launcher's tile today: the app's icon (`iconUrl` or Iconify), the name (truncated), an attention dot
(`wm.appHasAttention`), a disabled tile with the reason `unavailableKey`; plus, with `markOpen`, a small "open" label for an
app that already has a tab. Kept test ids: `data-app-id`, `open-chat` on the chat tile, `launcher-menu-pin`.

## Pages

- `tileGrid(width, height, {compact, rows})` gives `columns`, `rows`, `perPage` (data-model.md); a `ResizeObserver` on
  the container recomputes it, so a window made smaller or larger re-pages live.
- One slide per page in an async-loaded Swiper (R5). **One page** (everything fits, or zero apps): no Swiper, no
  indicator, no swipe, plain grid.
- The current page stays valid when the page count changes (`clampPage`); the component always opens on page 0.
- Slides that are not shown are `inert` and `aria-hidden`.

## Changing the page (FR-003a)

| Input                                   | Behaviour                                                             |
| --------------------------------------- | --------------------------------------------------------------------- |
| swipe (finger, pen, mouse drag)         | Swiper, threshold 8 px; a swipe never picks a tile or opens a menu    |
| horizontal touchpad gesture, wheel      | Swiper's mousewheel module with `forceToAxis`, as the password app    |
| `ArrowLeft`/`ArrowRight` at a page edge | focus moves to the neighbouring page's first/last tile in that row    |
| `PageUp` / `PageDown`                   | previous / next page, focus to its first tile                         |
| the page indicator                      | a row of buttons "Page X of Y" (accessible names), the current one `aria-current` |

Under `prefers-reduced-motion` the speed is 0. A tile is always a full touch target (at least `MIN_TILE_WIDTH` ×
`TILE_HEIGHT`, never shrunk below it to squeeze more per page).

## The launcher (FR-022)

`Launcher.vue` renders `<AppTiles :apps :rows="compact ? 4 : 3" pin-menu @pick="launch">` inside its drawer and, **below
the pages in a fixed strip**, the voice-assistant tile and the lock tile (`launcher-assistant-toggle`, `lock-instance`).
`launch` opens a window (`wm.app.open`) as today.

## The "+" (FR-003)

`TabSwitcher.vue` (mode `apps`) renders `<AppTiles :apps :rows="'fit'" mark-open @pick="pickApp">`; `pickApp` calls
`wm.tab.new {windowId, appId}` and closes. The assistant and lock tiles do not appear.
