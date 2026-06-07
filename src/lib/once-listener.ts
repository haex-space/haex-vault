import type { UnlistenFn } from '@tauri-apps/api/event'

export interface OnceListener {
  initAsync: () => Promise<void>
  dispose: () => void
}

/**
 * Wrap a Tauri-`listen()` factory so concurrent `initAsync()` callers share
 * one in-flight Promise. Fixes the TOCTOU race where two callers each pass
 * an "already initialised?" guard before the first `listen()` resolves and
 * end up registering duplicate listeners.
 *
 * The factory may return a single `UnlistenFn` or an array; `dispose()`
 * calls each one. If the factory rejects, the in-flight slot clears so a
 * retry can succeed. If `dispose()` is called while setup is still
 * pending, the eventually-resolved listeners are immediately torn down
 * (detected by comparing the captured in-flight Promise to the current
 * module slot — `dispose()` nulls the slot, the captured reference still
 * resolves).
 */
// A single bad unlisten must not strand the others — Tauri's UnlistenFn
// is a remote call that can in principle reject.
function safeUnlisten(fns: readonly UnlistenFn[]): void {
  for (const u of fns) {
    try { u() } catch { /* swallow; one bad unlisten must not strand the others */ }
  }
}

export function createOnceListener(
  setup: () => Promise<UnlistenFn | UnlistenFn[]>,
): OnceListener {
  let inflight: Promise<UnlistenFn[]> | null = null
  let unlisteners: UnlistenFn[] | null = null

  return {
    async initAsync() {
      if (unlisteners) return
      if (!inflight) {
        inflight = (async () => {
          const result = await setup()
          return Array.isArray(result) ? result : [result]
        })()
        inflight.catch(() => {
          inflight = null
        })
      }
      const captured = inflight
      const resolved = await captured
      if (inflight !== captured) {
        safeUnlisten(resolved)
        return
      }
      unlisteners = resolved
    },
    dispose() {
      if (unlisteners) {
        safeUnlisten(unlisteners)
        unlisteners = null
      }
      inflight = null
    },
  }
}
