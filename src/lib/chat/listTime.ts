/**
 * The time an entry of the chat's list shows (spec 072 FR-001), worded as messengers do: "Jetzt"
 * and minutes within the last hour, the clock time for the rest of today, "Gestern", the weekday
 * within the last week, else the date. The words come from the caller, so the rules stay testable
 * without the i18n plugin.
 */
export interface ListTimeLabels {
  /** Less than a minute ago. */
  now: string
  /** Within the last hour. */
  minutes: (count: number) => string
  /** Today, an hour or more ago. */
  clock: (date: Date) => string
  yesterday: string
  /** The last six days before yesterday. */
  weekday: (date: Date) => string
  /** Anything older; `withYear` when it is not this year. */
  date: (date: Date, withYear: boolean) => string
}

const MINUTE = 60_000

/** Whole calendar days from `date` to `now` (0 = same day), by local midnights. */
function daysBetween(date: Date, now: Date): number {
  const start = (d: Date) =>
    new Date(d.getFullYear(), d.getMonth(), d.getDate())
  return Math.round((start(now).getTime() - start(date).getTime()) / 86_400_000)
}

export function listTime(
  at: number,
  labels: ListTimeLabels,
  now: Date = new Date(),
): string {
  const date = new Date(at)
  // A clock a little ahead of the sender's reads as just now, not as a future time.
  const age = Math.max(0, now.getTime() - at)
  if (age < MINUTE) return labels.now
  if (age < 60 * MINUTE) return labels.minutes(Math.floor(age / MINUTE))
  const days = daysBetween(date, now)
  if (days <= 0) return labels.clock(date)
  if (days === 1) return labels.yesterday
  if (days < 7) return labels.weekday(date)
  return labels.date(date, date.getFullYear() !== now.getFullYear())
}
