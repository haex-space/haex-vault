// Spec 066 T050 (operator 2026-11-02: one toggle, no further buttons or shortcut): the voice
// assistant has exactly one action, a guardrail no agent can call, without default keys, with a
// title in German and English and a global handler.
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { test } from 'node:test'

import { ASSISTANT_ACTIONS } from '../src/lib/actions/assistantActions.ts'
import { ALL_ACTIONS } from '../src/lib/actions/catalog.ts'

const root = resolve(import.meta.dirname, '..')
const read = (path: string) => readFileSync(resolve(root, path), 'utf8')

function localeText(locale: 'de' | 'en', key: string): unknown {
  let node: unknown = JSON.parse(read(`src/i18n/locales/${locale}.json`))
  for (const part of key.split('.')) {
    node = (node as Record<string, unknown> | undefined)?.[part]
  }
  return node
}

test('the assistant has exactly its toggle, in the catalog', () => {
  assert.deepEqual(
    ALL_ACTIONS.filter((a) => a.id.startsWith('assistant.')).map((a) => a.id),
    ['assistant.toggle'],
  )
  assert.deepEqual(
    ASSISTANT_ACTIONS.map((a) => a.id),
    ['assistant.toggle'],
  )
})

test('the toggle is a guardrail no agent can call, without a shortcut', () => {
  const [toggle] = ASSISTANT_ACTIONS
  assert.equal(toggle?.scope, 'guardrails')
  assert.equal(toggle?.agentCallable, false)
  assert.equal(toggle?.defaultKeys, undefined)
})

test('the toggle has a German and an English title and a global handler', () => {
  for (const action of ASSISTANT_ACTIONS) {
    for (const locale of ['de', 'en'] as const) {
      const title = localeText(locale, action.titleKey)
      assert.ok(
        typeof title === 'string' && title.trim() !== '',
        `${locale}: ${action.id}`,
      )
    }
  }
  assert.ok(
    read('src/stores/assistantActionHandlers.ts').includes(
      "registerGlobalActionHandler('assistant.toggle'",
    ),
  )
  assert.match(
    read('src/plugins/actions.client.ts'),
    /registerAssistantActionHandlers\(/,
  )
})
