// Part of `pnpm check:extensions` (spec 063, US2, FR-017, FR-008): what the window keeps of a mail
// link opened outside holzi (src/lib/mail/link.ts) and that the links of shares still go their
// own way (src/lib/contacts/deepLink.ts).
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  CONTACT_LINK_PREFIX,
  REPEAT_WITHIN_MS,
  parkOrOpen,
} from '../src/lib/contacts/deepLink.ts'
import {
  MAX_MAIL_LINK_BYTES,
  acceptMailLink,
  mailLinkText,
} from '../src/lib/mail/link.ts'

const LINK = 'mailto:ben@example.org?subject=Hallo'

test('a mail link is kept as it is, in any case of the scheme', () => {
  assert.equal(acceptMailLink(LINK, null, 0), LINK)
  assert.equal(acceptMailLink(`  ${LINK}\n`, null, 0), LINK)
  assert.equal(
    acceptMailLink('MAILTO:ben@example.org', null, 0),
    'MAILTO:ben@example.org',
  )
  assert.equal(
    acceptMailLink('mailto:?subject=x', null, 0),
    'mailto:?subject=x',
  )
})

test('another scheme or no text is ignored', () => {
  for (const other of [
    '',
    'tel:+491701234567',
    'sms:+491701234567',
    'https://example.com/mailto:ben@example.org',
    'xmailto:ben@example.org',
    `${CONTACT_LINK_PREFIX}AAAA`,
    null,
    undefined,
    42,
  ]) {
    assert.equal(acceptMailLink(other, null, 0), null, String(other))
  }
})

test('a link over 32 KiB is ignored, the limit itself is not', () => {
  const fits = 'mailto:' + 'a'.repeat(MAX_MAIL_LINK_BYTES - 'mailto:'.length)
  assert.equal(mailLinkText(fits), fits)
  assert.equal(mailLinkText(`${fits}a`), null)
  // The limit counts bytes: two-byte characters reach it with half as many.
  const wide = 'mailto:' + 'ü'.repeat(MAX_MAIL_LINK_BYTES / 2)
  assert.equal(mailLinkText(wide), null)
})

test('the same link twice within three seconds is one link, a later one is new', () => {
  const last = { text: LINK, at: 1000 }
  assert.equal(acceptMailLink(LINK, last, 1000 + REPEAT_WITHIN_MS - 1), null)
  assert.equal(acceptMailLink(LINK, last, 1000 + REPEAT_WITHIN_MS), LINK)
})

test('a second, different link is accepted while the first still waits', () => {
  const first = { text: LINK, at: 0 }
  assert.equal(
    acceptMailLink('mailto:ada@example.org', first, 10),
    'mailto:ada@example.org',
  )
})

test('a mail link is no share and a share is no mail link', () => {
  assert.equal(parkOrOpen(LINK, null, null, 0), null)
  assert.equal(mailLinkText(`${CONTACT_LINK_PREFIX}AAAA`), null)
  assert.equal(
    parkOrOpen(`${CONTACT_LINK_PREFIX}AAAA`, null, null, 0),
    `${CONTACT_LINK_PREFIX}AAAA`,
  )
})
