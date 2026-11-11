# Contract: the tab switcher overlay

Implements FR-006–FR-016. Components: `TabSwitcher.vue` (the overlay and its modes), `TabOverview.vue` (the tab
tiles), `AppTiles.vue` (the app tiles, [app-tiles.md](./app-tiles.md)); state in `wmTabSwitcher.ts`
([data-model.md](../data-model.md) §SwitcherState).

## Structure

```text
Window.vue
├── <WmHeader> … <WmTabButton/> <WmWindowControls/>      (stays visible and operable)
└── content area
    ├── tab panels            (inert while the overlay is open)
    └── <WmTabSwitcher v-if="state">   absolute inset-0, bg-background
        ├── mode "tabs": <WmTabOverview>   (tiles + trailing "+" tile)
        └── mode "apps": <WmAppTiles>      (scrolling grid; picks call wm.tab.new)
```

## Tab tile (FR-007)

Icon (the app's), title, place (current location's path, one line, truncated), attention dot, close button, and for the
active tab a ring and a check icon. No picture of the content. `data-testid`: `tab-tile-<tabId>`, `tab-tile-close-<tabId>`,
`tab-tile-new`, overlay `tab-switcher`, mode `data-mode="tabs|apps"`.

## Pick and close

| Action                    | Call                                                  | After                                                      |
| ------------------------- | ----------------------------------------------------- | ---------------------------------------------------------- |
| pick a tab tile           | `wm.tab.activate {tabId}`                             | close; if it is the active tile, close only                 |
| pick an app tile          | `wm.tab.new {windowId, appId}`                        | close; the new tab (or the open single-instance one) shows |
| close button / Delete     | `wm.tab.close {tabId}`                                | guards ask first; declining leaves the tile; count 1 closes the overlay; the window never closes |
| pick the "+" tile         | `open(windowId, 'apps')` with `appsFrom: 'overview'`  | app tiles                                                  |

A single-instance app that is open in **another window** is shown by `wm.tab.new` as today (that window comes forward);
the overlay closes.

## Keyboard (FR-015, FR-016)

On open, focus goes to the active tab's tile (overview) or the first tile (apps). Roving `tabindex`.
`moveFocus` rules: arrows by one or by a row, `Home`/`End`, no wrap, left/right swapped in a right-to-left layout;
`Enter` picks; `Delete` closes the tab (overview only); `Escape` closes (or goes back to the overview, data-model.md);
in the app tiles the focused tile is scrolled into view ([app-tiles.md](./app-tiles.md)). The global shortcuts of spec 020
are not intercepted. On close, focus returns to the tab button.

## Inertness (FR-010)

While open: the tab-panel container is `inert`; the header's app areas are `inert` (090's `Header.vue` prop); the tab
button and the window buttons are not. Pointer events on the overlay do not reach the window's drag handler (the header
is the only drag region).

## Live updates and ends (FR-009, FR-013)

The tiles are derived from `window.tabs`, so they follow actions, agents and the app itself. The overlay closes when
the count falls to one, when the window closes, on Escape, on the platform's back action
(`wm.system.back` → `closeOverlay`, research R2), and when the button is pressed again.

## Compact (FR-017)

`wm.compact`: two columns, close buttons 44 × 44 px, no horizontal scroll, the overlay scrolls vertically.

## Not saved

The overlay is never part of the session (`wmSession`); after a restore it is closed.
