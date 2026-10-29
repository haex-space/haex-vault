// Mail links opened outside holzi (spec 063, US2, FR-017). Pure: `useDeepLink` feeds it, and
// `scripts/check-extensions-openers-link.ts` tests it. Whether the link is a good one is for
// Rust to say (`opener_mail_check`); the window only keeps what may be one.
import { isRepeat, type ReceivedLink } from '../contacts/deepLink.ts'

const SCHEME = 'mailto:'

/** The largest link holzi reads (FR-008), in bytes. Rust agrees (`openers::mail`). */
export const MAX_MAIL_LINK_BYTES = 32 * 1024

/** The text of a mail link if `url` is one: only the scheme and the size are checked here. */
export function mailLinkText(url: unknown): string | null {
  if (typeof url !== 'string') return null
  const text = url.trim()
  if (
    text.slice(0, SCHEME.length).toLowerCase() !== SCHEME ||
    new TextEncoder().encode(text).length > MAX_MAIL_LINK_BYTES
  ) {
    return null
  }
  return text
}

/**
 * What to keep of a link that arrived: its text, or nothing when it is no mail link or is the one
 * that arrived just before (a start brings it twice). A link that arrives while another waits
 * replaces it, even when its dialog is open: the person's last tap wins (research R6).
 */
export function acceptMailLink(
  url: unknown,
  last: ReceivedLink | null,
  now: number,
): string | null {
  const text = mailLinkText(url)
  if (text === null || isRepeat(last, text, now)) return null
  return text
}
