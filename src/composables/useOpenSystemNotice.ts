import { shallowRef } from 'vue'
import type { OpenSystemOutcome } from '@bindings/OpenSystemOutcome'

/**
 * The course of "Mit System-App öffnen" for a file that has to be decrypted first (spec 048
 * FR-028, spec 058 FR-010, spec 072 US3), the same in the file browser, the password manager and the
 * chat: the first try answers `needsNotice` until the user confirmed the notice about the decrypted
 * copy (or chose "Nicht mehr fragen" on this device); `noticing` opens it, and confirming tries again
 * with `confirmed` and that choice. `onError` shows any other failure. What is opened (a path, an
 * attachment) and how the try is made is the caller's `run`.
 */
export function useOpenSystemNotice<T>(
  run: (
    item: T,
    confirmed: boolean,
    dontAskAgain: boolean,
  ) => Promise<OpenSystemOutcome>,
  onError: (cause: unknown) => void,
) {
  /** The item waiting for the notice. */
  const pending = shallowRef<{ item: T } | null>(null)
  /** The notice is open. */
  const noticing = ref(false)

  async function openAsync(item: T): Promise<void> {
    try {
      if ((await run(item, false, false)) !== 'needsNotice') return
    } catch (cause) {
      onError(cause)
      return
    }
    pending.value = { item }
    noticing.value = true
  }

  async function confirmAsync(dontAskAgain: boolean): Promise<void> {
    const waiting = pending.value
    pending.value = null
    if (!waiting) return
    await run(waiting.item, true, dontAskAgain).catch(onError)
  }

  return { noticing, openAsync, confirmAsync }
}
