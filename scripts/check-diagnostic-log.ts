import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  allows,
  createBucket,
  createLogState,
  MAX_TEXT,
  messageText,
} from '../src/lib/diagnosticLog.ts'

test('several arguments are joined with spaces', () => {
  assert.equal(
    messageText(['sync failed', 42, true, null]),
    'sync failed 42 true null',
  )
})

test('an Error gives its message and its stack', () => {
  const error = new Error('probe failed')
  const text = messageText(['while syncing', error])
  assert.ok(text.startsWith('while syncing '))
  assert.ok(text.includes('Error: probe failed'))
  assert.ok(text.includes('check-diagnostic-log'), 'the stack names this file')
})

test('the message of an Error is there even when its stack lacks it', () => {
  // WebKit's stack holds the frames only, not the message.
  const error = new TypeError('no title')
  error.stack = 'run@http://localhost/app.js:1:1'
  const text = messageText([error])
  assert.ok(text.startsWith('TypeError: no title'))
  assert.ok(text.includes('run@http://localhost/app.js:1:1'))
})

test('an object or an array becomes [object] and none of its fields appear', () => {
  const entry = { title: 'Mail', password: 'hunter2', nested: { key: 'k' } }
  const text = messageText(['saving', entry, ['hunter2', 'k']])
  assert.equal(text, 'saving [object] [object]')
})

test('a structured backend error stays [object] and none of its fields appear', () => {
  const failed = {
    kind: 'InvalidInput',
    reason: 'no such file',
    path: '/secret',
  }
  assert.equal(messageText(['reading failed', failed]), 'reading failed [object]')
})

test('a field that is no text is not read', () => {
  assert.equal(
    messageText([{ kind: { password: 'hunter2' }, reason: 42 }]),
    '[object]',
  )
  assert.equal(messageText([['kind', 'hunter2']]), '[object]')
})

test('a function, a symbol and a missing value have a plain text', () => {
  assert.equal(
    messageText([() => 'secret', Symbol('s'), undefined]),
    '[function] Symbol(s) undefined',
  )
})

test('an object with a throwing toString does not break the text', () => {
  const hostile = {
    toString() {
      throw new Error('boom')
    },
  }
  assert.equal(messageText([hostile]), '[object]')
})

test('a long text is cut with a marker', () => {
  const text = messageText(['x'.repeat(MAX_TEXT * 2)])
  assert.ok(text.length < MAX_TEXT + 64)
  assert.ok(text.startsWith('x'.repeat(MAX_TEXT)))
  assert.match(text, /cut/)
})

test('a text at the limit stays whole', () => {
  const whole = 'y'.repeat(MAX_TEXT)
  assert.equal(messageText([whole]), whole)
})

test('the bucket lets a burst of 200 through and then 100 a second', () => {
  let now = 0
  const bucket = createBucket(() => now)
  let passed = 0
  for (let i = 0; i < 300; i += 1) if (bucket.take()) passed += 1
  assert.equal(passed, 200)

  now = 1000
  passed = 0
  for (let i = 0; i < 300; i += 1) if (bucket.take()) passed += 1
  assert.equal(passed, 100)
})

test('the bucket does not refill beyond the burst', () => {
  let now = 0
  const bucket = createBucket(() => now)
  now = 60_000
  let passed = 0
  for (let i = 0; i < 300; i += 1) if (bucket.take()) passed += 1
  assert.equal(passed, 200)
})

test('the bucket reports its dropped count once a second', () => {
  let now = 0
  const bucket = createBucket(() => now)
  for (let i = 0; i < 250; i += 1) bucket.take()
  now = 500
  assert.equal(bucket.report(), 0, 'not yet a second')
  now = 1000
  assert.equal(bucket.report(), 50)
  assert.equal(bucket.report(), 0, 'reported once')
})

test('nothing is reported when nothing was dropped', () => {
  let now = 0
  const bucket = createBucket(() => now)
  bucket.take()
  now = 5000
  assert.equal(bucket.report(), 0)
})

test('info and debug are dropped in Standard and kept in Ausführlich', () => {
  for (const level of ['error', 'warn'] as const) {
    assert.equal(allows(level, false), true)
    assert.equal(allows(level, true), true)
  }
  for (const level of ['info', 'debug'] as const) {
    assert.equal(allows(level, false), false)
    assert.equal(allows(level, true), true)
  }
})

const status = (level: 'standard' | 'verbose', sessionId: string | null) => ({
  level,
  untilMs: null,
  writable: true,
  sessionId,
})

test('the state starts at Standard without a session', () => {
  const state = createLogState()
  assert.deepEqual(state.current(), { verbose: false, sessionId: null })
})

test('an event sets the level and the session', () => {
  const state = createLogState()
  state.onEvent(status('verbose', 'a'))
  assert.deepEqual(state.current(), { verbose: true, sessionId: 'a' })
})

test('the startup answer applies when no event came before it', () => {
  const state = createLogState()
  state.onStartup(status('verbose', 'a'))
  assert.deepEqual(state.current(), { verbose: true, sessionId: 'a' })
})

test('a startup answer after an event does not overwrite the event', () => {
  const state = createLogState()
  state.onEvent(status('verbose', 'new'))
  state.onStartup(status('standard', 'old'))
  assert.deepEqual(state.current(), { verbose: true, sessionId: 'new' })
})

test('a later event still wins over the startup answer', () => {
  const state = createLogState()
  state.onStartup(status('standard', 'a'))
  state.onEvent(status('verbose', 'b'))
  assert.deepEqual(state.current(), { verbose: true, sessionId: 'b' })
})

test('closing the vault drops the session', () => {
  const state = createLogState()
  state.onEvent(status('verbose', 'a'))
  state.onEvent(status('standard', null))
  assert.deepEqual(state.current(), { verbose: false, sessionId: null })
})
