import type { Entry } from '@bindings/Entry'
import type { SourceRef } from '@bindings/SourceRef'
import {
  type DragPayload,
  dragPayload,
  ENTRIES_MIME,
  parseDragPayload,
  dropOp,
  sameSource,
} from '~/lib/files/clipboard'

/** The drag under way inside holzi: the browser hides its data while it hovers, and the cursor
 * should say move or copy by then. Shared by all panels, as the drag crosses them. */
let dragging: DragPayload | null = null

/**
 * Dragging entries inside holzi (spec 044 US3, spec 062 US2): a row starts the drag with the
 * selection (or itself), a folder row or the open folder takes it in. The payload carries the
 * source, so a drag from the other panel lands in this one; whether it moves or copies is decided by
 * the panel that takes it (`useFilesManage.onDrop`). A folder that takes no changes (`droppable`
 * false: holzi's own place, a share read only) refuses at the hover, so nothing is offered that
 * would only be refused after the drop.
 */
export function useFilesDrag(options: {
  source: () => SourceRef
  /** The open folder, where a drop on the empty area goes. */
  folder: () => string
  entries: () => readonly Entry[]
  selected: () => readonly string[]
  droppable: () => boolean
  drop: (
    payload: DragPayload,
    target: string,
    keys: { ctrl: boolean; alt: boolean },
  ) => void
}) {
  /** The folder a drag hovers over (a folder row, or the open folder). */
  const dropOver = ref<string | null>(null)
  const endDrag = () => {
    dragging = null
    dropOver.value = null
  }
  onMounted(() => window.addEventListener('dragend', endDrag))
  onBeforeUnmount(() => window.removeEventListener('dragend', endDrag))

  function onDragStart(entry: Entry, event: DragEvent) {
    const selected = options.selected()
    const paths = selected.includes(entry.path) ? selected : [entry.path]
    const items = options
      .entries()
      .filter((candidate) => paths.includes(candidate.path))
      .map((candidate) => ({ path: candidate.path, kind: candidate.kind }))
    dragging = { source: options.source(), folder: options.folder(), items }
    event.dataTransfer?.setData(ENTRIES_MIME, dragPayload(dragging))
    if (event.dataTransfer) event.dataTransfer.effectAllowed = 'copyMove'
  }

  /** Accepts a drag of holzi's entries over a folder row (`entry`) or the open folder (`null`). */
  function onDragOver(entry: Entry | null, event: DragEvent) {
    if (!event.dataTransfer?.types.includes(ENTRIES_MIME)) return
    if (entry && entry.kind !== 'dir') return
    event.stopPropagation()
    if (!options.droppable()) {
      // Not cancelled: the cursor shows the refusal and no drop follows.
      event.dataTransfer.dropEffect = 'none'
      dropOver.value = null
      return
    }
    event.preventDefault()
    const payload = parseDragPayload(event.dataTransfer.getData(ENTRIES_MIME))
    if (payload) {
      const op = dropOp(sameSource(payload.source, options.source()), {
        ctrl: event.ctrlKey,
        alt: event.altKey,
        mac: /Mac/.test(navigator.userAgent),
      })
      event.dataTransfer.dropEffect = op === 'copy' ? 'copy' : 'move'
    }
    dropOver.value = entry?.path ?? options.folder()
  }

  function onDrop(entry: Entry | null, event: DragEvent) {
    dropOver.value = null
    if (entry && entry.kind !== 'dir') return
    if (!options.droppable()) return
    const payload = parseDragPayload(event.dataTransfer?.getData(ENTRIES_MIME))
    dragging = null
    if (!payload) return
    event.preventDefault()
    event.stopPropagation()
    options.drop(payload, entry?.path ?? options.folder(), {
      ctrl: event.ctrlKey,
      alt: event.altKey,
    })
  }

  return { dropOver, onDragStart, onDragOver, onDrop }
}
