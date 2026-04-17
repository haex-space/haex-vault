/**
 * Integration tests for extension broadcast routing.
 *
 * Every test runs against **real** `MessageChannel` instances. Dispatchers
 * call `entry.port.postMessage`; the paired port receives via a real
 * `message` listener (no `postMessage` spies). Attack scenarios prove that
 * unauthorised extensions observe nothing.
 *
 * Covered properties:
 *   - authorisation scoping (file readers, shell owner)
 *   - fail-closed defaults (empty / missing / unknown readers)
 *   - multi-instance fan-out (same extension, multiple iframes)
 *   - ready-ACK buffering (events before PORT_READY land in `buffer`)
 *   - payload-integrity (routing metadata does not leak into iframe payload)
 */

import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import {
  dispatchFileChangedBroadcast,
  dispatchShellEventBroadcast,
  type RoutableEntry,
  type RoutablePort,
} from '~/stores/extensions/broadcastRouting'

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

interface Fixture {
  extensionId: string
  channel: MessageChannel
  port: MessagePort // main-side (port1 equivalent)
  remoteReceived: Array<unknown>
  entry: RoutableEntry<MessagePort>
}

/** Create a routable entry with a real MessageChannel; mark ready by default. */
const makeEntry = (extensionId: string, opts: { ready?: boolean } = {}): Fixture => {
  const channel = new MessageChannel()
  const remoteReceived: unknown[] = []
  channel.port2.addEventListener('message', (event: MessageEvent) => {
    remoteReceived.push(event.data)
  })
  channel.port2.start()

  return {
    extensionId,
    channel,
    port: channel.port1,
    remoteReceived,
    entry: {
      instance: { extension: { id: extensionId } },
      port: channel.port1,
      ready: opts.ready ?? true,
      buffer: [],
    },
  }
}

/**
 * Drain the event loop enough times for jsdom's MessageChannel to deliver
 * queued messages to the paired port's listener. A single setTimeout(0) is
 * insufficient on slower CI runners — message delivery hops through
 * worker-thread messaging and may span several macrotask ticks.
 */
const flush = async (): Promise<void> => {
  for (let i = 0; i < 10; i++) {
    await new Promise<void>((resolve) => setTimeout(resolve, 1))
  }
}

const cleanup: Fixture[] = []

beforeEach(() => {
  cleanup.length = 0
})

afterEach(() => {
  for (const fx of cleanup) {
    fx.channel.port1.close()
    fx.channel.port2.close()
  }
})

const track = (f: Fixture): Fixture => {
  cleanup.push(f)
  return f
}

const entriesOf = (fixtures: Fixture[]): RoutableEntry<MessagePort>[] =>
  fixtures.map((f) => f.entry)

// ---------------------------------------------------------------------------
// File-change broadcast — authorisation
// ---------------------------------------------------------------------------

describe('dispatchFileChangedBroadcast — authorisation', () => {
  it('delivers only to entries whose extension id is in readerExtensionIds', async () => {
    const a = track(makeEntry('ext-a'))
    const b = track(makeEntry('ext-b'))
    const c = track(makeEntry('ext-c'))

    dispatchFileChangedBroadcast(
      {
        ruleId: 'rule-1',
        changeType: 'modified',
        path: 'docs/note.md',
        readerExtensionIds: ['ext-a', 'ext-c'],
      },
      entriesOf([a, b, c]),
    )
    await flush()

    expect(a.remoteReceived.length).toBe(1)
    expect(c.remoteReceived.length).toBe(1)
    // Security invariant: ext-b is NOT authorised; must not observe anything.
    expect(b.remoteReceived.length).toBe(0)
  })

  it('delivers to all entries of the same extension (multi-instance)', async () => {
    const a1 = track(makeEntry('ext-a'))
    const a2 = track(makeEntry('ext-a'))
    const b = track(makeEntry('ext-b'))

    dispatchFileChangedBroadcast(
      { ruleId: 'r', changeType: 'created', path: 'x', readerExtensionIds: ['ext-a'] },
      entriesOf([a1, a2, b]),
    )
    await flush()

    expect(a1.remoteReceived.length).toBe(1)
    expect(a2.remoteReceived.length).toBe(1)
    expect(b.remoteReceived.length).toBe(0)
  })
})

// ---------------------------------------------------------------------------
// File-change broadcast — fail-closed defaults
// ---------------------------------------------------------------------------

describe('dispatchFileChangedBroadcast — fail-closed defaults', () => {
  it('does not broadcast when readerExtensionIds is undefined', async () => {
    const a = track(makeEntry('ext-a'))

    const result = dispatchFileChangedBroadcast(
      { ruleId: 'r', changeType: 'modified', path: 'p' },
      entriesOf([a]),
    )
    await flush()

    expect(result.postedTo).toEqual([])
    expect(result.buffered).toEqual([])
    expect(result.message).toBeNull()
    expect(a.remoteReceived.length).toBe(0)
  })

  it('does not broadcast when readerExtensionIds is empty', async () => {
    const a = track(makeEntry('ext-a'))

    dispatchFileChangedBroadcast(
      { ruleId: 'r', changeType: 'modified', path: 'p', readerExtensionIds: [] },
      entriesOf([a]),
    )
    await flush()

    expect(a.remoteReceived.length).toBe(0)
  })
})

// ---------------------------------------------------------------------------
// File-change broadcast — hostile input
// ---------------------------------------------------------------------------

describe('dispatchFileChangedBroadcast — hostile / malformed input', () => {
  it('ignores unknown extension IDs without crashing', async () => {
    const a = track(makeEntry('ext-a'))

    const result = dispatchFileChangedBroadcast(
      { ruleId: 'r', changeType: 'modified', path: 'p', readerExtensionIds: ['ext-ghost'] },
      entriesOf([a]),
    )
    await flush()

    expect(result.postedTo).toEqual([])
    expect(a.remoteReceived.length).toBe(0)
  })

  it('delivers to known readers even when unknown IDs are mixed in', async () => {
    const a = track(makeEntry('ext-a'))
    const b = track(makeEntry('ext-b'))

    dispatchFileChangedBroadcast(
      { ruleId: 'r', changeType: 'modified', path: 'p', readerExtensionIds: ['ext-a', 'ext-ghost'] },
      entriesOf([a, b]),
    )
    await flush()

    expect(a.remoteReceived.length).toBe(1)
    expect(b.remoteReceived.length).toBe(0)
  })
})

// ---------------------------------------------------------------------------
// File-change broadcast — buffering
// ---------------------------------------------------------------------------

describe('dispatchFileChangedBroadcast — pre-ready buffering', () => {
  it('buffers messages for entries whose ready flag is false', async () => {
    const a = track(makeEntry('ext-a', { ready: false }))

    const result = dispatchFileChangedBroadcast(
      { ruleId: 'r', changeType: 'modified', path: 'p', readerExtensionIds: ['ext-a'] },
      entriesOf([a]),
    )
    await flush()

    expect(result.postedTo).toEqual([])
    expect(result.buffered).toHaveLength(1)
    expect(a.entry.buffer).toHaveLength(1)
    // The port did NOT deliver yet — buffered events must not hit the remote
    // side until the store explicitly flushes after PORT_READY.
    expect(a.remoteReceived.length).toBe(0)
  })

  it('delivers subsequent messages once ready flips true', async () => {
    const a = track(makeEntry('ext-a', { ready: false }))

    dispatchFileChangedBroadcast(
      { ruleId: 'r', changeType: 'modified', path: 'p', readerExtensionIds: ['ext-a'] },
      entriesOf([a]),
    )

    // Simulate the store's flush-on-ACK behaviour.
    a.entry.ready = true
    for (const buffered of a.entry.buffer) a.entry.port.postMessage(buffered)
    a.entry.buffer.length = 0

    await flush()
    expect(a.remoteReceived.length).toBe(1)
  })
})

// ---------------------------------------------------------------------------
// File-change broadcast — payload integrity
// ---------------------------------------------------------------------------

describe('dispatchFileChangedBroadcast — payload integrity', () => {
  it('does NOT forward readerExtensionIds to the extension (meta-leak guard)', async () => {
    const a = track(makeEntry('ext-a'))

    dispatchFileChangedBroadcast(
      {
        ruleId: 'r',
        changeType: 'modified',
        path: 'secret/path.txt',
        readerExtensionIds: ['ext-a', 'ext-b', 'ext-c'],
      },
      entriesOf([a]),
    )
    await flush()

    const payload = a.remoteReceived[0] as Record<string, unknown>
    expect(payload).not.toHaveProperty('readerExtensionIds')
    expect(payload.ruleId).toBe('r')
    expect(payload.changeType).toBe('modified')
    expect(payload.path).toBe('secret/path.txt')
  })
})

// ---------------------------------------------------------------------------
// Shell event broadcast — owner scoping
// ---------------------------------------------------------------------------

describe('dispatchShellEventBroadcast — owner scoping', () => {
  it('delivers only to the owning extension', async () => {
    const owner = track(makeEntry('ext-owner'))
    const other = track(makeEntry('ext-other'))

    dispatchShellEventBroadcast(
      'shell:output',
      { extensionId: 'ext-owner', sessionId: 's1', data: 'some-secret-stdout' },
      entriesOf([owner, other]),
    )
    await flush()

    expect(owner.remoteReceived.length).toBe(1)
    // Security invariant: stdout must not reach any other extension.
    expect(other.remoteReceived.length).toBe(0)
  })

  it('delivers to all iframes of the owner', async () => {
    const owner1 = track(makeEntry('ext-owner'))
    const owner2 = track(makeEntry('ext-owner'))
    const other = track(makeEntry('ext-other'))

    dispatchShellEventBroadcast(
      'shell:output',
      { extensionId: 'ext-owner', sessionId: 's1', data: 'x' },
      entriesOf([owner1, owner2, other]),
    )
    await flush()

    expect(owner1.remoteReceived.length).toBe(1)
    expect(owner2.remoteReceived.length).toBe(1)
    expect(other.remoteReceived.length).toBe(0)
  })
})

// ---------------------------------------------------------------------------
// Shell event broadcast — fail-closed
// ---------------------------------------------------------------------------

describe('dispatchShellEventBroadcast — fail-closed', () => {
  it('does not broadcast when extensionId is empty', async () => {
    const a = track(makeEntry('ext-a'))

    const result = dispatchShellEventBroadcast(
      'shell:output',
      { extensionId: '', sessionId: 's', data: 'x' },
      entriesOf([a]),
    )
    await flush()

    expect(result.postedTo).toEqual([])
    expect(a.remoteReceived.length).toBe(0)
  })

  it('does not broadcast when target extensionId has no registered entry', async () => {
    const a = track(makeEntry('ext-a'))

    const result = dispatchShellEventBroadcast(
      'shell:output',
      { extensionId: 'ext-ghost', sessionId: 's', data: 'x' },
      entriesOf([a]),
    )
    await flush()

    expect(result.postedTo).toEqual([])
    expect(a.remoteReceived.length).toBe(0)
  })
})

// ---------------------------------------------------------------------------
// Shell event broadcast — payload integrity
// ---------------------------------------------------------------------------

describe('dispatchShellEventBroadcast — payload integrity', () => {
  it('strips extensionId from the forwarded message (routing-only metadata)', async () => {
    const owner = track(makeEntry('ext-owner'))

    dispatchShellEventBroadcast(
      'shell:output',
      { extensionId: 'ext-owner', sessionId: 's1', data: 'abc' },
      entriesOf([owner]),
    )
    await flush()

    const payload = owner.remoteReceived[0] as Record<string, unknown>
    expect(payload).not.toHaveProperty('extensionId')
    expect(payload.sessionId).toBe('s1')
    expect(payload.data).toBe('abc')
    expect(payload.type).toBe('shell:output')
  })

  it('preserves exit events with exitCode field', async () => {
    const owner = track(makeEntry('ext-owner'))

    dispatchShellEventBroadcast(
      'shell:exit',
      { extensionId: 'ext-owner', sessionId: 's1', exitCode: 137 },
      entriesOf([owner]),
    )
    await flush()

    const payload = owner.remoteReceived[0] as Record<string, unknown>
    expect(payload.type).toBe('shell:exit')
    expect(payload.exitCode).toBe(137)
    expect(payload).not.toHaveProperty('extensionId')
  })

  it('supports minimal port surface (any object with postMessage)', async () => {
    // Demonstrates that broadcastRouting does not require a real MessagePort —
    // stores that want to unit-test delivery in non-jsdom environments can
    // pass any `RoutablePort`.
    const calls: unknown[] = []
    const port: RoutablePort = {
      postMessage: (msg) => calls.push(msg),
    }
    const entry: RoutableEntry = {
      instance: { extension: { id: 'ext-a' } },
      port,
      ready: true,
      buffer: [],
    }

    dispatchFileChangedBroadcast(
      { ruleId: 'r', changeType: 'modified', path: 'p', readerExtensionIds: ['ext-a'] },
      [entry],
    )
    expect(calls).toHaveLength(1)
  })
})
