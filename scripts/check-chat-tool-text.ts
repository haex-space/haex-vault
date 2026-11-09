// `pnpm check:chat-state` (spec 032): tool calls and results are shown indented, and anything
// that is not JSON is shown as it is; the user's own message keeps its text; a busy chat is
// named in words.
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { test } from 'node:test'

import { prettyToolText } from '../src/lib/chat/toolText.ts'

test('a JSON object or array is indented', () => {
  assert.equal(
    prettyToolText('{"a":1,"b":[2]}'),
    '{\n  "a": 1,\n  "b": [\n    2\n  ]\n}',
  )
  assert.equal(prettyToolText(' [1] '), '[\n  1\n]')
  assert.equal(prettyToolText('{}'), '{}')
})

test('markers, plain text and broken JSON stay as they are', () => {
  for (const text of ['denied_by_user', 'Tabs: 3', '{"a":', '', '"text"', '42'])
    assert.equal(prettyToolText(text), text)
})

test('a message of the user shows its text in the bubble', () => {
  const source = readFileSync(
    resolve(import.meta.dirname, '../src/components/chat/MessageList.vue'),
    'utf8',
  )
  // The bubble renders tool calls, then assistant/system as Markdown; everything else, the
  // user's own message, as plain text. Losing that branch leaves an empty dark bubble.
  assert.match(
    source,
    /v-else-if="m\.role === 'assistant' \|\| m\.role === 'system'"[\s\S]*?<!-- eslint-enable vue\/no-v-html -->\s*<template v-else>\{\{ m\.content \}\}<\/template>/,
  )
})

test('a chat that is still busy is named in words, the same text everywhere', () => {
  const read = (path: string) =>
    readFileSync(resolve(import.meta.dirname, '..', path), 'utf8')
  const reason = /OPERATION_BUSY_REASON =\s*'([^']+)'/.exec(
    read('src/composables/useErrorString.ts'),
  )?.[1]
  assert.ok(reason, 'useErrorString names the busy reason')
  assert.ok(
    read('src-tauri/src/chat/session.rs').includes(`reason: "${reason}"`),
    'the backend sends that text',
  )
  assert.ok(read('scripts/e2e/lib/flows.ts').includes(`'${reason}'`))
  assert.match(
    read('src/composables/useErrorString.ts'),
    /reason === OPERATION_BUSY_REASON\) return t\('errors\.operationBusy'\)/,
  )
  for (const locale of ['de', 'en']) {
    const errors = JSON.parse(read(`src/i18n/locales/${locale}.json`)).errors
    assert.equal(typeof errors.operationBusy, 'string', locale)
  }
  // A typed message waits for the chat first (operator 2026-11-03).
  assert.match(
    read('src-tauri/src/chat/commands.rs'),
    /pub async fn send_message[\s\S]*?wait_until_free\(SEND_WAIT_WHEN_BUSY\)[\s\S]*?start_turn\(/,
  )
})
