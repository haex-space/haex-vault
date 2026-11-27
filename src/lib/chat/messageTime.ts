import { daysBetween } from './listTime.ts'

/**
 * The time a message of a conversation shows: the clock time for a message of today, the short date
 * with the clock time for an older one, in the language `locale`. Shared by the contact
 * conversation and the agent's transcript.
 */
export function messageTime(at: number, now: Date, locale: string): string {
  const date = new Date(at)
  return date.toDateString() === now.toDateString()
    ? date.toLocaleTimeString(locale, { hour: '2-digit', minute: '2-digit' })
    : date.toLocaleString(locale, { dateStyle: 'short', timeStyle: 'short' })
}

/** The full date and clock time of a message, for its tooltip. */
export function messageDateTime(at: number, locale: string): string {
  return new Date(at).toLocaleString(locale, {
    dateStyle: 'medium',
    timeStyle: 'short',
  })
}

export interface DayLabels {
  today: string
  yesterday: string
  /** Any earlier day. */
  date: (date: Date) => string
}

/** The heading of a day in a transcript: "Today", "Yesterday", else the date. */
export function dayLabel(at: number, now: Date, labels: DayLabels): string {
  const date = new Date(at)
  const days = daysBetween(date, now)
  if (days <= 0) return labels.today
  if (days === 1) return labels.yesterday
  return labels.date(date)
}

/** The ids of the messages that open a new calendar day (the first one included), in order. */
export function dayStarts(messages: { id: string; createdAt: number }[]) {
  const starts = new Set<string>()
  let previous: string | null = null
  for (const m of messages) {
    const day = new Date(m.createdAt).toDateString()
    if (day !== previous) starts.add(m.id)
    previous = day
  }
  return starts
}
