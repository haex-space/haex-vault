# Contract: the app tiles (shared by the "+" and the launcher)

Implements FR-003, FR-003a, FR-022, SC-006, SC-007. Component: `src/components/wm/AppTiles.vue`; columns in
`src/lib/wm/tileGrid.ts`. Research R5, R6.

## Props and events

| Prop        | Type                 | Meaning                                                                                  |
| ----------- | -------------------- | ---------------------------------------------------------------------------------------- |
| `apps`      | `AppDefinition[]`    | what `wm.apps()` returns (system apps and installed extensions), in the launcher's order  |
| `markOpen`  | `boolean`            | label a single-instance app that already has a tab (the "+"); the launcher passes `false` |
| `pinMenu`   | `boolean`            | right click or long press opens the dock pin menu (the launcher)                          |

| Event  | Payload  | Meaning                                  |
| ------ | -------- | ---------------------------------------- |
| `pick` | `appId`  | a tile was pressed (never after a scroll) |

## Tile (the look of COSMIC's app library)

- The app's icon large (a 56 px box; `iconUrl` image or Iconify icon), the name centred below it in up to two lines
  (`line-clamp-2`), no frame, a soft hover background; a full touch target (at least `MIN_TILE_WIDTH` 112 px wide).
- An attention dot at the icon's corner (`wm.appHasAttention`); a disabled tile with the reason `unavailableKey` as a
  small third line; with `markOpen`, a small "open" label for an app that already has a tab.
- Kept test ids: `data-app-id`, `open-chat` on the chat tile, `launcher-menu-pin`.

## Scroll (FR-003a)

- The grid is the scroll container (`overflow-y: auto`, `overscroll-behavior: contain`) with a bounded height from its
  host (the overlay: the content area; the launcher: the drawer region above the fixed strip). The header and the tab
  button never scroll away. It opens at the top; no scroll bar when everything fits.
- Columns: `gridColumns(width, {compact})` (2 compact, else 3 to 7 by width, `GAP` 12, `MIN_TILE_WIDTH` 112); a
  `ResizeObserver` recomputes it, so a window made smaller or larger re-flows live. Tiles are never shrunk to fit.
- Keyboard: roving `tabindex`, arrows move by one or by a row (`moveFocus` of `tabSwitcher.ts`, right-to-left aware),
  `Home`/`End`, `Enter` picks, and the focused tile is scrolled into view (`block: 'nearest'`).
- A touch or wheel gesture that moves scrolls and never picks a tile; a press without movement picks it. Smooth
  scrolling is not used, so `prefers-reduced-motion` needs no special case.

## The launcher (FR-022)

`Launcher.vue` renders `<AppTiles :apps pin-menu @pick="launch">` inside its drawer and, **below the grid in a fixed
strip**, the voice-assistant tile and the lock tile (`launcher-assistant-toggle`, `lock-instance`). `launch` opens a
window (`wm.app.open`) as today.

## The "+" (FR-003)

`TabSwitcher.vue` (mode `apps`) renders `<AppTiles :apps mark-open @pick="pickApp">`; `pickApp` calls
`wm.tab.new {windowId, appId}` and closes. The assistant and lock tiles do not appear.

## Not here (later specs)

Groups as in COSMIC's app library (a bar below the grid: Library, Office, System, "+ Group", edit, delete) and a filter
field. The grid's container leaves the strip below it free so a group bar can be added without a rewrite.
