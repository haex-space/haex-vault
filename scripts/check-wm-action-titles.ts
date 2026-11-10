// Part of `pnpm check:wm-navigation`: every action of the shipped catalog (catalog.ts) has a
// German and an English title under its `titleKey`. A missing one shows the raw key in the
// command palette and the shortcut settings and makes vue-i18n warn.
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'

import { ALL_ACTIONS } from '../src/lib/actions/catalog.ts'

function messages(locale: 'de' | 'en'): unknown {
  const url = new URL(`../src/i18n/locales/${locale}.json`, import.meta.url)
  return JSON.parse(readFileSync(url, 'utf8'))
}

function textAt(root: unknown, key: string): unknown {
  let node = root
  for (const part of key.split('.')) {
    node = (node as Record<string, unknown> | undefined)?.[part]
  }
  return node
}

for (const locale of ['de', 'en'] as const) {
  test(`every catalog action has a ${locale} title`, () => {
    const root = messages(locale)
    const missing = ALL_ACTIONS.filter((action) => {
      const title = textAt(root, action.titleKey)
      return typeof title !== 'string' || title.trim() === ''
    }).map((action) => action.titleKey)
    assert.deepEqual(missing, [])
  })
}
