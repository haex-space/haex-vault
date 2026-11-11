# Data Model: Switching tabs by hand

Nothing here is persisted (FR-018). The tab model (`WmWindow.tabs`, `activeTabId`, `TabRuntime.attention`) is spec 015's
and is only read.

## SwitcherState (per window, `wmTabSwitcher.ts`)

| Field        | Type                                  | Notes                                                                     |
| ------------ | ------------------------------------- | ------------------------------------------------------------------------- |
| `mode`       | `'tabs'` \| `'apps'`                  | the overview or the app tiles; absent in the map = closed                 |
| `appsFrom`   | `'button'` \| `'overview'`            | where the app tiles were opened from; decides the way back (FR-011)       |

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
any    ── tab count changes to 1 while tabs ▶ closed
any    ── window closes ───────────────────▶ entry removed
```

The press of the button while `apps` is open from the overview goes back to the overview (one step), not to closed.

## TabButtonFace

Derived from the tab count `n` (`tabButtonFace(n)`):

| `n`      | `kind`   | `label` (what is drawn) | accessible name                               |
| -------- | -------- | ------------------------ | --------------------------------------------- |
| 1        | `plus`   | `+`                      | "New tab"                                     |
| 2–99     | `count`  | `n`                      | "Show n tabs"                                 |
| 100 up   | `count`  | `99+`                    | "Show 99+ tabs"                               |

Plus `attention: boolean` (any inactive tab has `TabRuntime.attention`), drawn as a dot, with " – a tab needs attention"
appended to the name; and `expanded: boolean` (`aria-expanded`) when the switcher is open.

## TileGrid (pure, `appPages.ts`)

`tileGrid(width, height, {compact, rows})` → `{columns, rows, perPage}`; `pagesOf(count, perPage)` → number of pages
(0 apps → 1 empty page); `clampPage(page, pages)` → a valid index. Constants: `GAP = 12`, `MIN_TILE_WIDTH = 112`,
`TILE_HEIGHT = 104`; columns 2 when compact, else between 3 and 6.

## AppTile (view model)

What `AppTiles.vue` draws per app from `AppDefinition` (spec 015, `lib/wm/apps.ts`): `id`, `title` (`title` or the
translated `titleKey`), icon (`iconUrl` or Iconify `icon`), `unavailableKey` (disabled with the reason), attention
(`wm.appHasAttention(id)`), and for `markOpen` whether a single-instance app already has a tab somewhere
(`isOpenElsewhere`, moved from `NewTabMenu.vue`).

## TabTile (view model)

Per tab of the window, in tab order: `tabId`, `title` (`wm.tabDisplayInfo`), icon, `place` (current location path),
`active` (`tabId === window.activeTabId`), `attention` (`TabRuntime.attention`), plus one trailing "+" tile.

## Focus (pure, `moveFocus`)

`moveFocus(index, key, columns, count, rtl)` → the next focused index for `ArrowLeft/Right/Up/Down`, `Home`, `End`
(clamped, no wrap), left and right swapped when `rtl`. The "+" tile has index `count` (the last).
