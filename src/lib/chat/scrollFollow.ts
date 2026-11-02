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
