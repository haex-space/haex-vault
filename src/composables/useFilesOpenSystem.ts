import type { SourceRef } from '@bindings/SourceRef'
import { asFilesError } from '~/composables/useFiles'

/**
 * "Mit System-App öffnen" of the file browser (spec 044 FR-014). A file of an encrypted folder
 * answers `needsNotice` first (spec 048 FR-028): the notice opens, and confirming it opens the
 * file with `confirmed` and the "Nicht mehr fragen" choice (`useOpenSystemNotice`). `onError` shows
 * any other failure.
 */
export function useFilesOpenSystem(
  source: Ref<SourceRef>,
  onError: (cause: unknown) => void,
) {
  const { openSystemAsync } = useFiles()
  const notice = useOpenSystemNotice<{ path: string; app: string | null }>(
    async ({ path, app }, confirmed, dontAskAgain) => {
      try {
        await openSystemAsync(source.value, path, confirmed, dontAskAgain, app)
        return 'opened'
      } catch (cause) {
        if (asFilesError(cause)?.code === 'needsNotice') return 'needsNotice'
        throw cause
      }
    },
    onError,
  )

  return {
    noticing: notice.noticing,
    openAsync: (path: string, app: string | null = null) =>
      notice.openAsync({ path, app }),
    confirmAsync: notice.confirmAsync,
  }
}
