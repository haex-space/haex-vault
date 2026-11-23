// Part of `pnpm check:chat-state` (spec 095 T041, US4): every protection state Rust answers has a
// label, an icon and an explanation in both languages, and no delivery or protection text says
// that a Nostr-Relay's acceptance means the message was delivered (US4 scenario 3, FR-010).
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'

import {
  PROTECTION_LIMITS,
  PROTECTION_STATES,
  PROTECTION_VIEWS,
} from '../src/lib/chat/protection.ts'

type Messages = { [key: string]: string | Messages }

const LOCALES = ['de', 'en'] as const
const messages = Object.fromEntries(
  LOCALES.map((locale) => [
    locale,
    JSON.parse(
      readFileSync(
        new URL(`../src/i18n/locales/${locale}.json`, import.meta.url),
        'utf8',
      ),
    ) as Messages,
  ]),
) as Record<(typeof LOCALES)[number], Messages>

function lookup(tree: Messages, key: string): string | Messages | undefined {
  let node: string | Messages | undefined = tree
  for (const part of key.split('.')) {
    if (typeof node !== 'object' || node === null) return undefined
    node = node[part]
  }
  return node
}

function textsUnder(tree: Messages, key: string): string[] {
  const node = lookup(tree, key)
  if (typeof node === 'string') return [node]
  if (node === undefined) return []
  return Object.keys(node).flatMap((child) =>
    textsUnder(tree, `${key}.${child}`),
  )
}

test('the states match the ones Rust exports', () => {
  const binding = readFileSync(
    new URL(
      '../src/types/bindings/ChatPeerProtectionState.ts',
      import.meta.url,
    ),
    'utf8',
  )
  const exported = [...binding.matchAll(/"([A-Za-z]+)"/g)].map((m) => m[1])
  assert.deepEqual([...exported].sort(), [...PROTECTION_STATES].sort())
  assert.deepEqual(
    Object.keys(PROTECTION_VIEWS).sort(),
    [...PROTECTION_STATES].sort(),
  )
})

test('every state has an icon, and its label and explanation in both languages', () => {
  for (const state of PROTECTION_STATES) {
    const view = PROTECTION_VIEWS[state]
    assert.match(view.icon, /^lucide:[a-z-]+$/, state)
    assert.equal(view.warning, state !== 'protected', state)
    for (const locale of LOCALES) {
      for (const key of [view.label, view.explanation]) {
        const text = lookup(messages[locale], key)
        assert.equal(typeof text, 'string', `${locale}: ${key}`)
        assert.ok((text as string).trim().length > 0, `${locale}: ${key}`)
      }
    }
  }
})

test('the limits of the protection are said in both languages', () => {
  for (const locale of LOCALES) {
    for (const key of [
      ...PROTECTION_LIMITS,
      'chat.peer.protection.renewedAt',
    ]) {
      assert.equal(
        typeof lookup(messages[locale], key),
        'string',
        `${locale}: ${key}`,
      )
    }
  }
  // FR-014: relays see routing, timing, size and the network address.
  const relays = lookup(messages.en, 'chat.peer.protection.relays') as string
  for (const seen of ['which device', 'when', 'how large', 'network address']) {
    assert.ok(relays.includes(seen), seen)
  }
})

test('no text says that a relay accepting a message means it was delivered', () => {
  const delivered = { de: /zugestellt/i, en: /deliver/i }
  const accepted = { de: /angenommen/i, en: /accepted/i }
  for (const locale of LOCALES) {
    const texts = [
      ...textsUnder(messages[locale], 'chat.delivery'),
      ...textsUnder(messages[locale], 'chat.peer.protection'),
    ]
    assert.ok(texts.length > 10, locale)
    for (const text of texts) {
      if (accepted[locale].test(text)) {
        assert.doesNotMatch(text, delivered[locale], `${locale}: ${text}`)
      }
    }
    const sent = lookup(messages[locale], 'chat.delivery.state.sent') as string
    assert.match(sent, accepted[locale], `${locale}: sent names the acceptance`)
    const confirmed = lookup(
      messages[locale],
      'chat.delivery.state.delivered',
    ) as string
    assert.doesNotMatch(confirmed, accepted[locale], `${locale}: delivered`)
  }
})
