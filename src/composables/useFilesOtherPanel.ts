import { toast } from 'vue-sonner'
import type { Entry } from '@bindings/Entry'
import type { SourceRef } from '@bindings/SourceRef'
import type { PanelHandle } from '~/composables/useFilesSplit'
import {
  type DragPayload,
  type FilesClipboard,
  type PlaceLock,
  sameSource as isSameSource,
} from '~/lib/files/clipboard'

/**
 * Copying and moving the selection into the folder of the other panel (spec 062 FR-008 to FR-013):
 * F5 and F6, the menu and the action bar. It builds the clipboard `placeAsync` takes
 * (`useFilesManage`), so the refusals and the conflict question are those of a paste; the other
 * panel loads its folder again when the transfer is through.
 */
export function useFilesOtherPanel(options: {
  source: Ref<SourceRef>
  path: Ref<string | null>
  other: () => PanelHandle | null
  placeAsync: (
    clip: FilesClipboard,
    target: string,
    targetLock: PlaceLock,
    targetSource: SourceRef,
    onDone: () => void,
  ) => Promise<boolean>
  clearSelection: () => void
}) {
  const { t } = useI18n()

  /** A drag that came from the other panel is through: that panel loads its folder again, and a move
   * leaves nothing selected there (FR-014). */
  function droppedFromOther(payload: DragPayload, op: FilesClipboard['op']) {
    const other = options.other()
    const target = other?.target()
    if (
      !other ||
      target?.path !== payload.folder ||
      !isSameSource(target.source, payload.source)
    )
      return
    void other.reload()
    if (op === 'cut') other.clearSelection()
  }

  async function toOtherAsync(op: FilesClipboard['op'], entries: Entry[]) {
    const other = options.other()
    const target = other?.target()
    if (!other || !target?.path) {
      toast.error(t('files.error.noOtherPanel'))
      return
    }
    const folder = options.path.value
    if (!entries.length || folder === null) return
    // A move into the folder it comes from does nothing; say so (FR-013). A copy there asks what to
    // do with the names that exist, as a paste does.
    if (
      op === 'cut' &&
      target.path === folder &&
      isSameSource(options.source.value, target.source)
    ) {
      toast.error(t('files.error.sameFolder'))
      return
    }
    const started = await options.placeAsync(
      {
        op,
        source: options.source.value,
        folder,
        items: entries.map((entry) => ({ path: entry.path, kind: entry.kind })),
      },
      target.path,
      target.lock,
      target.source,
      () => void other.reload(),
    )
    if (started && op === 'cut') options.clearSelection()
  }
  return { toOtherAsync, droppedFromOther }
}
