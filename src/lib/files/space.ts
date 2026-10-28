// How much a drive or a cloud storage holds, as the sidebar of the file browser shows it:
// `12,3/100 GB`, the used and the total in the unit of the total, so what is free reads at a glance.
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

/**
 * `12,3/100 GB` for `used` of `total` bytes; `12,3 GB` while the total is not known. The unit is the
 * one of the total (of the used bytes without it).
 */
export function formatSpace(
  used: number,
  total: number | null,
  locale?: string,
): string {
  const unit = unitOf(total ?? used)
  const scale = 1000 ** unit
  const number = (bytes: number) =>
    new Intl.NumberFormat(locale, {
      // Below a tenth of the unit a second digit keeps a small value from reading as nothing.
      maximumFractionDigits: unit > 0 && bytes / scale < 0.1 ? 2 : 1,
    }).format(bytes / scale)
  return total === null
    ? `${number(used)} ${UNITS[unit]}`
    : `${number(used)}/${number(total)} ${UNITS[unit]}`
}
