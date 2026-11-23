// `pnpm check:agent-memory` (spec 097, T016): what "KI → Gedächtnis" does without the window: the
// tables it follows, the names of refused writes, the pointers of a page as links, and the actions
// behind its writes. Pure modules, loaded with Node type-stripping like check-settings.ts.
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'

import {
  AGENT_MEMORY_TABLES,
  memoryRefusal,
  withPointerLinks,
} from '../src/lib/agentMemory/pages.ts'
import { ALL_ACTIONS } from '../src/lib/actions/catalog.ts'
import { concerns } from '../src/lib/sync/vaultData.ts'

function localeTree(locale: 'de' | 'en'): unknown {
  const url = new URL(`../src/i18n/locales/${locale}.json`, import.meta.url)
  return JSON.parse(readFileSync(url, 'utf8'))
}

function hasKey(tree: unknown, key: string): boolean {
  let node: unknown = tree
  for (const part of key.split('.')) {
    node = (node as Record<string, unknown> | undefined)?.[part]
  }
  return typeof node === 'string'
}

test('the views follow the table of pages and nothing else', () => {
  assert.equal(
    concerns(AGENT_MEMORY_TABLES, ['chat_threads', 'haex_agent_memory_pages']),
    true,
  )
  assert.equal(concerns(AGENT_MEMORY_TABLES, ['chat_threads']), false)
  assert.equal(concerns(AGENT_MEMORY_TABLES, ['haex_agent_profiles']), false)
})

test('a refused write is named by its code, with the field of a refused form', () => {
  assert.deepEqual(
    memoryRefusal({ kind: 'InvalidInput', reason: 'too_long:title' }),
    { key: 'settings.agentMemory.refused.too_long', field: 'title' },
  )
  assert.deepEqual(memoryRefusal({ kind: 'InvalidInput', reason: 'secret' }), {
    key: 'settings.agentMemory.refused.secret',
  })
  assert.equal(
    memoryRefusal({ kind: 'InvalidInput', reason: 'something else' }),
    null,
  )
  assert.equal(memoryRefusal({ kind: 'NotFound', name: 'x' }), null)
  assert.equal(memoryRefusal(new Error('boom')), null)
})

test('every refusal and field has a text in both languages', () => {
  const codes = [
    'empty',
    'line_break',
    'too_long',
    'invisible_characters',
    'secret',
    'instruction',
    'title_taken',
    'memory_full',
    'page_too_long',
    'not_found',
  ]
  for (const locale of ['de', 'en'] as const) {
    const tree = localeTree(locale)
    for (const code of codes) {
      const refusal = memoryRefusal({ kind: 'InvalidInput', reason: code })
      assert.ok(refusal, code)
      assert.ok(hasKey(tree, refusal.key), `${locale} ${refusal.key}`)
    }
    for (const field of ['title', 'description', 'body']) {
      const key = `settings.agentMemory.fields.${field}`
      assert.ok(hasKey(tree, key), `${locale} ${key}`)
    }
  }
})

test('a pointer becomes a link to its page or is marked as missing', () => {
  const html = withPointerLinks(
    'Siehe [[Steuer 2026]], [[bernd krause|Bernd]] und [[Fehlt]].',
    [
      { title: 'Steuer 2026', id: 'p1' },
      { title: 'Bernd Krause', id: 'p2' },
      { title: 'Fehlt', id: null },
    ],
  )
  assert.equal(
    html,
    'Siehe <a href="#" data-memory-page="p1">Steuer 2026</a>, ' +
      '<a href="#" data-memory-page="p2">Bernd</a> und ' +
      '<span class="memory-missing" data-memory-missing="Fehlt">Fehlt</span>.',
  )
})

test('pointers in code and empty brackets stay as written, titles are escaped', () => {
  const text = 'Code `[[Kein]]`\n```\n[[Auch nicht]]\n```\n[[ ]] [[<b>x</b>]]'
  const html = withPointerLinks(text, [])
  assert.ok(html.includes('`[[Kein]]`'))
  assert.ok(html.includes('\n[[Auch nicht]]\n'))
  assert.ok(html.includes('[[ ]]'))
  assert.ok(
    html.includes(
      '<span class="memory-missing" data-memory-missing="&lt;b&gt;x&lt;/b&gt;">&lt;b&gt;x&lt;/b&gt;</span>',
    ),
  )
})

test('the writes of the page are guardrails no agent can call', () => {
  const ids = [
    'settings.agentMemory.add',
    'settings.agentMemory.change',
    'settings.agentMemory.remove',
  ]
  for (const id of ids) {
    const action = ALL_ACTIONS.find((candidate) => candidate.id === id)
    assert.ok(action, id)
    assert.equal(action.scope, 'guardrails', id)
    assert.equal(action.agentCallable, false, id)
  }
})
