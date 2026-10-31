<script setup lang="ts">
/**
 * "Orte" in the sidebar of the file browser (spec 044 FR-002, spec 071): the known places of the
 * device, then the folders the user added, from any source (this device, a cloud storage, another own
 * device, a contact's share). An entry is a shortcut that never touches the folder. A folder comes in
 * by its context menu (`useFilesManage`) or by dropping it on the section; any entry, a known place
 * too, goes by its own context menu or by dragging it onto the strip that shows while it is dragged.
 * A removed known place is hidden on this device and comes back when it is added again.
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
  shownKnown,
  shownPins,
} from '~/lib/files/pins'

const props = defineProps<{
  sources: Sources | null
  /** The open folder of the device, `null` while a storage or a list is open. */
  current: string | null
  currentStorage: string | null
  /** The open folder of the open place. */
  currentPath: string | null
}>()

const emit = defineEmits<{ pick: [path: string]; pickPin: [pin: FilesPin] }>()

const { t } = useI18n()
const prefs = useFilesPrefs()

/** What a dragged pin carries, so no other drop target takes it for entries. */
const PIN_MIME = 'application/x-holzi-files-pin'

const PLACE_ICONS: Record<string, string> = {
  home: 'lucide:house',
  pictures: 'lucide:image',
  downloads: 'lucide:download',
  documents: 'lucide:file-text',
  desktop: 'lucide:monitor',
  videos: 'lucide:film',
}

const known = computed(
  () => props.sources?.known.map((place) => place.path) ?? [],
)

/** The known places the user did not remove. */
const knownShown = computed(() => {
  const paths = shownKnown(prefs.places.value, known.value)
  return (
    props.sources?.known.filter((place) => paths.includes(place.path)) ?? []
  )
})

const devicePin = (path: string): FilesPin => ({
  source: { kind: 'device' },
  path,
})

/** The source of a pin as the sidebar lists it: its name and whether it can be opened now; `null`
 * for a folder of this device, `undefined` for a source that is gone. */
type PinSource = { name: string; available: boolean } | null | undefined

function sourceOf(pin: FilesPin): PinSource {
  const { source } = pin
  if (source.kind === 'device') return null
  const id = source.storageId
  const storage = props.sources?.storages.find((entry) => entry.id === id)
  if (storage) return { name: storage.name, available: true }
  const device = props.sources?.devices.find((entry) => entry.id === id)
  if (device)
    return {
      name: device.name ?? t('files.peer.unnamed'),
      available: device.state === 'reachable',
    }
  for (const contact of props.sources?.contacts ?? []) {
    const share = contact.shares.find((entry) => entry.storageId === id)
    if (share) return { name: share.name, available: share.reachable }
  }
  return undefined
}

/** The pins to show: a pin whose source is gone stays stored but hidden, as the source may return. */
const shown = computed(() =>
  shownPins(prefs.places.value, known.value).flatMap((pin) => {
    const source = sourceOf(pin)
    return source === undefined ? [] : [{ pin, source }]
  }),
)

const keyOf = (pin: FilesPin) => `${JSON.stringify(pin.source)}${pin.path}`

const labelOf = (pin: FilesPin, source: PinSource) =>
  pinLabel(pin, source?.name ?? pin.path)

function titleOf(pin: FilesPin, source: PinSource): string {
  return source ? `${source.name}: ${pin.path}` : pin.path
}

function isCurrent(pin: FilesPin): boolean {
  return pin.source.kind === 'device'
    ? props.current === pin.path
    : props.currentStorage === pin.source.storageId &&
        props.currentPath === pin.path
}

// --- Dropping folders on the section ---

const over = ref(false)

/** A drag of holzi's entries that may hold a folder to add. While it hovers the browser hides what
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
  // "copy", as the entries drag allows only copy and move (`useFilesDrag`): a "link" would make the
  // browser refuse the drop. Nothing is copied; the folder itself stays where it is.
  if (event.dataTransfer) event.dataTransfer.dropEffect = 'copy'
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
      (pin) => offerPin(prefs.places.value, pin, known.value) === 'pin',
    ),
    known.value,
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
  if (pin) prefs.unpin(pin, known.value)
}

function endPinDrag() {
  dragged.value = null
  overStrip.value = false
}
</script>

<template>
  <section
    v-if="sources"
    :class="['rounded', over && 'ring-2 ring-primary']"
    data-testid="files-places"
    @dragover="onDragOver"
    @dragleave="over = false"
    @drop="onDrop"
  >
    <h2 class="px-2 pb-1 text-xs font-medium text-muted-foreground">
      {{ t('files.sidebar.places') }}
    </h2>
    <ul>
      <li v-for="place in knownShown" :key="place.path">
        <FilesMenu
          :entries="pinMenu()"
          @run="prefs.unpin(devicePin(place.path), known)"
        >
          <button
            type="button"
            draggable="true"
            class="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-sm hover:bg-accent"
            :class="current === place.path ? 'bg-accent font-medium' : ''"
            :data-testid="`files-place-${place.name}`"
            @click="emit('pick', place.path)"
            @dragstart="onPinDragStart(devicePin(place.path), $event)"
            @dragend="endPinDrag"
          >
            <Icon
              :name="PLACE_ICONS[place.name] ?? 'lucide:folder'"
              class="size-4 shrink-0"
            />
            <span class="truncate">{{ t(`files.places.${place.name}`) }}</span>
          </button>
        </FilesMenu>
      </li>
      <li v-for="{ pin, source } in shown" :key="keyOf(pin)">
        <FilesMenu :entries="pinMenu()" @run="prefs.unpin(pin, known)">
          <button
            type="button"
            draggable="true"
            class="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-sm hover:bg-accent aria-disabled:cursor-not-allowed aria-disabled:opacity-60"
            :class="isCurrent(pin) ? 'bg-accent font-medium' : ''"
            :aria-disabled="source?.available === false"
            :title="titleOf(pin, source)"
            :data-testid="`files-pin-${labelOf(pin, source)}`"
            @click="source?.available !== false && emit('pickPin', pin)"
            @dragstart="onPinDragStart(pin, $event)"
            @dragend="endPinDrag"
          >
            <span class="size-4 shrink-0">
              <FilesFolderIcon :path="pin.path" />
            </span>
            <span class="min-w-0 flex-1 truncate">
              {{ labelOf(pin, source) }}
            </span>
            <span
              v-if="source"
              class="max-w-[45%] shrink-0 truncate text-xs text-muted-foreground"
            >
              {{ source.name }}
            </span>
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
