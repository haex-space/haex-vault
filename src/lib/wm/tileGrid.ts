// The grid of app tiles (spec 091-tab-switcher, FR-003a, contracts/app-tiles.md): how many columns a
// width gives and where the keyboard moves the focus. The grid scrolls, so nothing here counts rows
// or pages. Pure — relative sibling imports with an explicit .ts extension, so the Node check script
// (`scripts/check-wm-tile-grid.ts`) loads it without Nuxt.
import { moveIndex, type MoveTarget } from '../files/shortcuts.ts'

/** The space between two tiles, in px (also the grid's `gap`). */
export const GAP = 12
/** A tile is never narrower than this: a full touch target, and room for a two-line name. */
export const MIN_TILE_WIDTH = 112
/** As many columns as COSMIC's app library shows on a wide screen. */
export const MAX_COLUMNS = 7
/** A compact window (a phone) keeps big tiles instead of three cramped ones. */
const COMPACT_MAX_COLUMNS = 2

/**
 * The number of columns for a grid `width` px wide: the most that keep every tile at least
 * `MIN_TILE_WIDTH` wide, at least one, at most `MAX_COLUMNS` (two when compact). It only follows
 * the width, so a window made smaller or larger re-flows live. The same number drives the CSS grid
 * and the keyboard's row steps.
 */
export function gridColumns(
  width: number,
  options: { compact: boolean },
): number {
  const fitting = Math.floor((width + GAP) / (MIN_TILE_WIDTH + GAP))
  const most = options.compact ? COMPACT_MAX_COLUMNS : MAX_COLUMNS
  return Math.min(Math.max(fitting, 1), most)
}

/** The keys that move the focus, and where each takes it (`rtl` swaps left and right). */
function target(key: string, rtl: boolean): MoveTarget | null {
  switch (key) {
    case 'ArrowRight':
      return rtl ? 'previous' : 'next'
    case 'ArrowLeft':
      return rtl ? 'next' : 'previous'
    case 'ArrowDown':
      return 'below'
    case 'ArrowUp':
      return 'above'
    case 'Home':
      return 'first'
    case 'End':
      return 'last'
    default:
      return null
  }
}

/**
 * The index the focus moves to when `key` is pressed on tile `index` (-1: none has it) among
 * `count` focusable tiles in rows of `columns`: arrows by one tile or one row, `Home` and `End`
 * to the ends, no wrap. `null` for a key that moves nothing (so the caller keeps its default) and
 * for an empty grid. The moving itself is the file browser's grid move (`moveIndex`, spec 044),
 * not a second copy.
 *
 * ponytail: the move lives in `lib/files/shortcuts.ts`. A third user is the time to lift it into a
 * neutral module; two importers do not justify touching the file browser's code now.
 */
export function moveFocus(
  index: number,
  key: string,
  columns: number,
  count: number,
  rtl: boolean,
): number | null {
  if (count <= 0) return null
  const to = target(key, rtl)
  return to === null ? null : moveIndex(index, to, count, columns)
}
