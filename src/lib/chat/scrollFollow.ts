// When the transcript follows new content (pure, so the Node check scripts can load it). A reply
// that is being written scrolls the transcript along only while the reader is at its end; once
// the reader scrolled up to read earlier messages, nothing pulls the view down again.

/** How far from the end (px) the reader still counts as being at the end: a scroll position is
 * fractional on scaled displays, so an exact comparison would lose the end by a pixel. */
export const BOTTOM_TOLERANCE_PX = 16

export type ScrollMetrics = {
  scrollHeight: number
  scrollTop: number
  clientHeight: number
}

/** Whether the view shows the end of its content. A view whose content fits is at its end. */
export function isAtBottom(
  metrics: ScrollMetrics,
  tolerance = BOTTOM_TOLERANCE_PX,
): boolean {
  return (
    metrics.scrollHeight - metrics.scrollTop - metrics.clientHeight <= tolerance
  )
}

/**
 * Whether the transcript follows new content after a scroll event. Only a move up stops following,
 * however small: while a reply streams, every token pins the view to the end again, so a touchpad
 * or an animated wheel step that stays within the tolerance would otherwise never get away from
 * it. Reaching the end (within the tolerance) follows again. Any other scroll keeps the state, e.g.
 * the late event of a programmatic jump to the end after more content arrived. A view clamped to
 * its end because its content shrank also moves up, but lands on the end and keeps following.
 */
export function followsAfterScroll(
  following: boolean,
  previousScrollTop: number,
  metrics: ScrollMetrics,
  tolerance = BOTTOM_TOLERANCE_PX,
): boolean {
  const distance =
    metrics.scrollHeight - metrics.scrollTop - metrics.clientHeight
  if (metrics.scrollTop < previousScrollTop && distance > 1) return false
  if (isAtBottom(metrics, tolerance)) return true
  return following
}
