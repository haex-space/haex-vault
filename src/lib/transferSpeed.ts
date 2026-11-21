// How fast a download or transfer runs, from the progress reports of its backend. Pure.

/** `8.3 MB/s`, `640 kB/s`: the rate of a transfer, in decimal units like the download hosts show. */
export function formatSpeed(bytesPerSecond: number): string {
  if (bytesPerSecond >= 1e6) return `${(bytesPerSecond / 1e6).toFixed(1)} MB/s`
  return `${Math.round(bytesPerSecond / 1e3)} kB/s`
}

/** How many bytes of a transfer had arrived at a monotonic-clock timestamp. */
export interface ProgressSample {
  at: number
  bytes: number
}

/** The speed is the average over this long, so it follows the connection without jumping. */
export const SPEED_WINDOW_MS = 4000

/**
 * The speed of a transfer in bytes per second, averaged over the samples of the last
 * `SPEED_WINDOW_MS`; `null` while those span less than half a second, which is also what a
 * stalled transfer shows once its last sample is older than the window.
 */
export function downloadSpeed(
  samples: readonly ProgressSample[],
  now: number,
): number | null {
  const recent = samples.filter((sample) => sample.at >= now - SPEED_WINDOW_MS)
  const first = recent[0]
  const last = recent[recent.length - 1]
  if (!first || !last) return null
  const span = last.at - first.at
  if (span < 500 || last.bytes < first.bytes) return null
  return ((last.bytes - first.bytes) / span) * 1000
}
