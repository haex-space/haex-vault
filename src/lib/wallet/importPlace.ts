// The place of an imported pass as the dialog edits it (spec 092, FR-019, FR-031): the folders below
// the root of Wallet as one line of text ("Work / Trips") and back. Pure, so
// `scripts/check-wallet-import.ts` runs it without vue.

/** The separator the dialog writes between folder names. */
const SEPARATOR = ' / '

/** The folders of a path as one line. */
export function pathText(path: readonly string[]): string {
  return path.join(SEPARATOR)
}

/**
 * The folders a line names, outermost first: split at `/`, each name trimmed, empty names left out
 * (so "Work//Trips/" is "Work", "Trips"). A line of only separators is the top of Wallet.
 */
export function textPath(text: string): string[] {
  return text
    .split('/')
    .map((name) => name.trim())
    .filter((name) => name !== '')
}

/** A tag name as the dialog sends it: trimmed, and none when empty. */
export function tagOrNull(text: string): string | null {
  const name = text.trim()
  return name === '' ? null : name
}

/**
 * The start of a pass in the fields the date helpers read (`startDate`, `startAt`, `startZone`), so
 * the dialog shows it as the list does.
 */
export function startFields(
  start: { date: string } | { at: string; zone: string } | null | undefined,
): { startDate?: string; startAt?: string; startZone?: string } {
  if (!start) return {}
  if ('date' in start) return { startDate: start.date }
  return { startAt: start.at, startZone: start.zone }
}
