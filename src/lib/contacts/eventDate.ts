// Dates of events (birthday, anniversary) as a person reads and types them. A date is stored as
// `YYYY-MM-DD`, or `--MM-DD` without a year (as vCard and Android have it); the window shows and
// takes it in the order and with the separator of the chosen language, `TT.MM.JJJJ` in German. Pure,
// so `scripts/check-contacts-dates.ts` runs it without vue.

type Part = 'day' | 'month' | 'year'

export type DateLetters = { day: string; month: string; year: string }

/** The order of day, month and year and the separator of a language. */
function layout(locale: string): { order: Part[]; separator: string } {
  const parts = new Intl.DateTimeFormat(locale, {
    day: '2-digit',
    month: '2-digit',
    year: 'numeric',
    timeZone: 'UTC',
  }).formatToParts(Date.UTC(2000, 10, 22))
  const order = parts
    .map((part) => part.type)
    .filter((type): type is Part => ['day', 'month', 'year'].includes(type))
  const separator =
    parts.find((part) => part.type === 'literal')?.value.trim() || '.'
  return { order, separator }
}

const STORED = /^(?:(\d{4})|-)-(\d{2})-(\d{2})$/

/** A stored date in the order of the language: `17.05.1990` or, without a year, `17.05.`. Anything
 * else (a date the person is still typing) stays as it is. */
export function formatEventDate(stored: string, locale: string): string {
  const match = STORED.exec(stored.trim())
  if (!match) return stored
  const { order, separator } = layout(locale)
  const values: Record<Part, string | undefined> = {
    year: match[1],
    month: match[2],
    day: match[3],
  }
  const shown = order
    .map((part) => values[part])
    .filter((value): value is string => value !== undefined)
  const text = shown.join(separator)
  return values.year === undefined && separator === '.' ? `${text}.` : text
}

/** What the person typed as the stored date, or `null` while it is not a date yet: three numbers
 * in the order of the language (a year of four digits), or two for a date without a year. A date
 * typed as `1990-05-17` is understood in any language. */
export function parseTypedDate(input: string, locale: string): string | null {
  const numbers = input.trim().match(/\d+/g)
  if (!numbers || input.trim() === '') return null
  const [first, second, third] = numbers
  let year: string | undefined
  let month: string | undefined
  let day: string | undefined
  if (numbers.length === 3 && first?.length === 4) {
    ;[year, month, day] = [first, second, third]
  } else {
    const { order } = layout(locale)
    const wanted = order.filter(
      (part) => numbers.length === 3 || part !== 'year',
    )
    if (numbers.length !== wanted.length) return null
    const byPart = new Map(wanted.map((part, index) => [part, numbers[index]]))
    year = byPart.get('year')
    month = byPart.get('month')
    day = byPart.get('day')
    if (year !== undefined && year.length !== 4) return null
  }
  if (month === undefined || day === undefined) return null
  const [m, d] = [Number(month), Number(day)]
  if (!(m >= 1 && m <= 12 && d >= 1 && d <= 31)) return null
  const pad = (value: string) => value.padStart(2, '0')
  return year === undefined
    ? `--${pad(month)}-${pad(day)}`
    : `${year}-${pad(month)}-${pad(day)}`
}

/** The hint of the field: `TT.MM.JJJJ oder TT.MM.`, with the letters of the language. */
export function datePattern(
  locale: string,
  letters: DateLetters,
): { full: string; short: string } {
  const { order, separator } = layout(locale)
  const full = order.map((part) => letters[part]).join(separator)
  const short = order
    .filter((part) => part !== 'year')
    .map((part) => letters[part])
    .join(separator)
  return { full, short: separator === '.' ? `${short}.` : short }
}
