import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'

import { driver } from 'driver.js'
import { useTourStore } from '~/stores/tour'

const driveMock = vi.fn()
const destroyMock = vi.fn()

vi.mock('driver.js', () => ({
  driver: vi.fn(() => ({
    drive: driveMock,
    destroy: destroyMock,
    moveNext: vi.fn(),
  })),
}))

beforeEach(() => {
  setActivePinia(createPinia())
  vi.clearAllMocks()
  // The store reads these Nuxt composables at setup time; stub them so the
  // store can be instantiated outside the Nuxt runtime.
  vi.stubGlobal('useNuxtApp', () => ({ $i18n: { locale: { value: 'en' } } }))
  vi.stubGlobal('useWindowManagerStore', () => ({ openWindowAsync: vi.fn() }))
  vi.stubGlobal('useLauncherStore', () => ({ isOpen: false }))
})

afterEach(() => {
  // vi.clearAllMocks() does not restore stubbed globals; without this they
  // leak into later test files (vitest.config.ts has no unstubGlobals: true).
  vi.unstubAllGlobals()
})

describe('tourStore.start (Promise coupling)', () => {
  it('resolves only after the tour completes', async () => {
    const store = useTourStore()
    let resolved = false
    void store.start().then(() => {
      resolved = true
    })

    // Flush microtasks AND a macrotask. An immediately-resolving start()
    // (the pre-Option-a behavior) would have resolved by now; the coupled
    // implementation must still be pending until complete() runs.
    await new Promise(resolve => setTimeout(resolve, 0))
    await Promise.resolve()
    expect(store.isActive).toBe(true)
    expect(driveMock).toHaveBeenCalledOnce()
    expect(resolved).toBe(false)

    store.complete()
    // A macrotask drains all pending microtasks, regardless of how many hops
    // Pinia's action wrapper adds before the returned promise settles.
    await new Promise(resolve => setTimeout(resolve, 0))
    expect(resolved).toBe(true)
    expect(store.isActive).toBe(false)
  })

  it('does not spin up a second driver while one is already active', async () => {
    const store = useTourStore()
    store.start()
    expect(driver).toHaveBeenCalledTimes(1)

    // Second call must NOT create a new driver instance.
    store.start()
    expect(driver).toHaveBeenCalledTimes(1)
  })

  it('lets concurrent start() callers share the same end-of-tour signal', async () => {
    // Regression: a previous implementation returned Promise.resolve() for the
    // second call, which let a `await tourStore.start()` callsite proceed to
    // post-tour work while the first tour was still running. Both awaiters must
    // resolve together when complete() fires.
    const store = useTourStore()
    let firstResolved = false
    let secondResolved = false

    void store.start().then(() => {
      firstResolved = true
    })
    void store.start().then(() => {
      secondResolved = true
    })

    await new Promise(resolve => setTimeout(resolve, 0))
    expect(firstResolved).toBe(false)
    expect(secondResolved).toBe(false)

    store.complete()
    await new Promise(resolve => setTimeout(resolve, 0))
    expect(firstResolved).toBe(true)
    expect(secondResolved).toBe(true)
  })
})
