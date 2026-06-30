import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest'

let capturedListener:
  | ((event: { payload: { tables: string[] } }) => unknown)
  | null = null

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockImplementation((_eventName, callback) => {
    capturedListener = callback
    return Promise.resolve(() => Promise.resolve())
  }),
}))

vi.mock('@/stores/logging', () => ({
  createLogger: () => ({
    debug: vi.fn(),
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
  }),
}))

import {
  initSyncEventsAsync,
  registerStoreForTables,
  stopSyncEvents,
} from '@/stores/sync/syncEvents'

const flushPendingTimers = async (windowMs: number) => {
  await new Promise((r) => setTimeout(r, windowMs + 50))
}

describe('syncEvents — dispatch coalescing', () => {
  beforeEach(() => {
    capturedListener = null
  })

  afterEach(() => {
    stopSyncEvents()
  })

  it('coalesces a burst of identical events into one reload', async () => {
    const reloadFn = vi.fn().mockResolvedValue(undefined)
    registerStoreForTables(['haex_space_devices'], reloadFn)

    await initSyncEventsAsync()
    expect(capturedListener).not.toBeNull()

    for (let i = 0; i < 10; i++) {
      void capturedListener!({ payload: { tables: ['haex_space_devices'] } })
    }

    await flushPendingTimers(150)

    expect(reloadFn).toHaveBeenCalledTimes(1)
  })

  it('unions tables across coalesced events so every affected reload still runs once', async () => {
    const devicesReload = vi.fn().mockResolvedValue(undefined)
    const passwordsReload = vi.fn().mockResolvedValue(undefined)
    registerStoreForTables(['haex_space_devices'], devicesReload)
    registerStoreForTables(['haex_passwords_item_details'], passwordsReload)

    await initSyncEventsAsync()

    void capturedListener!({ payload: { tables: ['haex_space_devices'] } })
    void capturedListener!({
      payload: { tables: ['haex_passwords_item_details'] },
    })
    void capturedListener!({
      payload: {
        tables: ['haex_space_devices', 'haex_passwords_item_details'],
      },
    })

    await flushPendingTimers(150)

    expect(devicesReload).toHaveBeenCalledTimes(1)
    expect(passwordsReload).toHaveBeenCalledTimes(1)
  })

  it('does not block the event handler on the reload work', async () => {
    const release: { fn: (() => void) | null } = { fn: null }
    const reloadFn = vi.fn().mockImplementation(
      () =>
        new Promise<void>((resolve) => {
          release.fn = resolve
        }),
    )
    registerStoreForTables(['haex_space_devices'], reloadFn)

    await initSyncEventsAsync()

    const dispatched = capturedListener!({
      payload: { tables: ['haex_space_devices'] },
    })

    // Race the handler return against a short timeout: if it returns first,
    // the dispatch is non-blocking. If the timeout wins, the handler is
    // awaiting reloadFn (the regression we are guarding against).
    const winner = await Promise.race([
      Promise.resolve(dispatched).then(() => 'handler'),
      new Promise((r) => setTimeout(() => r('timeout'), 100)),
    ])

    expect(winner).toBe('handler')

    release.fn?.()
  })

  it('starts a fresh coalesce window after a flush, so a later burst still triggers reload', async () => {
    const reloadFn = vi.fn().mockResolvedValue(undefined)
    registerStoreForTables(['haex_space_devices'], reloadFn)

    await initSyncEventsAsync()

    void capturedListener!({ payload: { tables: ['haex_space_devices'] } })
    await flushPendingTimers(150)
    expect(reloadFn).toHaveBeenCalledTimes(1)

    void capturedListener!({ payload: { tables: ['haex_space_devices'] } })
    await flushPendingTimers(150)
    expect(reloadFn).toHaveBeenCalledTimes(2)
  })
})
