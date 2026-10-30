<script setup lang="ts">
/**
 * "Favoriten" in the sidebar of the file browser (spec 071): the folders the user pinned, as
 * shortcuts that never touch the folder. A folder is pinned from its context menu (`useFilesManage`)
 * or by dropping it on this section; a pin goes by its own context menu, or by dragging it onto the
 * strip that shows while it is dragged. The section stays when it is empty, as a place to drop on.
 */
import type { Sources } from '@bindings/Sources'
import type { SourceRef } from '@bindings/SourceRef'
import { currentDrag } from '~/composables/useFilesDrag'
import { ENTRIES_MIME, parseDragPayload } from '~/lib/files/clipboard'
import { pinMenu } from '~/lib/files/menus'
import {
  type FilesPin,
  formatPins,
  offerPin,
  pinLabel,
  pinnableSource,
} from '~/lib/files/pins'

const props = defineProps<{
  sources: Sources | null
  /** The open folder of the device, `null` while a storage or a list is open. */
  current: string | null
  currentStorage: string | null
  /** The open folder of the open place. */
  currentPath: string | null
}>()

const emit = defineEmits<{ pick: [pin: FilesPin] }>()

const { t } = useI18n()
const prefs = useFilesPrefs()

/** What a dragged pin carries, so no other drop target takes it for entries. */
const PIN_MIME = 'application/x-holzi-files-pin'

const known = computed(
  () => props.sources?.known.map((place) => place.path) ?? [],
)

function storageOf(pin: FilesPin) {
  const { source } = pin
  return source.kind === 'storage'
    ? props.sources?.storages.find((storage) => storage.id === source.storageId)
    : undefined
}

/** The pins to show: a pin of a storage that is gone stays stored but hidden, as it may return. */
const shown = computed(() =>
  prefs.pins.value.filter(
    (pin) => pin.source.kind === 'device' || storageOf(pin) !== undefined,
  ),
)

const labelOf = (pin: FilesPin) =>
  pinLabel(pin, storageOf(pin)?.name ?? t('files.sidebar.storages'))

function titleOf(pin: FilesPin): string {
  const storage = storageOf(pin)
  return storage ? `${storage.name}: ${pin.path}` : pin.path
}

function isCurrent(pin: FilesPin): boolean {
  return pin.source.kind === 'device'
    ? props.current === pin.path
    : props.currentStorage === pin.source.storageId &&
        props.currentPath === pin.path
}

// --- Dropping folders on the section ---

const over = ref(false)

/** A drag of holzi's entries that may hold a folder to pin. While it hovers the browser hides what
 * it carries, so the drag under way says; a drag from elsewhere is judged at the drop. */
function offered(event: DragEvent): boolean {
  if (!event.dataTransfer?.types.includes(ENTRIES_MIME)) return false
  const drag = currentDrag()
  return (
    !drag ||
    (pinnableSource(drag.source as SourceRef) &&
      drag.items.some((item) => item.kind === 'dir'))
  )
}

function onDragOver(event: DragEvent) {
  if (!offered(event)) return
  event.preventDefault()
  // A link: the folder itself is not moved or copied.
  if (event.dataTransfer) event.dataTransfer.dropEffect = 'link'
  over.value = true
}

function onDrop(event: DragEvent) {
  over.value = false
  const payload = parseDragPayload(event.dataTransfer?.getData(ENTRIES_MIME))
  if (!payload) return
  event.preventDefault()
  const dropped = payload.items
    .filter((item) => item.kind === 'dir')
    .map((item) => ({ source: payload.source as SourceRef, path: item.path }))
  prefs.pin(
    dropped.filter(
      (pin) => offerPin(prefs.pins.value, pin, known.value) === 'pin',
    ),
  )
}

// --- Dragging a pin out ---

const dragged = ref<FilesPin | null>(null)

function onPinDragStart(pin: FilesPin, event: DragEvent) {
  dragged.value = pin
  event.dataTransfer?.setData(PIN_MIME, formatPins([pin]))
  if (event.dataTransfer) event.dataTransfer.effectAllowed = 'move'
}

const overStrip = ref(false)

function onStripDragOver(event: DragEvent) {
  if (!dragged.value) return
  event.preventDefault()
  if (event.dataTransfer) event.dataTransfer.dropEffect = 'move'
  overStrip.value = true
}

function onStripDrop(event: DragEvent) {
  event.preventDefault()
  const pin = dragged.value
  endPinDrag()
  if (pin) prefs.unpin(pin)
}

function endPinDrag() {
  dragged.value = null
  overStrip.value = false
}
</script>

<template>
  <section
    :class="['rounded', over && 'ring-2 ring-primary']"
    data-testid="files-pinned"
    @dragover="onDragOver"
    @dragleave="over = false"
    @drop="onDrop"
  >
    <h2 class="px-2 pb-1 text-xs font-medium text-muted-foreground">
      {{ t('files.sidebar.pinned') }}
    </h2>
    <p
      v-if="shown.length === 0"
      class="px-2 py-1.5 text-xs text-muted-foreground"
      data-testid="files-pinned-empty"
    >
      {{ t('files.sidebar.pinnedHint') }}
    </p>
    <ul>
      <li
        v-for="pin in shown"
        :key="`${JSON.stringify(pin.source)}${pin.path}`"
      >
        <FilesMenu :entries="pinMenu()" @run="prefs.unpin(pin)">
          <button
            type="button"
            draggable="true"
            class="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-sm hover:bg-accent"
            :class="isCurrent(pin) ? 'bg-accent font-medium' : ''"
            :title="titleOf(pin)"
            :data-testid="`files-pin-${labelOf(pin)}`"
            @click="emit('pick', pin)"
            @dragstart="onPinDragStart(pin, $event)"
            @dragend="endPinDrag"
          >
            <Icon name="lucide:folder" class="size-4 shrink-0" />
            <span class="truncate">{{ labelOf(pin) }}</span>
          </button>
        </FilesMenu>
      </li>
    </ul>
    <div
      v-if="dragged"
      :class="[
        'mt-1 flex items-center justify-center gap-2 rounded border border-dashed px-2 py-2 text-xs text-muted-foreground',
        overStrip && 'border-destructive bg-destructive/10 text-destructive',
      ]"
      data-testid="files-pins-unpin-strip"
      @dragover="onStripDragOver"
      @dragleave="overStrip = false"
      @drop="onStripDrop"
    >
      <Icon name="lucide:pin-off" class="size-4 shrink-0" />
      {{ t('files.sidebar.unpinDrop') }}
    </div>
  </section>
</template>
