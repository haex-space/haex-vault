// `pnpm check:agent-memory` (spec 097, T016): what "KI → Gedächtnis" does without the window: the
// tables it follows, the names of refused writes, the pointers of a page as links, and the actions
// behind its writes. Pure modules, loaded with Node type-stripping like check-settings.ts.
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'

import {
  AGENT_MEMORY_TABLES,
  memoryNoticeKey,
  memoryWritePreview,
  isMemoryWrite,
  memoryRefusal,
  withPointerLinks,
} from '../src/lib/agentMemory/pages.ts'
import { ALL_ACTIONS } from '../src/lib/actions/catalog.ts'
import { rightsDraftOf, rightsOf } from '../src/lib/agents/rights.ts'
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
    'role_marker',
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

test('every kind of notice has its text in both languages (spec 097 FR-023)', () => {
  for (const locale of ['de', 'en'] as const) {
    const tree = localeTree(locale)
    for (const kind of ['added', 'changed', 'removed'] as const) {
      assert.ok(hasKey(tree, memoryNoticeKey(kind)), `${locale} ${kind}`)
    }
  }
})

test('the question before a write shows the stored page, not what the agent says (FR-021)', () => {
  assert.equal(isMemoryWrite('memory_add'), true)
  assert.equal(isMemoryWrite('memory_search'), false)
  const added = memoryWritePreview(
    'memory_add',
    { title: 'Mail-Stil', text: 'Kurz.', description: 'Wie ich schreibe' },
    null,
  )
  assert.deepEqual(added, {
    kind: 'add',
    title: 'Mail-Stil',
    missing: true,
    text: 'Kurz.',
    description: 'Wie ich schreibe',
  })
  const stored = { description: 'Steuerberater', body: 'Alt.' }
  const append = memoryWritePreview(
    'memory_add',
    { title: 'Bernd', text: 'Ergänzung.' },
    stored,
  )
  assert.deepEqual(append.stored, {
    description: 'Steuerberater',
    text: 'Alt.',
  })
  assert.equal(append.text, 'Ergänzung.')
  const rewritten = memoryWritePreview(
    'memory_write',
    { title: 'Bernd', description: 'Anwalt', text: 'Neu.', newTitle: 'B.' },
    stored,
  )
  assert.deepEqual(rewritten.stored, {
    description: 'Steuerberater',
    text: 'Alt.',
  })
  assert.equal(rewritten.text, 'Neu.')
  assert.equal(rewritten.newTitle, 'B.')
  const removed = memoryWritePreview(
    'memory_remove',
    { title: 'Bernd' },
    stored,
  )
  assert.deepEqual(removed, {
    kind: 'remove',
    title: 'Bernd',
    missing: false,
    stored: { description: 'Steuerberater', text: 'Alt.' },
  })
  assert.equal(
    memoryWritePreview('memory_remove', { title: 'X' }, null).missing,
    true,
  )
})

test('the preview names every part in both languages', () => {
  for (const locale of ['de', 'en'] as const) {
    const tree = localeTree(locale)
    for (const key of [
      'newPage',
      'notFound',
      'newTitle',
      'before',
      'after',
      'added',
      'removed',
    ]) {
      assert.ok(
        hasKey(tree, `settings.agentMemory.preview.${key}`),
        `${locale} ${key}`,
      )
    }
  }
})

test('saving another group of a profile keeps its memory level (spec 097 FR-006)', () => {
  for (const memory of ['read', 'readWrite'] as const) {
    const stored = { operate: { 'wm.read': 'read' as const }, memory }
    const draft = rightsDraftOf(stored)
    assert.equal(draft.memory, memory)
    assert.equal(rightsOf({ ...draft, commands: true }).memory, memory)
  }
  assert.equal(rightsDraftOf(null).memory, 'off')
  assert.equal(
    'memory' in rightsOf(rightsDraftOf({ operate: {} })),
    false,
    'off is sent as absent, which the backend reads as off',
  )
})
