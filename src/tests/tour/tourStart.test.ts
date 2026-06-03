import { describe, it, expect, beforeEach, vi } from 'vitest'
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

  it('does not start a second tour while one is already active', async () => {
    const store = useTourStore()
    store.start()
    expect(driver).toHaveBeenCalledTimes(1)

    await store.start() // resolves immediately, no new driver instance
    expect(driver).toHaveBeenCalledTimes(1)
  })
})
