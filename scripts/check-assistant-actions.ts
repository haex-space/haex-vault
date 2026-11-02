// Spec 066 T050: the voice assistant's actions are guardrails no agent can call, `assistant.talk`
// has its default keys without clashing with another action, every action has a title in German
// and English, and a global handler.
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { test } from 'node:test'

import { ASSISTANT_ACTIONS } from '../src/lib/actions/assistantActions.ts'
import { ALL_ACTIONS } from '../src/lib/actions/catalog.ts'
import { resolveChord } from '../src/lib/wm/keybindings.ts'

const root = resolve(import.meta.dirname, '..')
const read = (path: string) => readFileSync(resolve(root, path), 'utf8')

function localeText(locale: 'de' | 'en', key: string): unknown {
  let node: unknown = JSON.parse(read(`src/i18n/locales/${locale}.json`))
  for (const part of key.split('.')) {
    node = (node as Record<string, unknown> | undefined)?.[part]
  }
  return node
}

const IDS = [
  'assistant.talk',
  'assistant.mute.set',
  'assistant.conversation.end',
  'assistant.conversation.show',
]

test('the assistant offers exactly its four actions, all in the catalog', () => {
  assert.deepEqual(
    ASSISTANT_ACTIONS.map((a) => a.id),
    IDS,
  )
  for (const id of IDS) {
    assert.ok(
      ALL_ACTIONS.some((a) => a.id === id),
      `${id} is in the catalog`,
    )
  }
})

test('every assistant action is a guardrail no agent can call', () => {
  for (const action of ALL_ACTIONS.filter((a) =>
    a.id.startsWith('assistant.'),
  )) {
    assert.equal(action.scope, 'guardrails', action.id)
    assert.equal(action.agentCallable, false, action.id)
  }
})

test('assistant.talk has its default keys and no other action takes them', () => {
  const talk = ASSISTANT_ACTIONS.find((a) => a.id === 'assistant.talk')
  assert.deepEqual(talk?.defaultKeys, {
    default: ['Ctrl+Shift+Space'],
    mac: ['Shift+Meta+Space'],
  })
  for (const [platform, chord] of [
    ['default', 'Ctrl+Shift+Space'],
    ['mac', 'Shift+Meta+Space'],
  ] as const) {
    assert.equal(
      resolveChord(ALL_ACTIONS, chord, platform)?.id,
      'assistant.talk',
    )
    const others = ALL_ACTIONS.filter(
      (a) =>
        a.id !== 'assistant.talk' &&
        [
          ...(a.defaultKeys?.default ?? []),
          ...(a.defaultKeys?.mac ?? []),
        ].includes(chord),
    )
    assert.deepEqual(others, [], `${chord} on ${platform}`)
  }
})

test('every assistant action has a German and an English title', () => {
  for (const action of ASSISTANT_ACTIONS) {
    for (const locale of ['de', 'en'] as const) {
      const title = localeText(locale, action.titleKey)
      assert.ok(
        typeof title === 'string' && title.trim() !== '',
        `${locale}: ${action.id}`,
      )
    }
  }
})

test('every assistant action has a global handler', () => {
  const handlers = read('src/stores/assistantActionHandlers.ts')
  for (const id of IDS) {
    assert.ok(handlers.includes(`on('${id}'`), `handler for ${id}`)
  }
  assert.match(
    read('src/plugins/actions.client.ts'),
    /registerAssistantActionHandlers\(/,
  )
})
