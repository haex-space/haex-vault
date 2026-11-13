/**
 * Which crumbs of a trail are drawn when the room is short (`CrumbTrail`): the folders at the front
 * give way to a "…" menu and the ones at the back stay. Pure, on the rectangles of a measured copy
 * of the whole trail, so the choice follows the width of the box and never the width of what is shown.
 */

export type CrumbRect = { left: number; right: number; width: number }

export type CrumbMeasure = {
  /** The crumbs, in order, each without its separator. */
  items: readonly CrumbRect[]
  /** The separators: `separators[i]` stands between crumb `i` and crumb `i + 1`. */
  separators: readonly CrumbRect[]
  /** The "…" button's own width. */
  more: number
}

/** The smallest width the open folder's name keeps before it moves into the menu too. */
export const MIN_LAST_CRUMB = 56

/**
 * The index of the first crumb to draw: `0` when the whole trail fits, a larger one when the crumbs
 * before it go into the menu, and `items.length` when not even the open folder keeps `minLast`
 * (then the menu holds all of them).
 */
export function firstShownCrumb(
  { items, separators, more }: CrumbMeasure,
  available: number,
  minLast: number = MIN_LAST_CRUMB,
): number {
  const count = items.length
  const head = items[0]
  const tail = items[count - 1]
  if (!head || !tail || tail.right - head.left <= available) return 0

  const firstSep = separators[0]
  const lastSep = separators[count - 2]
  const gap = firstSep ? firstSep.left - head.right : 0
  // The span from crumb `i` (with the separator before it) to the end of the trail.
  const span = (i: number) =>
    tail.right - (separators[i - 1]?.left ?? head.left)
  const sepWidth = lastSep ? lastSep.width : 0

  let start = count - 1
  while (start > 1 && more + gap + span(start - 1) <= available) start--
  const onlyLast =
    count === 1 || more + gap + sepWidth + gap + minLast <= available
  return start === count - 1 && !onlyLast ? count : start
}
