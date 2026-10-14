// The extension frames whose SDK port is ready, per tab (spec 018, research R9): Rust asks holzi's
// window for a frame to call an extension's tools in and opens the MCP connection only once the
// frame can hear it. Pure: `composables/useExtensionFrame.ts` reports, `useExtensionHost.ts` waits,
// `scripts/check-extensions-agent-frames.ts` tests it.

export class ReadyFrames {
  private frames = new Map<string, string>()
  private waiting = new Map<string, Array<(frame: string) => void>>()

  /** The frame of `tabId` finished its port handshake. */
  ready(tabId: string, frame: string): void {
    this.frames.set(tabId, frame)
    const waiters = this.waiting.get(tabId) ?? []
    this.waiting.delete(tabId)
    for (const resolve of waiters) resolve(frame)
  }

  /** The frame of `tabId` closed or loads a new page. */
  gone(tabId: string): void {
    this.frames.delete(tabId)
  }

  frameOf(tabId: string): string | null {
    return this.frames.get(tabId) ?? null
  }

  /** The ready frame of `tabId`, waiting up to `timeoutMs`; `null` when none gets ready. */
  waitFor(tabId: string, timeoutMs: number): Promise<string | null> {
    const frame = this.frameOf(tabId)
    if (frame !== null) return Promise.resolve(frame)
    return new Promise((resolve) => {
      const waiters = this.waiting.get(tabId) ?? []
      const done = (found: string | null) => {
        clearTimeout(timer)
        resolve(found)
      }
      const timer = setTimeout(() => {
        const rest = (this.waiting.get(tabId) ?? []).filter((w) => w !== done)
        if (rest.length > 0) this.waiting.set(tabId, rest)
        else this.waiting.delete(tabId)
        done(null)
      }, timeoutMs)
      waiters.push(done)
      this.waiting.set(tabId, waiters)
    })
  }
}

/** The frames of holzi's window; one per document. */
export const readyFrames = new ReadyFrames()
