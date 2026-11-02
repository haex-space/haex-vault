/**
 * The time an entry of the chat's list shows (spec 072 FR-001): hour and minute for today, else day
 * and month, as messengers do.
 */
export function listTime(
  at: number,
  format: (date: Date, options: Intl.DateTimeFormatOptions) => string,
  now: Date = new Date(),
): string {
  const date = new Date(at)
  return now.toDateString() === date.toDateString()
    ? format(date, { hour: '2-digit', minute: '2-digit' })
    : format(date, { day: '2-digit', month: '2-digit' })
}
