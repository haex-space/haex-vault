// Part of `pnpm check:extensions` (spec 017, T042): the relay between an extension frame's port
// and Rust (src/lib/extensions/bridge.ts).
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  FrameEventQueue,
  MAX_HELD_EVENTS,
  encodeBytes,
  eventMessage,
  readRequest,
  type FrameEvent,
} from '../src/lib/extensions/bridge.ts'

test('requests are read with id, method and params; anything else is not a request', () => {
  assert.deepEqual(
    readRequest({
      id: 'r1',
      method: 'extension_database_query',
      params: { sql: 'x' },
      timestamp: 1,
    }),
    { id: 'r1', method: 'extension_database_query', params: { sql: 'x' } },
  )
  assert.deepEqual(readRequest({ id: 7, method: 'extension_context_get' }), {
    id: 7,
    method: 'extension_context_get',
    params: null,
  })
  for (const data of [
    { type: 'haexspace:port:ready' },
    { id: {}, method: 'x' },
    { id: 'r', method: '' },
    null,
    'x',
  ]) {
    assert.equal(readRequest(data), null)
  }
})

test('bytes become $bytes base64 at any depth, other values stay', () => {
  const bytes = new Uint8Array([0, 1, 2, 250, 255])
  assert.deepEqual(
    encodeBytes({
      sql: 'INSERT',
      params: [1, 'a', bytes, null, true],
      blob: bytes.buffer,
    }),
    {
      sql: 'INSERT',
      params: [1, 'a', { $bytes: 'AAEC+v8=' }, null, true],
      blob: { $bytes: 'AAEC+v8=' },
    },
  )
  const big = new Uint8Array(100_000).fill(65)
  const encoded = encodeBytes(big) as { $bytes: string }
  assert.equal(Buffer.from(encoded.$bytes, 'base64').length, 100_000)
  let deep: unknown = 1
  for (let i = 0; i < 100; i++) deep = [deep]
  assert.doesNotThrow(() => encodeBytes(deep))
})

test('events are held until the port is ready, then delivered in order, only for their frame', () => {
  const delivered: FrameEvent[] = []
  const queue = new FrameEventQueue('f1', (e) => delivered.push(e))
  const event = (frame: string, type: string): FrameEvent => ({
    frame,
    type,
    data: null,
    timestamp: 1,
  })
  queue.push(event('f1', 'a'))
  queue.push(event('f2', 'foreign'))
  queue.push(event('f1', 'b'))
  assert.equal(delivered.length, 0)
  queue.ready()
  assert.deepEqual(
    delivered.map((e) => e.type),
    ['a', 'b'],
  )
  queue.push(event('f1', 'c'))
  queue.reset()
  queue.push(event('f1', 'd'))
  assert.deepEqual(
    delivered.map((e) => e.type),
    ['a', 'b', 'c'],
  )
  queue.ready()
  assert.deepEqual(
    delivered.map((e) => e.type),
    ['a', 'b', 'c', 'd'],
  )
  assert.deepEqual(eventMessage(event('f1', 'x')), {
    type: 'x',
    data: null,
    timestamp: 1,
  })
})

test('a frame that is not ready holds a bounded number of events, dropping old shell output first', () => {
  const delivered: FrameEvent[] = []
  const queue = new FrameEventQueue('f1', (e) => delivered.push(e))
  const event = (type: string, data: unknown): FrameEvent => ({
    frame: 'f1',
    type,
    data,
    timestamp: 1,
  })
  queue.push(event('extension:permission-resolved', 'first'))
  for (let i = 0; i < MAX_HELD_EVENTS * 3; i++)
    queue.push(event('shell:output', i))
  queue.push(event('shell:exit', 'end'))
  queue.ready()
  assert.equal(delivered.length, MAX_HELD_EVENTS)
  assert.deepEqual(
    delivered[0],
    event('extension:permission-resolved', 'first'),
  )
  assert.deepEqual(delivered.at(-1), event('shell:exit', 'end'))
  const outputs = delivered.filter((e) => e.type === 'shell:output')
  assert.equal(outputs.length, MAX_HELD_EVENTS - 2)
  assert.equal(
    outputs.at(-1)?.data,
    MAX_HELD_EVENTS * 3 - 1,
    'the newest output stays',
  )

  // With nothing but other events held, the oldest one goes.
  const others: FrameEvent[] = []
  const busy = new FrameEventQueue('f1', (e) => others.push(e))
  const warn = console.warn
  console.warn = () => {}
  try {
    for (let i = 0; i <= MAX_HELD_EVENTS; i++) busy.push(event('x', i))
  } finally {
    console.warn = warn
  }
  busy.ready()
  assert.equal(others.length, MAX_HELD_EVENTS)
  assert.equal(others[0]?.data, 1)
})

test('a full queue drops fast peer messages first, and reports a lost reliable one once', () => {
  const delivered: FrameEvent[] = []
  const lost: string[] = []
  const queue = new FrameEventQueue(
    'f1',
    (e) => delivered.push(e),
    (channelId) => lost.push(channelId),
  )
  const event = (type: string, data: unknown): FrameEvent => ({
    frame: 'f1',
    type,
    data,
    timestamp: 1,
  })
  const message = (channelId: string, reliable: boolean, n: number) =>
    event('peers:message', { channelId, data: String(n), reliable })
  const warn = console.warn
  console.warn = () => {}
  try {
    queue.push(event('peers:channel-opened', 'first'))
    queue.push(message('c1', true, 0))
    queue.push(event('extension:permission-resolved', 'second'))
    for (let i = 0; i < MAX_HELD_EVENTS * 2; i++)
      queue.push(message('c1', false, i))
    assert.deepEqual(lost, [], 'fast messages go first, nothing is reported')
    queue.ready()
    assert.equal(delivered.length, MAX_HELD_EVENTS)
    assert.deepEqual(
      delivered.slice(0, 3).map((e) => e.type),
      [
        'peers:channel-opened',
        'peers:message',
        'extension:permission-resolved',
      ],
      'the other events keep their order',
    )
    const newest = delivered.at(-1)?.data as { data: string }
    assert.equal(newest.data, String(MAX_HELD_EVENTS * 2 - 1))
  } finally {
    console.warn = warn
  }

  // Nothing droppable is held: the oldest event goes, and a reliable message is reported once.
  const reported: string[] = []
  const busy = new FrameEventQueue(
    'f1',
    () => {},
    (id) => reported.push(id),
  )
  console.warn = () => {}
  try {
    for (let i = 0; i < MAX_HELD_EVENTS; i++)
      busy.push(message(i < 2 ? 'c2' : 'c3', true, i))
    assert.deepEqual(reported, [])
    busy.push(event('x', 1))
    busy.push(event('x', 2))
  } finally {
    console.warn = warn
  }
  assert.deepEqual(reported, ['c2'], 'two losses of one channel, one report')
})

test('a file change reaches the SDK flat, other events keep their data', () => {
  const change = {
    frame: 'f1',
    type: 'filesync:file-changed',
    data: { ruleId: 'r', changeType: 'modified', path: 'a.txt' },
    timestamp: 2,
  }
  assert.deepEqual(eventMessage(change), {
    ruleId: 'r',
    changeType: 'modified',
    path: 'a.txt',
    type: 'filesync:file-changed',
    data: change.data,
    timestamp: 2,
  })
  const tables = {
    frame: 'f1',
    type: 'haextension:sync:tables-updated',
    data: { tables: ['t'] },
    timestamp: 3,
  }
  assert.deepEqual(eventMessage(tables), {
    type: 'haextension:sync:tables-updated',
    data: { tables: ['t'] },
    timestamp: 3,
  })
})
