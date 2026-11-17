// The assistant's name (design 2026-11-16): one per vault, written and spoken. Texts that mean the
// assistant by name carry a token that becomes the written form; the page that sets the name has
// its texts in both languages.
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { test } from 'node:test'

import {
  ASSISTANT_TOKEN,
  DEFAULT_ASSISTANT_NAME,
  isValidName,
  withAssistantName,
} from '../src/lib/assistant/name.ts'

const root = resolve(import.meta.dirname, '..')
const locale = (code: 'de' | 'en') =>
  JSON.parse(
    readFileSync(resolve(root, `src/i18n/locales/${code}.json`), 'utf8'),
  ) as Record<string, unknown>

function strings(value: unknown, path = ''): [string, string][] {
  if (typeof value === 'string') return [[path, value]]
  if (value && typeof value === 'object')
    return Object.entries(value).flatMap(([key, inner]) =>
      strings(inner, path ? `${path}.${key}` : key),
    )
  return []
}

test('the default name is written Haex and said Hex', () => {
  assert.deepEqual(DEFAULT_ASSISTANT_NAME, { written: 'Haex', spoken: 'Hex' })
})

test('the token becomes the written name, everywhere it stands', () => {
  assert.equal(
    withAssistantName(
      `${ASSISTANT_TOKEN} hört zu, ${ASSISTANT_TOKEN}.`,
      'Luna',
    ),
    'Luna hört zu, Luna.',
  )
  assert.equal(withAssistantName('Ohne Namen', 'Luna'), 'Ohne Namen')
})

test('a name has one or two words of letters, digits and hyphens, up to 24 characters', () => {
  for (const ok of ['Luna', 'Anna-Lena', 'R2D2', 'Zoë', 'Anna  Lena', ' Luna '])
    assert.ok(isValidName(ok), ok)
  for (const bad of [
    '',
    '  ',
    'Eins Zwei Drei',
    'Lu_na',
    'Luna!',
    'Dr. Who',
    'a'.repeat(25),
  ])
    assert.ok(!isValidName(bad), bad)
  assert.ok(isValidName('a'.repeat(24)))
})

test('the texts that name the assistant carry the token in both languages, the same keys', () => {
  const keys = (code: 'de' | 'en') =>
    strings(locale(code))
      .filter(([, text]) => text.includes(ASSISTANT_TOKEN))
      .map(([key]) => key)
      .sort()
  assert.deepEqual(keys('de'), keys('en'))
  assert.ok(keys('de').length >= 20, 'the audit found the texts')
})

test('no text names the assistant the old way, in lower case', () => {
  // `haex` as a word of its own is the assistant; the tool, the extension and the product have
  // their own spellings (haex-pass, HaexVault, haextension, the quoted tool "haex").
  const pattern = /(?<![\w-])haex(?![\w-])/
  const allowed = new Set([
    'extensions.install.errors.legacy_signature_format',
    'settings.assistantPage.name.spokenHint',
    'settings.assistantPage.name.wakeHint',
  ])
  for (const code of ['de', 'en'] as const)
    for (const [key, text] of strings(locale(code)))
      if (!allowed.has(key) && pattern.test(text))
        assert.fail(`${code}: ${key} says "haex": ${text}`)
})

test('the page that sets the name has its texts in both languages', () => {
  for (const key of [
    'title',
    'written',
    'writtenHint',
    'spoken',
    'spokenHint',
    'wakeTitle',
    'wakeHint',
    'invalid',
    'save',
    'reset',
    'saveFailed',
  ])
    for (const code of ['de', 'en'] as const) {
      const found = strings(locale(code)).find(
        ([path]) => path === `settings.assistantPage.name.${key}`,
      )
      assert.ok(found && found[1] !== '', `${code}: ${key}`)
    }
})
