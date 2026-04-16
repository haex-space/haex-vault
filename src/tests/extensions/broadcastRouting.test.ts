/**
 * Integration tests for extension broadcast routing.
 *
 * Every test runs against **real** jsdom iframes. The dispatch helpers call
 * the actual `iframe.contentWindow.postMessage`; iframes register real
 * `message` event listeners; receipt is asserted via those listeners firing
 * (not by spying on `postMessage`). This exercises the full browser delivery
 * chain as it behaves at runtime.
 *
 * The tests focus on security invariants — what must NOT leak — rather than
 * re-verifying that `Set.has` filters. Attack scenarios included:
 *   - unauthorised extension fishing for events
 *   - empty / missing readers list (fail-closed)
 *   - unknown extension ID in readers list
 *   - shell output addressed to wrong owner
 *   - reader list metadata leaking into per-extension payload
 *   - iframe that lost its contentWindow (e.g. detached)
 *   - multiple instances of the same extension all receive
 *   - payload field tampering isolation (payload shape integrity)
 */

import { beforeEach, afterEach, describe, expect, it } from 'vitest'
import {
  dispatchFileChangedBroadcast,
  dispatchShellEventBroadcast,
  type RoutableInstance,
} from '~/stores/extensions/broadcastRouting'

// ---------------------------------------------------------------------------
// Harness: real iframe + captured messages via real message-event listeners
// ---------------------------------------------------------------------------

interface IframeFixture {
  iframe: HTMLIFrameElement
  instance: RoutableInstance
  received: Array<{ origin: string; data: unknown }>
}

const makeIframe = (extensionId: string): IframeFixture => {
  const iframe = document.createElement('iframe')
  document.body.appendChild(iframe)
  // jsdom gives us a real contentWindow with working postMessage/addEventListener.
  const received: Array<{ origin: string; data: unknown }> = []
  iframe.contentWindow!.addEventListener('message', (event) => {
    received.push({ origin: event.origin, data: event.data })
  })
  return {
    iframe,
    instance: { extension: { id: extensionId } },
    received,
  }
}

const registryFrom = (fixtures: IframeFixture[]): Iterable<readonly [HTMLIFrameElement, RoutableInstance]> =>
  fixtures.map((f) => [f.iframe, f.instance] as const)

// jsdom's postMessage fires listeners asynchronously via the event loop — we
// need to wait a microtask tick before asserting `received`.
const flushMessages = () => new Promise<void>((resolve) => setTimeout(resolve, 0))

const cleanup: IframeFixture[] = []

beforeEach(() => {
  cleanup.length = 0
})

afterEach(() => {
  for (const fixture of cleanup) {
    fixture.iframe.remove()
  }
})

const track = (fixture: IframeFixture): IframeFixture => {
  cleanup.push(fixture)
  return fixture
}

// ---------------------------------------------------------------------------
// File-change broadcast
// ---------------------------------------------------------------------------

describe('dispatchFileChangedBroadcast — authorisation', () => {
  it('delivers only to iframes whose extension id is in readerExtensionIds', async () => {
    const a = track(makeIframe('ext-a'))
    const b = track(makeIframe('ext-b'))
    const c = track(makeIframe('ext-c'))

    dispatchFileChangedBroadcast(
      {
        ruleId: 'rule-1',
        changeType: 'modified',
        path: 'docs/note.md',
        readerExtensionIds: ['ext-a', 'ext-c'],
      },
      registryFrom([a, b, c]),
    )
    await flushMessages()

    expect(a.received.length).toBe(1)
    expect(c.received.length).toBe(1)
    // Security invariant: ext-b is NOT authorised; must not observe the event.
    expect(b.received.length).toBe(0)
  })

  it('delivers to all iframes of the same extension (multi-instance)', async () => {
    const a1 = track(makeIframe('ext-a'))
    const a2 = track(makeIframe('ext-a'))
    const b = track(makeIframe('ext-b'))

    dispatchFileChangedBroadcast(
      {
        ruleId: 'r',
        changeType: 'created',
        path: 'x',
        readerExtensionIds: ['ext-a'],
      },
      registryFrom([a1, a2, b]),
    )
    await flushMessages()

    expect(a1.received.length).toBe(1)
    expect(a2.received.length).toBe(1)
    expect(b.received.length).toBe(0)
  })
})

describe('dispatchFileChangedBroadcast — fail-closed defaults', () => {
  it('does not broadcast when readerExtensionIds is undefined', async () => {
    const a = track(makeIframe('ext-a'))
    const b = track(makeIframe('ext-b'))

    const result = dispatchFileChangedBroadcast(
      { ruleId: 'r', changeType: 'modified', path: 'p' },
      registryFrom([a, b]),
    )
    await flushMessages()

    expect(result.postedTo).toEqual([])
    expect(result.message).toBeNull()
    expect(a.received.length).toBe(0)
    expect(b.received.length).toBe(0)
  })

  it('does not broadcast when readerExtensionIds is empty', async () => {
    const a = track(makeIframe('ext-a'))

    dispatchFileChangedBroadcast(
      { ruleId: 'r', changeType: 'modified', path: 'p', readerExtensionIds: [] },
      registryFrom([a]),
    )
    await flushMessages()

    expect(a.received.length).toBe(0)
  })
})

describe('dispatchFileChangedBroadcast — hostile input handling', () => {
  it('ignores unknown extension IDs in readerExtensionIds without crashing', async () => {
    const a = track(makeIframe('ext-a'))

    const result = dispatchFileChangedBroadcast(
      {
        ruleId: 'r',
        changeType: 'modified',
        path: 'p',
        readerExtensionIds: ['ext-nonexistent-1', 'ext-nonexistent-2'],
      },
      registryFrom([a]),
    )
    await flushMessages()

    expect(result.postedTo).toEqual([])
    expect(a.received.length).toBe(0)
  })

  it('delivers to known readers even when unknown IDs are mixed in', async () => {
    const a = track(makeIframe('ext-a'))
    const b = track(makeIframe('ext-b'))

    dispatchFileChangedBroadcast(
      {
        ruleId: 'r',
        changeType: 'modified',
        path: 'p',
        readerExtensionIds: ['ext-a', 'ext-ghost'],
      },
      registryFrom([a, b]),
    )
    await flushMessages()

    expect(a.received.length).toBe(1)
    expect(b.received.length).toBe(0)
  })

  it('skips iframes whose contentWindow is null (detached) without erroring', async () => {
    const a = track(makeIframe('ext-a'))
    // Forcibly simulate a detached iframe — the iframe element remains in the
    // registry but has no live window to post into.
    const detachedInstance: RoutableInstance = { extension: { id: 'ext-a' } }
    const detachedIframe = { contentWindow: null }

    const result = dispatchFileChangedBroadcast(
      { ruleId: 'r', changeType: 'modified', path: 'p', readerExtensionIds: ['ext-a'] },
      [
        [detachedIframe as unknown as HTMLIFrameElement, detachedInstance] as const,
        [a.iframe, a.instance] as const,
      ],
    )
    await flushMessages()

    // Only the live iframe is counted + received the message.
    expect(result.postedTo.length).toBe(1)
    expect(a.received.length).toBe(1)
  })
})

describe('dispatchFileChangedBroadcast — payload integrity', () => {
  it('does NOT forward readerExtensionIds to the iframe (prevents meta-leak)', async () => {
    const a = track(makeIframe('ext-a'))

    dispatchFileChangedBroadcast(
      {
        ruleId: 'r',
        changeType: 'modified',
        path: 'secret/path.txt',
        readerExtensionIds: ['ext-a', 'ext-b', 'ext-c'],
      },
      registryFrom([a]),
    )
    await flushMessages()

    expect(a.received.length).toBe(1)
    const payload = a.received[0]!.data as Record<string, unknown>
    // An iframe learning who *else* has read access is a privacy leak —
    // the reader list must stop at the broadcast boundary.
    expect(payload).not.toHaveProperty('readerExtensionIds')
    // Correct forwarded fields:
    expect(payload.ruleId).toBe('r')
    expect(payload.changeType).toBe('modified')
    expect(payload.path).toBe('secret/path.txt')
  })
})

// ---------------------------------------------------------------------------
// Shell event broadcast
// ---------------------------------------------------------------------------

describe('dispatchShellEventBroadcast — owner scoping', () => {
  it('delivers only to the owning extension, not to other extensions', async () => {
    const owner = track(makeIframe('ext-owner'))
    const other = track(makeIframe('ext-other'))

    dispatchShellEventBroadcast(
      'shell:output',
      { extensionId: 'ext-owner', sessionId: 's1', data: 'some-secret-stdout' },
      registryFrom([owner, other]),
    )
    await flushMessages()

    expect(owner.received.length).toBe(1)
    // Security invariant: stdout must not reach another extension even though
    // they share the same main-window origin (Android case).
    expect(other.received.length).toBe(0)
  })

  it('delivers to all iframes of the owner (multi-window extensions)', async () => {
    const owner1 = track(makeIframe('ext-owner'))
    const owner2 = track(makeIframe('ext-owner'))
    const other = track(makeIframe('ext-other'))

    dispatchShellEventBroadcast(
      'shell:output',
      { extensionId: 'ext-owner', sessionId: 's1', data: 'x' },
      registryFrom([owner1, owner2, other]),
    )
    await flushMessages()

    expect(owner1.received.length).toBe(1)
    expect(owner2.received.length).toBe(1)
    expect(other.received.length).toBe(0)
  })
})

describe('dispatchShellEventBroadcast — fail-closed', () => {
  it('does not broadcast when extensionId is missing / empty', async () => {
    const a = track(makeIframe('ext-a'))

    const result = dispatchShellEventBroadcast(
      'shell:output',
      { extensionId: '', sessionId: 's', data: 'x' },
      registryFrom([a]),
    )
    await flushMessages()

    expect(result.postedTo).toEqual([])
    expect(a.received.length).toBe(0)
  })

  it('does not broadcast when target extensionId has no registered iframes', async () => {
    const a = track(makeIframe('ext-a'))

    const result = dispatchShellEventBroadcast(
      'shell:output',
      { extensionId: 'ext-ghost', sessionId: 's', data: 'x' },
      registryFrom([a]),
    )
    await flushMessages()

    expect(result.postedTo).toEqual([])
    expect(a.received.length).toBe(0)
  })
})

describe('dispatchShellEventBroadcast — payload integrity', () => {
  it('strips extensionId from the forwarded message (routing-only metadata)', async () => {
    const owner = track(makeIframe('ext-owner'))

    dispatchShellEventBroadcast(
      'shell:output',
      { extensionId: 'ext-owner', sessionId: 's1', data: 'abc' },
      registryFrom([owner]),
    )
    await flushMessages()

    expect(owner.received.length).toBe(1)
    const payload = owner.received[0]!.data as Record<string, unknown>
    expect(payload).not.toHaveProperty('extensionId')
    expect(payload.sessionId).toBe('s1')
    expect(payload.data).toBe('abc')
    expect(payload.type).toBe('shell:output')
  })

  it('preserves exit events with exitCode field shape', async () => {
    const owner = track(makeIframe('ext-owner'))

    dispatchShellEventBroadcast(
      'shell:exit',
      { extensionId: 'ext-owner', sessionId: 's1', exitCode: 137 },
      registryFrom([owner]),
    )
    await flushMessages()

    const payload = owner.received[0]!.data as Record<string, unknown>
    expect(payload.type).toBe('shell:exit')
    expect(payload.exitCode).toBe(137)
    expect(payload).not.toHaveProperty('extensionId')
  })
})
