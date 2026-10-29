/**
 * The open folder of this device is gone (spec 044 edge case "Der offene Ordner verschwindet",
 * spec 064 research R10): the panel moves to the nearest folder above it that still exists, without
 * a new history entry, and says so. Up to the root nothing left, it goes back to its start.
 */
import { toast } from 'vue-sonner'
import type { FilesError } from '@bindings/FilesError'
import type { SourceRef } from '@bindings/SourceRef'
import type { FilesPanelContext } from '~/composables/useFilesSplit'
import { nearestExisting } from '~/lib/files/drives'

export function useFilesNearestFolder(
  context: FilesPanelContext,
  source: Ref<SourceRef>,
  path: Ref<string | null>,
  error: Ref<FilesError | null>,
) {
  const { t } = useI18n()
  const { statAsync } = useFiles()

  async function existsAsync(at: string): Promise<boolean> {
    try {
      return (await statAsync(source.value, at)).kind === 'dir'
    } catch {
      return false
    }
  }

  watch(error, async (now) => {
    const gone = path.value
    if (now?.code !== 'notFound' || source.value.kind !== 'device' || !gone)
      return
    const target = await nearestExisting(gone, existsAsync)
    // The panel moved on while the walk ran.
    if (path.value !== gone) return
    context.go({ source: source.value, path: target }, { replace: true })
    if (target) toast(t('files.drives.goneJump', { folder: gone, target }))
  })
}
