// Sorting the list by date (spec 092, FR-021): upcoming first with the nearest on top, passes that
// are over after them with the most recent first, entries without a date last, ties by title. A
// pass is dated by its start; a date alone sorts at 00:00 of the device's zone and counts as
// upcoming through its day. A pass without a start, and a login, is dated by its expiry date (the
// end). Pure, so `scripts/check-passwords-sort-filter.ts` runs it without vue.
import { isExpiredPass, startInstant } from './passDate.ts'

/** The part of a header the date sort reads. */
export type SortableHeader = {
  id: string
  title?: string | null
  kind?: string | null
  startDate?: string | null
  startAt?: string | null
  startZone?: string | null
  expiresAt?: string | null
}

/** Midnight at the start of a `YYYY-MM-DD` day in the device's zone, `null` for any other text. */
function localMidnight(date: string | null | undefined): number | null {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(date ?? '')
  if (!match) return null
  const [year, month, day] = [
    Number(match[1]),
    Number(match[2]),
    Number(match[3]),
  ]
  const built = new Date(year, month - 1, day)
  // `new Date` rolls 2030-13-01 over into 2031; a day that does not exist is no date.
  if (
    built.getFullYear() !== year ||
    built.getMonth() !== month - 1 ||
    built.getDate() !== day
  ) {
    return null
  }
  return built.getTime()
}

type DateKey = { at: number; upcoming: boolean }

/** Where a header sits in time, or `null` when it has no date. */
function dateKey(header: SortableHeader, now: Date): DateKey | null {
  if (header.kind === 'pass') {
    if (header.startAt) {
      const at = startInstant(header)
      return at === null ? null : { at, upcoming: at >= now.getTime() }
    }
    const day = localMidnight(header.startDate || header.expiresAt)
    if (day === null) return null
    const today = new Date(
      now.getFullYear(),
      now.getMonth(),
      now.getDate(),
    ).getTime()
    return { at: day, upcoming: day >= today }
  }
  const day = localMidnight(header.expiresAt)
  if (day === null) return null
  const today = new Date(
    now.getFullYear(),
    now.getMonth(),
    now.getDate(),
  ).getTime()
  return { at: day, upcoming: day >= today }
}

const byTitle = (a: SortableHeader, b: SortableHeader): number =>
  (a.title ?? '').localeCompare(b.title ?? '', undefined, {
    sensitivity: 'base',
    numeric: true,
  }) || a.id.localeCompare(b.id)

/** The headers in the order of FR-021. The input is not changed. */
export function sortByDate<T extends SortableHeader>(
  headers: readonly T[],
  now: Date,
): T[] {
  const keyed = headers.map((header) => ({
    header,
    key: dateKey(header, now),
  }))
  const rank = (entry: { key: DateKey | null }) =>
    entry.key === null ? 2 : entry.key.upcoming ? 0 : 1
  return keyed
    .sort((a, b) => {
      const byRank = rank(a) - rank(b)
      if (byRank !== 0) return byRank
      if (a.key !== null && b.key !== null && a.key.at !== b.key.at) {
        // Upcoming: the nearest first. Over: the most recent first.
        return a.key.upcoming ? a.key.at - b.key.at : b.key.at - a.key.at
      }
      return byTitle(a.header, b.header)
    })
    .map((entry) => entry.header)
}

/** The part of a header the kind filter reads. */
export type KindHeader = { passKind?: string | null }

/**
 * The passes of one kind; `null` keeps them all. A missing or unknown kind (one a newer holzi wrote)
 * counts as `other`, like everywhere else (spec 092, FR-004).
 */
export function filterByKind<T extends KindHeader>(
  headers: readonly T[],
  kind: string | null,
  kinds: readonly string[],
): T[] {
  if (kind === null) return [...headers]
  return headers.filter((header) => {
    const own = header.passKind
    const effective = own && kinds.includes(own) ? own : 'other'
    return effective === kind
  })
}

/** The passes that are not over (FR-010): an end before today's date on the device's calendar
 * hides it, a pass that ends today stays; one without an end never hides. */
export function hideExpired<T extends { expiresAt?: string | null }>(
  headers: readonly T[],
  now: Date,
): T[] {
  return headers.filter((header) => !isExpiredPass(header, now))
}
