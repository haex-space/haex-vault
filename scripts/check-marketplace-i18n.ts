// Part of `pnpm check:marketplace` (spec 069): every error kind, source error kind and reason a
// version cannot be installed has a text in both languages, and the languages have the same keys.
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'

import {
  BLOCKED_REASONS,
  MARKETPLACE_REASONS,
  SOURCE_ERROR_KINDS,
  reasonKey,
  sourceErrorKey,
} from '../src/lib/marketplace/errors.ts'

type Messages = { [key: string]: string | Messages }

const load = (language: string): Messages =>
  JSON.parse(
    readFileSync(
      new URL(`../src/i18n/locales/${language}.json`, import.meta.url),
      'utf8',
    ),
  )

function lookup(messages: Messages, key: string): unknown {
  return key
    .split('.')
    .reduce<unknown>(
      (node, part) =>
        node && typeof node === 'object' ? (node as Messages)[part] : undefined,
      messages,
    )
}

function keysOf(messages: Messages, prefix = ''): string[] {
  return Object.entries(messages).flatMap(([name, value]) =>
    typeof value === 'object'
      ? keysOf(value, `${prefix}${name}.`)
      : [`${prefix}${name}`],
  )
}

for (const language of ['de', 'en']) {
  const messages = load(language)

  test(`${language}: every error reason has a text`, () => {
    for (const reason of MARKETPLACE_REASONS)
      assert.equal(
        typeof lookup(messages, reasonKey(reason)),
        'string',
        reasonKey(reason),
      )
    assert.equal(
      typeof lookup(messages, 'marketplace.errors.generic'),
      'string',
    )
  })

  test(`${language}: every source error kind names the marketplace`, () => {
    for (const kind of SOURCE_ERROR_KINDS) {
      const text = lookup(messages, sourceErrorKey(kind))
      assert.equal(typeof text, 'string', sourceErrorKey(kind))
      assert.match(String(text), /\{name\}/, sourceErrorKey(kind))
    }
  })

  test(`${language}: every reason a version is blocked has a text`, () => {
    for (const reason of BLOCKED_REASONS)
      assert.equal(
        typeof lookup(messages, `marketplace.blocked.${reason}`),
        'string',
        reason,
      )
  })

  test(`${language}: no text contains angle brackets, which vue-i18n refuses`, () => {
    for (const key of keysOf(
      (messages as { marketplace: Messages }).marketplace,
      'marketplace.',
    ))
      assert.doesNotMatch(String(lookup(messages, key)), /[<>]/, key)
  })
}

for (const language of ['de', 'en']) {
  test(`${language}: every source error kind has a cause in the test result of the form`, () => {
    const messages = load(language)
    for (const kind of SOURCE_ERROR_KINDS)
      assert.equal(
        typeof lookup(
          messages,
          `settings.extensions.marketplaces.testError.${kind}`,
        ),
        'string',
        kind,
      )
  })
}

test('both languages have the same keys under settings.extensions.marketplaces', () => {
  const [de, en] = ['de', 'en'].map((language) =>
    keysOf(
      (
        load(language) as {
          settings: { extensions: { marketplaces: Messages } }
        }
      ).settings.extensions.marketplaces,
    ).sort(),
  )
  assert.deepEqual(de, en)
})

test('both languages have the same keys under marketplace', () => {
  const [de, en] = ['de', 'en'].map((language) =>
    keysOf((load(language) as { marketplace: Messages }).marketplace).sort(),
  )
  assert.deepEqual(de, en)
})
