import { describe, expect, it, vi } from 'vitest'
import { createOnceListener } from '@/lib/once-listener'

describe('createOnceListener', () => {
  it('calls setup exactly once for concurrent initAsync calls', async () => {
    const unlisten = vi.fn()
    let setupResolvers: Array<(value: typeof unlisten) => void> = []
    const setup = vi.fn(
      () => new Promise<typeof unlisten>((resolve) => {
        setupResolvers.push(resolve)
      }),
    )
    const listener = createOnceListener(setup)

    // Kick off 3 callers BEFORE the first listen() resolves — this is the
    // exact TOCTOU window we are fixing.
    const a = listener.initAsync()
    const b = listener.initAsync()
    const c = listener.initAsync()

    expect(setup).toHaveBeenCalledTimes(1)
    setupResolvers[0]!(unlisten)
    await Promise.all([a, b, c])

    expect(setup).toHaveBeenCalledTimes(1)
  })

  it('dispose calls every unlisten returned by setup', async () => {
    const u1 = vi.fn()
    const u2 = vi.fn()
    const listener = createOnceListener(async () => [u1, u2])

    await listener.initAsync()
    listener.dispose()

    expect(u1).toHaveBeenCalledOnce()
    expect(u2).toHaveBeenCalledOnce()
  })

  it('dispose followed by initAsync re-runs setup', async () => {
    const setup = vi.fn(async () => vi.fn())
    const listener = createOnceListener(setup)

    await listener.initAsync()
    listener.dispose()
    await listener.initAsync()

    expect(setup).toHaveBeenCalledTimes(2)
  })

  it('clears in-flight on setup rejection so retry is possible', async () => {
    const setup = vi
      .fn()
      .mockRejectedValueOnce(new Error('first fail'))
      .mockResolvedValueOnce(vi.fn())
    const listener = createOnceListener(setup)

    await expect(listener.initAsync()).rejects.toThrow('first fail')
    await listener.initAsync()

    expect(setup).toHaveBeenCalledTimes(2)
  })

  it('parallel callers share the same rejection', async () => {
    const setup = vi.fn().mockRejectedValue(new Error('boom'))
    const listener = createOnceListener(setup)

    const [a, b] = await Promise.allSettled([
      listener.initAsync(),
      listener.initAsync(),
    ])

    expect(a.status).toBe('rejected')
    expect(b.status).toBe('rejected')
    expect(setup).toHaveBeenCalledTimes(1)
  })

  it('initAsync after a resolved init is a no-op', async () => {
    const setup = vi.fn(async () => vi.fn())
    const listener = createOnceListener(setup)

    await listener.initAsync()
    await listener.initAsync()

    expect(setup).toHaveBeenCalledTimes(1)
  })

  it('dispose before init completes still unsubscribes once init resolves', async () => {
    // This guards against the "user called dispose() between initAsync()
    // kick-off and listen() resolution" case — the registered listener
    // must still be torn down.
    const unlisten = vi.fn()
    let resolveSetup!: (v: typeof unlisten) => void
    const setup = vi.fn(
      () => new Promise<typeof unlisten>((resolve) => {
        resolveSetup = resolve
      }),
    )
    const listener = createOnceListener(setup)

    const initPromise = listener.initAsync()
    // dispose() before setup resolves
    listener.dispose()
    resolveSetup(unlisten)
    await initPromise

    // The listener WAS registered (setup resolved) but the user wants it
    // gone. Calling dispose again should be safe; the inflight unlisten
    // should now be reachable.
    listener.dispose()
    expect(unlisten).toHaveBeenCalled()
  })
})
