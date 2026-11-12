// Dates and time zones of a pass (spec 092-wallet-passes, FR-002, research R4). Pure, so
// `scripts/check-passwords-pass-date.ts` runs it in Node without vue.
//
// A pass starts either on a date alone (`startDate`, `YYYY-MM-DD`: no time, no zone) or at a point
// in time (`startAt`, an RFC 3339 instant) together with `startZone`, the zone of the PLACE where it
// happens: an IANA name (`Europe/London`) or a fixed offset (`+02:00`). The end is `expiresAt`, a
// date. The time zone database is the one of `Intl`; fixed offsets are plain arithmetic.

/** The start fields of a header; all may be missing, at most one of the two starts is set. */
export type PassStartFields = {
  startDate?: string | null
  startAt?: string | null
  startZone?: string | null
}

/** How a start is shown: always the time of the place, plus the device's time when it differs. */
export type FormattedStart = { place: string; device?: string }

type Zone = {
  /** Offset from UTC in minutes at the instant (ms since epoch). */
  offsetMinutes: (instant: number) => number
  /** Date and time of the instant on the wall clock of this zone, with the zone label. */
  format: (instant: number, locale?: string) => string
}

const DATE_RE = /^(\d{4})-(\d{2})-(\d{2})$/
const INSTANT_RE =
  /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})$/
const OFFSET_RE = /^([+-])(\d{2}):(\d{2})$/

function isRealDate(year: number, month: number, day: number): boolean {
  const date = new Date(Date.UTC(year, month - 1, day))
  return (
    date.getUTCFullYear() === year &&
    date.getUTCMonth() === month - 1 &&
    date.getUTCDate() === day
  )
}

/** Milliseconds since the epoch of the UTC midnight of a `YYYY-MM-DD` date; null when it is not one. */
function parseDate(text: string): number | null {
  const m = DATE_RE.exec(text)
  if (!m) {
    return null
  }
  const [year, month, day] = [Number(m[1]), Number(m[2]), Number(m[3])]
  return isRealDate(year, month, day) ? Date.UTC(year, month - 1, day) : null
}

/** Milliseconds since the epoch of an RFC 3339 instant; null when the text is not one. */
function parseInstant(text: string): number | null {
  const m = INSTANT_RE.exec(text)
  if (!m) {
    return null
  }
  const [year, month, day] = [Number(m[1]), Number(m[2]), Number(m[3])]
  if (!isRealDate(year, month, day)) {
    return null
  }
  if (Number(m[4]) > 23 || Number(m[5]) > 59 || Number(m[6]) > 59) {
    return null
  }
  const ms = Date.parse(text)
  return Number.isNaN(ms) ? null : ms
}

function fixedOffset(name: string): number | null {
  const m = OFFSET_RE.exec(name)
  if (!m) {
    return null
  }
  const hours = Number(m[2])
  const minutes = Number(m[3])
  if (hours > 23 || minutes > 59) {
    return null
  }
  return (m[1] === '-' ? -1 : 1) * (hours * 60 + minutes)
}

function dateTimeFormat(
  timeZone: string,
  locale: string | undefined,
  options: Intl.DateTimeFormatOptions,
): Intl.DateTimeFormat {
  return new Intl.DateTimeFormat(locale, { timeZone, ...options })
}

const UTC_ZONE: Zone = zoneOf('UTC', 'UTC')

/** A zone that reads the wall clock of an IANA zone; `label` is what is shown next to the time. */
function zoneOf(timeZone: string, label: string): Zone {
  const parts = dateTimeFormat(timeZone, 'en-US', {
    hourCycle: 'h23',
    year: 'numeric',
    month: 'numeric',
    day: 'numeric',
    hour: 'numeric',
    minute: 'numeric',
    second: 'numeric',
  })
  return {
    offsetMinutes: (instant) => {
      // Whole seconds only: the zone rules never change inside a second.
      const second = Math.floor(instant / 1000) * 1000
      const get: Record<string, number> = {}
      for (const part of parts.formatToParts(second)) {
        if (part.type !== 'literal') {
          get[part.type] = Number(part.value)
        }
      }
      const wall = Date.UTC(
        get.year!,
        get.month! - 1,
        get.day!,
        get.hour!,
        get.minute!,
        get.second!,
      )
      return Math.round((wall - second) / 60000)
    },
    format: (instant, locale) =>
      `${dateTimeFormat(timeZone, locale, { dateStyle: 'medium', timeStyle: 'short' }).format(instant)} ${label}`,
  }
}

/** The zone for a stored name: an IANA name, a fixed offset, or (unknown or missing) UTC. */
function resolveZone(name: string | null | undefined): Zone {
  const text = (name ?? '').trim()
  if (text === '') {
    return UTC_ZONE
  }
  const minutes = fixedOffset(text)
  if (minutes !== null) {
    const label = `UTC${text}`
    return {
      offsetMinutes: () => minutes,
      format: (instant, locale) =>
        `${dateTimeFormat('UTC', locale, { dateStyle: 'medium', timeStyle: 'short' }).format(instant + minutes * 60000)} ${label}`,
    }
  }
  try {
    return zoneOf(text, text)
  } catch {
    // RangeError: not a zone this runtime knows. Keep the name, show UTC.
    const utc = zoneOf('UTC', `UTC (${text})`)
    return { offsetMinutes: () => 0, format: utc.format }
  }
}

/**
 * The start of a pass as milliseconds since the epoch: the instant itself, or the UTC midnight of a
 * date alone. Null when there is no start or it is not valid. The moment wins when both are set.
 */
export function startInstant(
  header: PassStartFields | null | undefined,
): number | null {
  const at = header?.startAt
  if (at !== undefined && at !== null && at !== '') {
    return parseInstant(at)
  }
  const date = header?.startDate
  if (date !== undefined && date !== null && date !== '') {
    return parseDate(date)
  }
  return null
}

/**
 * The start as text. A date alone is a date (no time, no zone, never shifted). A moment is shown in
 * the time of the place with its zone label; `device` is added, also labelled, only when the
 * device's zone has a different offset at that instant. Null when there is no valid start.
 */
export function formatStart(
  header: PassStartFields | null | undefined,
  deviceZone: string,
  locale?: string,
): FormattedStart | null {
  const instant = startInstant(header)
  if (instant === null) {
    return null
  }
  const at = header?.startAt
  if (at === undefined || at === null || at === '') {
    return {
      place: dateTimeFormat('UTC', locale, { dateStyle: 'medium' }).format(
        instant,
      ),
    }
  }
  const place = resolveZone(header?.startZone)
  const device = resolveZone(deviceZone)
  const result: FormattedStart = { place: place.format(instant, locale) }
  if (place.offsetMinutes(instant) !== device.offsetMinutes(instant)) {
    result.device = device.format(instant, locale)
  }
  return result
}

function localToday(now: Date): string {
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`
}

/**
 * A pass is over when its expiry date is before today's date on the device's calendar; a pass that
 * expires today is still good. A missing or invalid date never expires, and an end before the start
 * is not an error.
 */
export function isExpiredPass(
  header: { expiresAt?: string | null } | null | undefined,
  now: Date,
): boolean {
  const expires = header?.expiresAt
  if (
    expires === undefined ||
    expires === null ||
    parseDate(expires) === null
  ) {
    return false
  }
  return expires < localToday(now)
}

const LOCAL_RE = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2})$/
const DAY_MS = 24 * 60 * 60 * 1000

/**
 * The UTC instant (RFC 3339 with milliseconds) of a wall clock reading `YYYY-MM-DDTHH:mm` (a
 * `datetime-local` value) at the PLACE in `zone` (an IANA name or a fixed offset; an unknown name
 * counts as UTC, like in `formatStart`). A reading the clock skips (spring forward) resolves to the
 * first instant after the gap; a reading the clock shows twice (fall back) to the earlier one.
 * Null when the text is not a valid reading.
 */
export function wallTimeToInstant(
  local: string,
  zone: string | null | undefined,
): string | null {
  const m = LOCAL_RE.exec(local)
  if (!m) {
    return null
  }
  const [year, month, day] = [Number(m[1]), Number(m[2]), Number(m[3])]
  const [hour, minute] = [Number(m[4]), Number(m[5])]
  if (!isRealDate(year, month, day) || hour > 23 || minute > 59) {
    return null
  }
  const wall = Date.UTC(year, month - 1, day, hour, minute)
  const { offsetMinutes } = resolveZone(zone)

  // The offsets in force a day before and after; at most one change lies between them.
  const before = offsetMinutes(wall - DAY_MS)
  const after = offsetMinutes(wall + DAY_MS)
  const candidates = [...new Set([before, after])]
    .map((offset) => ({ offset, instant: wall - offset * 60000 }))
    .filter(({ offset, instant }) => offsetMinutes(instant) === offset)
    .map(({ instant }) => instant)
  if (candidates.length > 0) {
    return new Date(Math.min(...candidates)).toISOString()
  }

  // In the gap: `wall - before` lies after the change, the change itself is up to one gap earlier.
  // Find the first second that already has the new offset.
  let high = wall - before * 60000
  let low = high - Math.abs(after - before) * 60000
  while (high - low > 1000) {
    const mid = Math.floor((low + high) / 2000) * 1000
    if (offsetMinutes(mid) === after) {
      high = mid
    } else {
      low = mid
    }
  }
  return new Date(high).toISOString()
}

/**
 * The wall clock `YYYY-MM-DDTHH:mm` at the place in `zone` of a UTC instant (RFC 3339); the inverse
 * of `wallTimeToInstant` for every reading the clock shows once. Null when the instant is not valid.
 */
export function instantToWallTime(
  at: string,
  zone: string | null | undefined,
): string | null {
  const instant = parseInstant(at)
  if (instant === null) {
    return null
  }
  const offset = resolveZone(zone).offsetMinutes(instant)
  return new Date(instant + offset * 60000).toISOString().slice(0, 16)
}
