// How much a drive or a cloud storage holds, as the sidebar of the file browser shows it: a donut
// of the used share and, in a popover, the used, free and total bytes (`12,3 GB`).
// Decimal units (1 GB = 10^9 bytes), as providers and drive makers state sizes. Pure, so
// `check-files-space.ts` runs it without a DOM.

const UNITS = ['B', 'kB', 'MB', 'GB', 'TB', 'PB'] as const

/** The unit that fits `bytes`: the largest one in which it is at least 1. */
function unitOf(bytes: number): number {
  let unit = 0
  let value = bytes
  while (value >= 1000 && unit < UNITS.length - 1) {
    value /= 1000
    unit += 1
  }
  return unit
}

/** `12,3 GB`: `bytes` in the largest unit in which it is at least 1. */
export function formatSpace(bytes: number, locale?: string): string {
  const unit = unitOf(bytes)
  const number = new Intl.NumberFormat(locale, {
    maximumFractionDigits: 1,
  }).format(bytes / 1000 ** unit)
  return `${number} ${UNITS[unit]}`
}

/** The used share of `total` as 0..1; a used value beyond the total, or a total of 0, cannot overfill the donut. */
export function usedShare(used: number, total: number): number {
  return total > 0 ? Math.min(Math.max(used / total, 0), 1) : 0
}
