// Part of `pnpm check:contacts`: what the window keeps of a link opened outside holzi
// (src/lib/contacts/deepLink.ts, spec 060 FR-018a).
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  CONTACT_LINK_PREFIX,
  MAX_LINK_LENGTH,
  REPEAT_WITHIN_MS,
  contactLinkText,
  parkOrOpen,
} from '../src/lib/contacts/deepLink.ts'

const LINK = `${CONTACT_LINK_PREFIX}AAAA`

test('a link of a share is kept, as the text of the share', () => {
  assert.equal(parkOrOpen(LINK, null, null, 0), LINK)
  assert.equal(parkOrOpen(`  ${LINK}\n`, null, null, 0), LINK)
})

test('a foreign scheme, another host or no payload is ignored', () => {
  for (const other of [
    '',
    CONTACT_LINK_PREFIX,
    'haex-vault://other/AAAA',
    'haexvault://contact/AAAA',
    'https://example.com/?haex-vault://contact/AAAA',
    'mailto:ben@example.com',
    'holzi:card:AAAA',
    null,
    42,
  ]) {
    assert.equal(parkOrOpen(other, null, null, 0), null, String(other))
  }
})

test('a link longer than a share can be is ignored', () => {
  const rest = MAX_LINK_LENGTH - CONTACT_LINK_PREFIX.length
  assert.notEqual(contactLinkText(CONTACT_LINK_PREFIX + 'A'.repeat(rest)), null)
  assert.equal(
    contactLinkText(CONTACT_LINK_PREFIX + 'A'.repeat(rest + 1)),
    null,
  )
})

test('a second link does not replace the first before the first was consumed', () => {
  const first = LINK
  const second = `${CONTACT_LINK_PREFIX}BBBB`
  assert.equal(parkOrOpen(second, first, null, 0), null)
  // consumed: nothing waits any more
  assert.equal(parkOrOpen(second, null, { text: first, at: 0 }, 10_000), second)
})

test('the same link through getCurrent and onOpenUrl is one link', () => {
  const last = { text: LINK, at: 1000 }
  assert.equal(parkOrOpen(LINK, null, last, 1000 + REPEAT_WITHIN_MS - 1), null)
  assert.equal(parkOrOpen(LINK, null, last, 1000 + REPEAT_WITHIN_MS), LINK)
  assert.equal(
    parkOrOpen(`${CONTACT_LINK_PREFIX}BBBB`, null, last, 1001),
    `${CONTACT_LINK_PREFIX}BBBB`,
  )
})
