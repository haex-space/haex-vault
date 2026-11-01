// Part of `pnpm check:mcp-actions` (spec 021 US6, FR-025): a delegate's call is asked in its chat
// while a chat shows that thread; every other call, and a delegate's call without an open chat,
// goes to the call dialog, the oldest first.
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  callFor,
  closeThread,
  openThread,
  outsideCalls,
  type OpenThreads,
  type RoutedCall,
} from '../src/lib/agents/callRouting.ts'

interface Call extends RoutedCall {
  requestId: string
}

function call(requestId: string, delegateThreadId?: string): Call {
  return {
    requestId,
    question: delegateThreadId ? { delegateThreadId } : {},
  }
}

const external = call('external')
const inA = call('a1', 'thread-a')
const laterInA = call('a2', 'thread-a')
const inB = call('b1', 'thread-b')
const calls = [external, inA, inB, laterInA]

test('without an open chat every call goes to the dialog', () => {
  assert.deepEqual(outsideCalls(calls, {}), calls)
})

test('an open chat takes the calls of its thread, the oldest first', () => {
  const open = openThread({}, 'thread-a')
  assert.deepEqual(outsideCalls(calls, open), [external, inB])
  assert.equal(callFor(calls, 'thread-a'), inA)
  assert.equal(callFor(calls.slice(2), 'thread-a'), laterInA)
  assert.equal(callFor(calls, 'thread-c'), undefined)
})

test('an external agent is never asked in a chat', () => {
  assert.equal(callFor([external], 'thread-a'), undefined)
})

test('a thread shown in two chats stays with them until the last one closes', () => {
  let open: OpenThreads = openThread(openThread({}, 'thread-a'), 'thread-a')
  open = closeThread(open, 'thread-a')
  assert.deepEqual(outsideCalls(calls, open), [external, inB])
  open = closeThread(open, 'thread-a')
  assert.deepEqual(open, {})
  assert.deepEqual(outsideCalls(calls, open), calls)
})

test('closing a thread no chat shows changes nothing', () => {
  const open = openThread({}, 'thread-b')
  assert.deepEqual(closeThread(open, 'thread-a'), open)
})
