# Data Model: Switching tabs by hand

Nothing here is persisted (FR-018). The tab model (`WmWindow.tabs`, `activeTabId`, `TabRuntime.attention`) is spec 015's
and is only read.

## SwitcherState (per window, `wmTabSwitcher.ts`)

| Field      | Type                       | Notes                                                               |
| ---------- | -------------------------- | ------------------------------------------------------------------- |
| `mode`     | `'tabs'` \| `'apps'`       | the overview or the app tiles; absent in the map = closed           |
| `appsFrom` | `'button'` \| `'overview'` | where the app tiles were opened from; decides the way back (FR-011) |

Held as `Map<windowId, SwitcherState>`; an entry is removed when the window closes (`syncTabRuntime`-style prune) or the
switcher closes.

### Transitions

```text
closed ── press "+" (1 tab) ───────────────▶ apps(from: button)
closed ── press number (≥2 tabs) ──────────▶ tabs
tabs   ── press number / Esc / back ────────▶ closed
tabs   ── pick a tile ─────────────────────▶ closed   (wm.tab.activate)
tabs   ── pick the active tile ────────────▶ closed   (nothing else)
tabs   ── pick "+" tile ───────────────────▶ apps(from: overview)
tabs   ── close a tile, count falls to 1 ──▶ closed   (button becomes "+")
tabs   ── close a tile, count stays ≥2 ────▶ tabs     (tiles follow)
apps   ── pick an app ─────────────────────▶ closed   (wm.tab.new; new tab active)
apps   ── press "+"/number / Esc / back ───▶ closed if from: button, tabs if from: overview
any    ── tab count falls to 1 (mode tabs, or apps from overview) ▶ closed
any    ── window closes ───────────────────▶ entry removed
```

The press of the button while `apps` is open from the overview goes back to the overview (one step), not to closed.
System back (`wm.system.back`) calls `back` on the switcher of the focused window: the same one step as Escape.

## TabButtonFace

Derived from the tab count `n` (`tabButtonFace(n)`):

| `n`    | `kind`  | `label` (what is drawn) | accessible name |
| ------ | ------- | ----------------------- | --------------- |
| 1      | `plus`  | `+`                     | "New tab"       |
| 2–99   | `count` | `n`                     | "Show n tabs"   |
| 100 up | `count` | `99+`                   | "Show 99+ tabs" |

Plus `attention: boolean` (any inactive tab has `TabRuntime.attention`), drawn as a dot, with " – a tab needs attention"
appended to the name; and `expanded: boolean` (`aria-expanded`) when the switcher is open.

## TileGrid (pure, `tileGrid.ts`)

`gridColumns(width, {compact})` → number of columns from the width of the grid alone: `clamp(floor((width + GAP) /
(MIN_TILE_WIDTH + GAP)), 1, 7)`, and at most 2 when compact. Constants: `GAP = 12`, `MIN_TILE_WIDTH = 112`. A tile is
never narrower than `MIN_TILE_WIDTH` (a 360 px window gets two columns, not three cramped ones). The same number drives
the CSS grid and the keyboard's row steps. The grid scrolls, so no rows and no pages are computed.

## AppTile (view model)

What `AppTiles.vue` draws per app from `AppDefinition` (spec 015, `lib/wm/apps.ts`): `id`, `title` (`title` or the
translated `titleKey`), icon (`iconUrl` or Iconify `icon`), `unavailableKey` (disabled with the reason), attention
(`wm.appHasAttention(id)`), and for `markOpen` whether a single-instance app already has a tab somewhere
(`isOpenElsewhere`, moved from `NewTabMenu.vue`).

## TabTile (view model)

Per tab of the window, in tab order: `tabId`, `title` (`wm.tabDisplayInfo`), icon, `place` (current location path),
`active` (`tabId === window.activeTabId`), `attention` (`TabRuntime.attention`), plus one trailing "+" tile.

## Focus (pure, `moveFocus` in `tileGrid.ts`)

`moveFocus(index, key, columns, count, rtl)` → the next focused index for `ArrowLeft/Right/Up/Down`, `Home`, `End`
(clamped, no wrap), left and right swapped when `rtl`. `count` is the number of focusable tiles, so a caller that has a
trailing "+" tile includes it and the last index is `count - 1`; `null` for any other key and for an empty grid. The move
is the file browser's `moveIndex` (spec 044), wrapped for keys and layout direction.
