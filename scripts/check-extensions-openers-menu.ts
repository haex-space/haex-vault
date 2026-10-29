// Part of `pnpm check:extensions` (spec 063, US1, FR-001, FR-018): who the mail button offers and
// where a link from outside may not go (src/lib/mail/openers.ts).
import assert from 'node:assert/strict'
import { test } from 'node:test'

import type { ExtensionSummary } from '../src/types/bindings/ExtensionSummary.ts'
import {
  MAIL_COMPOSE,
  mailTargetBehavior,
  openerRows,
  systemAllowed,
} from '../src/lib/mail/openers.ts'

function summary(
  id: string,
  title: string,
  patch: Partial<ExtensionSummary> = {},
): ExtensionSummary {
  return {
    id,
    name: title,
    title,
    publisherFingerprint: 'aaaa',
    enabled: true,
    state: 'installed',
    singleInstance: false,
    hasIcon: false,
    devices: [],
    dev: false,
    openers: [MAIL_COMPOSE],
    ...patch,
  }
}

test('without an extension that writes mail the button opens the system as before', () => {
  assert.equal(mailTargetBehavior(openerRows([])), 'system')
  assert.equal(
    mailTargetBehavior(openerRows([summary('a', 'Notes', { openers: [] })])),
    'system',
  )
})

test('two openers are two rows with their names, sorted', () => {
  const rows = openerRows([
    summary('b', 'haex-mail'),
    summary('a', 'Briefe'),
    summary('c', 'Notes', { openers: [] }),
  ])
  assert.deepEqual(rows, [
    { extensionId: 'a', title: 'Briefe' },
    { extensionId: 'b', title: 'haex-mail' },
  ])
  assert.equal(mailTargetBehavior(rows), 'menu')
})

test('a switched-off extension is not offered', () => {
  assert.deepEqual(openerRows([summary('a', 'Mail', { enabled: false })]), [])
})

test('the order is the same however the list comes', () => {
  const list = [summary('2', 'Mail'), summary('1', 'Mail'), summary('3', 'Abc')]
  assert.deepEqual(openerRows(list), openerRows([...list].reverse()))
})

test('a link from outside never goes to the system, the button may', () => {
  assert.equal(systemAllowed('button'), true)
  assert.equal(systemAllowed('external'), false)
})
