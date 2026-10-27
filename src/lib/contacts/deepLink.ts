// Links of a share, opened outside holzi (spec 060 FR-018a). The checks of what the window does
// with a link live here so Node can run them (scripts/check-contacts-deeplink.ts).

/** What the link of a share starts with; other schemes and other hosts are not holzi's. */
export const CONTACT_LINK_PREFIX = 'haex-vault://contact/'

/** More than a share can be (16,384 characters of text, FR-018) is not a share. Rust agrees. */
export const MAX_LINK_LENGTH = 16 * 1024

/** The same link arriving twice within this time is one link (a start brings it through
 * `getCurrent` and through `onOpenUrl`). */
export const REPEAT_WITHIN_MS = 3000

/** The text of a share if `url` is the link of one: the link itself is the share's text. */
export function contactLinkText(url: unknown): string | null {
  if (typeof url !== 'string') return null
  const text = url.trim()
  if (
    text.length <= CONTACT_LINK_PREFIX.length ||
    text.length > MAX_LINK_LENGTH ||
    !text.startsWith(CONTACT_LINK_PREFIX)
  ) {
    return null
  }
  return text
}

export interface ReceivedLink {
  text: string
  at: number
}

/** Whether `text` is the link that arrived just before (so it is not shown again). */
export function isRepeat(
  last: ReceivedLink | null,
  text: string,
  now: number,
): boolean {
  return last !== null && last.text === text && now - last.at < REPEAT_WITHIN_MS
}

/**
 * What to keep of a link that arrived: its text, or nothing when it is no share, is a repeat, or a
 * link waits that nobody has looked at yet. The first link is never replaced by a later one before
 * it was consumed or dismissed (`pending` is null then), so a link the person has not seen is not
 * lost to the next one. The window opens the preview from what is kept, once a vault is open.
 */
export function parkOrOpen(
  url: unknown,
  pending: string | null,
  last: ReceivedLink | null,
  now: number,
): string | null {
  const text = contactLinkText(url)
  if (text === null || pending !== null || isRepeat(last, text, now))
    return null
  return text
}
