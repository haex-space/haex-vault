/**
 * The camera scan of a phone (barcode plugin) as one shared state: whether it is running, and how
 * to end it from outside. The plugin shows the camera in a native view and gives the page no way
 * out of it, so the page scans in the plugin's windowed mode (the page lies transparent above the
 * camera) and `ScanOverlay` puts a close button there; the back gesture ends it too.
 */

/** The scan was ended by the person (close button, back gesture): not a failure, no message. */
export class ScanCancelledError extends Error {
  constructor() {
    super('scan cancelled')
    this.name = 'ScanCancelledError'
  }
}

export function isScanCancelled(cause: unknown): boolean {
  return cause instanceof ScanCancelledError
}

export function useScanSession() {
  const active = useState<boolean>('scan-session-active', () => false)
  const cancelRequested = useState<boolean>(
    'scan-session-cancel-requested',
    () => false,
  )

  /** Ends the running scan; its promise then rejects with `ScanCancelledError`. */
  async function cancelAsync(): Promise<void> {
    if (!active.value) return
    cancelRequested.value = true
    try {
      const { cancel } = await import('@tauri-apps/plugin-barcode-scanner')
      await cancel()
    } catch (cause) {
      // The camera did not end: the scan stays and the person can try again.
      cancelRequested.value = false
      throw cause
    }
  }

  /** Runs one scan: marks the session active and turns a cancel into `ScanCancelledError`. */
  async function runAsync<T>(scan: () => Promise<T>): Promise<T> {
    cancelRequested.value = false
    active.value = true
    try {
      return await scan()
    } catch (cause) {
      if (cancelRequested.value) throw new ScanCancelledError()
      throw cause
    } finally {
      active.value = false
      cancelRequested.value = false
    }
  }

  return { active, cancelAsync, runAsync }
}
