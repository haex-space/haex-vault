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

  return { setTabHeader, headerOf, registerTabHeaderHandler, sendHeaderEvent }
}
