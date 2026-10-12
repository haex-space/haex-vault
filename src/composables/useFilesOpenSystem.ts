import type { SourceRef } from '@bindings/SourceRef'
import { asFilesError } from '~/composables/useFiles'

/**
 * "Mit System-App öffnen" of the file browser (spec 044 FR-014). A file of an encrypted folder
 * answers `needsNotice` first (spec 048 FR-028): the notice opens, and confirming it opens the
 * file with `confirmed` and the "Nicht mehr fragen" choice. `onError` shows any other failure.
 */
export function useFilesOpenSystem(
  source: Ref<SourceRef>,
  onError: (cause: unknown) => void,
) {
  const { openSystemAsync } = useFiles()
  /** The file waiting for the notice. */
  const pending = ref<string | null>(null)
  /** The notice is open. */
  const noticing = ref(false)

  async function openAsync(path: string): Promise<void> {
    try {
      await openSystemAsync(source.value, path)
    } catch (cause) {
      if (asFilesError(cause)?.code !== 'needsNotice') {
        onError(cause)
        return
      }
      pending.value = path
      noticing.value = true
    }
  }

  async function confirmAsync(dontAskAgain: boolean): Promise<void> {
    const path = pending.value
    pending.value = null
    if (!path) return
    await openSystemAsync(source.value, path, true, dontAskAgain).catch(onError)
  }

  return { noticing, openAsync, confirmAsync }
}
