// Relative `.ts` imports, so the Node check script (`scripts/check-wm-header.ts`) loads it.
import type { WmHeaderDescription } from '../types/bindings/WmHeaderDescription.ts'
import type { WmHeaderEvent } from '../types/bindings/WmHeaderEvent.ts'
import type { TabRuntime } from '../lib/wm/types.ts'

/**
 * A tab's header description and the handler of its events (spec 090-app-header-bar, FR-007,
 * FR-012, data-model.md). The functions take the store's `tabRuntime` map, as `wmLayoutHandlers.ts`
 * does, so the window manager store itself stays small. A tab that has closed has no runtime, so
 * every call for it is a no-op; the description of a tab in the background is kept.
 */
export function createWmTabHeader(tabRuntime: Map<string, TabRuntime>) {
  /** Replaces the tab's description whole; `null` clears it (the header then shows the title). */
  function setTabHeader(tabId: string, header: WmHeaderDescription | null) {
    const runtime = tabRuntime.get(tabId)
    if (runtime) runtime.header = header
  }

  /** The tab's description, `null` when it has none or has closed. */
  function headerOf(tabId: string): WmHeaderDescription | null {
    return tabRuntime.get(tabId)?.header ?? null
  }

  /** Registers the handler of the tab's header events (a new one replaces the previous); the
   * returned function removes it, unless a newer one has replaced it since. */
  function registerTabHeaderHandler(
    tabId: string,
    handler: (event: WmHeaderEvent) => void,
  ): () => void {
    const runtime = tabRuntime.get(tabId)
    if (runtime) runtime.headerHandler = handler
    return () => {
      const current = tabRuntime.get(tabId)
      if (current?.headerHandler === handler) current.headerHandler = null
    }
  }

  /** Hands an event of a header control to the tab's handler. */
  function sendHeaderEvent(tabId: string, event: WmHeaderEvent) {
    tabRuntime.get(tabId)?.headerHandler?.(event)
  }

  /** The header fields that may hold typed text the app has not been told about yet: each takes
   * a tab id and sends that tab's waiting input (one per window header). */
  const flushers = new Set<(tabId: string) => void>()

  /** Registers a header's flusher; the returned function removes it. */
  function registerHeaderFlusher(flusher: (tabId: string) => void): () => void {
    flushers.add(flusher)
    return () => {
      flushers.delete(flusher)
    }
  }

  /** Sends the text the user typed into a search field of the tab's header and the app has not
   * been told about yet (the field merges changes for 100 ms). It reaches the tab's handler now,
   * while the handler still sees the route the text was typed on. */
  function flushTabHeader(tabId: string) {
    for (const flusher of [...flushers]) flusher(tabId)
  }

  return {
    setTabHeader,
    headerOf,
    registerTabHeaderHandler,
    sendHeaderEvent,
    registerHeaderFlusher,
    flushTabHeader,
  }
}

/**
 * Wraps a function that changes a tab's route (its first argument is the tab id): the tab's
 * waiting header search input is sent before the route changes, so it is not applied to the route
 * reached (Back, Forward, a push within the merge window of the field).
 */
export function flushingFirst<A extends [string, ...unknown[]], R>(
  flush: (tabId: string) => void,
  change: (...args: A) => R,
): (...args: A) => R {
  return (...args) => {
    flush(args[0])
    return change(...args)
  }
}
