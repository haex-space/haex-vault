<script setup lang="ts">
/**
 * The open folder as list or grid (spec 044 FR-003, FR-008): both scroll virtually, so a folder
 * with 50 000 entries mounts only the rows on screen. Empty and error states replace the list, a
 * folder that takes a moment to load shows the loader over it (`FilesLoader`); a folder the system
 * denies shows why. A press reports the entry with the keys held (the frame decides between opening
 * and selecting, `lib/files/clipboard.ts`); a double click, Enter or Space opens it, a click on the
 * empty area clears the selection. Dragging with the mouse from the empty area pulls up a band that
 * selects the entries it touches (Ctrl, Cmd or Shift add to the selection). Entries drag onto a
 * folder row or the open folder (FR-018). The keyboard moves the focus through the entries (`focusEntry`, FR-017b), and a new folder takes the
 * focus unless a text field has it.
 */
import type { Entry } from '@bindings/Entry'
import type { FilesError } from '@bindings/FilesError'
import type { SourceRef } from '@bindings/SourceRef'
import type { FilesView } from '~/composables/useFilesPrefs'
import {
  bandBetween,
  bandSelection,
  type CellGrid,
  type ClickKeys,
  type DragPayload,
  dragPayload,
  ENTRIES_MIME,
  indexesInBand,
  parseDragPayload,
  type Selection,
} from '~/lib/files/clipboard'
import { shownNameKey } from '~/lib/files/encrypted'
import { isEditableTarget } from '~/lib/wm/keybindings'
import { formatFileSize } from '~/lib/passwords/format'

const props = defineProps<{
  source: SourceRef
  entries: Entry[]
  loading: boolean
  error: FilesError | null
  view: FilesView
  /** The open folder, where a drop on the empty area goes. */
  folder: string
  /** Selected paths. */
  selected: readonly string[]
  /** Paths cut to holzi's clipboard, shown faded. */
  dimmed: readonly string[]
  /** An error of a storage's credentials offers its settings (US5). */
  settingsLink?: boolean
}>()

const emit = defineEmits<{
  press: [entry: Entry, keys: ClickKeys]
  /** A band dragged over the entries sets the selection. */
  band: [selection: Selection]
  open: [entry: Entry]
  clear: []
  /** The context menu opens over an entry, or over the empty area (`null`). */
  context: [entry: Entry | null]
  settings: []
  drop: [
    payload: DragPayload,
    target: string,
    keys: { ctrl: boolean; alt: boolean },
  ]
}>()

const { t, locale } = useI18n()

/** The name as shown: unreadable encrypted entries get a word instead of their random id. */
function shownName(entry: Entry): string {
  const key = shownNameKey(entry)
  return key ? t(key) : entry.name
}

const ROW_HEIGHT = 40
const TILE_WIDTH = 132
const TILE_HEIGHT = 140

const container = useTemplateRef<HTMLElement>('container')
const { width } = useElementSize(container)
const columns = computed(() =>
  Math.max(1, Math.floor((width.value || TILE_WIDTH) / TILE_WIDTH)),
)

/** The list's rows: one entry each. */
const listRows = computed<Entry[][]>(() =>
  props.entries.map((entry) => [entry]),
)
/** The grid's rows: `columns` entries each. */
const gridRows = computed<Entry[][]>(() => {
  const out: Entry[][] = []
  for (let i = 0; i < props.entries.length; i += columns.value) {
    out.push(props.entries.slice(i, i + columns.value))
  }
  return out
})

// One virtual list per view, each with a fixed row height: given a function, VueUse adds up the
// heights of all rows above the first one shown on every scroll, and the garbage of that stalled
// a folder of 50 000 entries for more than half a second at a time (FR-008).
const listVirtual = useVirtualList(listRows, {
  itemHeight: ROW_HEIGHT,
  overscan: 8,
})
const gridVirtual = useVirtualList(gridRows, {
  itemHeight: TILE_HEIGHT,
  overscan: 8,
})
const virtual = computed(() =>
  props.view === 'list' ? listVirtual : gridVirtual,
)
const list = computed(() => virtual.value.list.value)
const containerProps = computed(() => virtual.value.containerProps)
const wrapperProps = computed(() => virtual.value.wrapperProps.value)

// Rows keep their elements while the list scrolls: a slot shows the next entry instead of a new row
// being built for it, which a jump through a large folder would do for every row on screen
// (FR-008). A focused row whose slot now shows another entry hands the focus to the list, so Enter
// never opens what the user did not focus and the keys still act here (FR-017b); so does a focused
// row that is gone (deleted, or scrolled away).
let focusedPath: string | null = null

function onFocusin(event: FocusEvent) {
  focusedPath =
    (event.target as HTMLElement | null)
      ?.closest('[data-path]')
      ?.getAttribute('data-path') ?? null
}

watch(
  list,
  () => {
    const active = document.activeElement
    const moved =
      active instanceof HTMLElement &&
      container.value?.contains(active) &&
      active !== container.value &&
      active.closest('[data-path]')?.getAttribute('data-path') !== focusedPath
    const lost = focusedPath !== null && (!active || active === document.body)
    if (moved || lost) {
      focusedPath = null
      container.value?.focus({ preventScroll: true })
    }
  },
  { flush: 'post' },
)

// A new folder starts at the top. Not on every new list: the watch and the focus load the same
// folder again, and that keeps where the user scrolled to.
watch(
  () => `${JSON.stringify(props.source)}|${props.folder}`,
  () => {
    listVirtual.scrollTo(0)
    gridVirtual.scrollTo(0)
    void nextTick(takeFocus)
  },
)
onMounted(takeFocus)

/** Gives the list the focus after a folder opened, so the keys act there (FR-017b): unless a text
 * field or a place outside the file browser has it. */
function takeFocus() {
  const active = document.activeElement
  const frame = container.value?.closest('[data-files-app]')
  const lost = !active || active === document.body
  if (!lost && (!frame?.contains(active) || isEditableTarget(active))) return
  if (container.value?.contains(active)) return
  container.value?.focus({ preventScroll: true })
}

/** How many entries one row holds: 1 in the list. */
const rowLength = computed(() => (props.view === 'list' ? 1 : columns.value))

const rowElements = () =>
  container.value?.querySelectorAll<HTMLElement>('[data-path]') ?? []

/**
 * Moves the focus to the entry at `path` (FR-017b): scrolls its row into view first, as the virtual
 * list only builds the rows on screen, and focuses it once it is built.
 */
async function focusEntry(path: string) {
  const index = props.entries.findIndex((entry) => entry.path === path)
  if (index < 0) return
  const height = props.view === 'list' ? ROW_HEIGHT : TILE_HEIGHT
  const top = Math.floor(index / rowLength.value) * height
  const scroller = containerProps.value.ref.value
  if (scroller) {
    if (top < scroller.scrollTop) scroller.scrollTop = top
    else if (top + height > scroller.scrollTop + scroller.clientHeight)
      scroller.scrollTop = top + height - scroller.clientHeight
  }
  // The virtual list builds the row after the scroll event: a few frames at most.
  for (let frame = 0; frame < 10; frame++) {
    await nextTick()
    const row = [...rowElements()].find(
      (element) => element.getAttribute('data-path') === path,
    )
    if (row) {
      focusedPath = path
      row.focus({ preventScroll: true })
      return
    }
    await new Promise((resolve) => requestAnimationFrame(resolve))
  }
}

defineExpose({ focusEntry, rowLength })

// One format for every row: vue-i18n's `d()` with options builds a new `Intl.DateTimeFormat` on
// each call, and a scroll through a large folder mounts dozens of rows per frame (FR-008).
const dateFormat = computed(
  () =>
    new Intl.DateTimeFormat(locale.value, {
      dateStyle: 'medium',
      timeStyle: 'short',
    }),
)

function modified(entry: Entry): string {
  return entry.modifiedMs === null
    ? ''
    : dateFormat.value.format(entry.modifiedMs)
}

function size(entry: Entry): string {
  return entry.kind === 'dir' || entry.size === null
    ? ''
    : formatFileSize(entry.size)
}

/** The last press came from a finger: a tap opens, and a double tap is no double click. */
let touch = false

function press(entry: Entry, event: MouseEvent) {
  // Enter and Space click a focused row without a pointer.
  if (event.detail === 0) {
    emit('open', entry)
    return
  }
  emit('press', entry, {
    toggle: event.ctrlKey || event.metaKey,
    range: event.shiftKey,
    touch,
  })
}

function onPointerdown(event: PointerEvent) {
  touch = event.pointerType === 'touch'
}

function open(entry: Entry) {
  if (!touch) emit('open', entry)
}

// --- Selecting with a band ---
// A press with the mouse on the empty area, then a drag of a few pixels, pulls up a band over the
// entries. The rows are virtual, so the band is worked out from the layout (`indexesInBand`) and
// not from the elements on screen; dragging past the top or bottom edge scrolls.

/** How far the pointer moves before a press becomes a band, so a click stays a click. */
const BAND_THRESHOLD = 4
/** The zone along the top and bottom edge that scrolls while a band is dragged. */
const BAND_EDGE = 32

const cells = computed<CellGrid>(() =>
  props.view === 'list'
    ? { columns: 1, cellWidth: Infinity, cellHeight: ROW_HEIGHT }
    : {
        columns: columns.value,
        cellWidth: TILE_WIDTH,
        cellHeight: TILE_HEIGHT,
      },
)

/** The drawn band, in the list's visible pixels. */
const bandBox = ref<{
  left: number
  top: number
  width: number
  height: number
} | null>(null)

let bandPress: { x: number; y: number } | null = null
let bandPointer: { x: number; y: number } | null = null
let bandBase: Selection = { paths: [], anchor: null }
let bandAdditive = false
let bandActive = false
let bandFrame = 0
let bandHits: string[] = []
/** The click that ends a band must not clear what the band selected. */
let swallowClick = false

function onBandDown(event: PointerEvent) {
  if (event.pointerType !== 'mouse' || event.button !== 0) return
  const scroller = containerProps.value.ref.value
  if (!scroller) return
  if ((event.target as HTMLElement | null)?.closest('[data-path]')) return
  const rect = scroller.getBoundingClientRect()
  // The scroll bar is no empty area.
  if (event.clientX - rect.left >= scroller.clientWidth) return
  bandPress = {
    x: event.clientX - rect.left + scroller.scrollLeft,
    y: event.clientY - rect.top + scroller.scrollTop,
  }
  bandPointer = { x: event.clientX, y: event.clientY }
  bandAdditive = event.ctrlKey || event.metaKey || event.shiftKey
  bandBase = { paths: [...props.selected], anchor: props.selected[0] ?? null }
  bandHits = []
  scroller.setPointerCapture(event.pointerId)
}

function onBandMove(event: PointerEvent) {
  if (!bandPress) return
  bandPointer = { x: event.clientX, y: event.clientY }
  updateBand()
}

function updateBand() {
  const scroller = containerProps.value.ref.value
  if (!scroller || !bandPress || !bandPointer) return
  const rect = scroller.getBoundingClientRect()
  const to = {
    x:
      Math.min(Math.max(bandPointer.x - rect.left, 0), scroller.clientWidth) +
      scroller.scrollLeft,
    y: bandPointer.y - rect.top + scroller.scrollTop,
  }
  if (!bandActive) {
    if (Math.hypot(to.x - bandPress.x, to.y - bandPress.y) < BAND_THRESHOLD)
      return
    bandActive = true
    bandFrame = requestAnimationFrame(bandTick)
  }
  const box = bandBetween(bandPress, to)
  bandBox.value = {
    left: box.left - scroller.scrollLeft,
    top: box.top - scroller.scrollTop,
    width: box.right - box.left,
    height: box.bottom - box.top,
  }
  const hits = indexesInBand(box, cells.value, props.entries.length).map(
    (index) => props.entries[index]!.path,
  )
  if (
    hits.length === bandHits.length &&
    hits.every((path, index) => path === bandHits[index])
  )
    return
  bandHits = hits
  emit('band', bandSelection(bandBase, hits, bandAdditive))
}

/** Scrolls while the pointer is held near the top or bottom edge, and follows the wheel. */
function bandTick() {
  bandFrame = 0
  const scroller = containerProps.value.ref.value
  if (!bandActive || !scroller || !bandPointer) return
  const rect = scroller.getBoundingClientRect()
  const above = rect.top + BAND_EDGE - bandPointer.y
  const below = bandPointer.y - (rect.bottom - BAND_EDGE)
  if (above > 0) scroller.scrollTop -= Math.min(above, BAND_EDGE * 2) / 3
  else if (below > 0) scroller.scrollTop += Math.min(below, BAND_EDGE * 2) / 3
  updateBand()
  bandFrame = requestAnimationFrame(bandTick)
}

function onBandEnd(event: PointerEvent) {
  if (!bandPress) return
  const scroller = containerProps.value.ref.value
  if (scroller?.hasPointerCapture(event.pointerId))
    scroller.releasePointerCapture(event.pointerId)
  if (bandActive) {
    swallowClick = true
    setTimeout(() => (swallowClick = false), 0)
  }
  bandPress = null
  bandPointer = null
  bandActive = false
  bandBox.value = null
  if (bandFrame) cancelAnimationFrame(bandFrame)
  bandFrame = 0
}

onBeforeUnmount(() => {
  if (bandFrame) cancelAnimationFrame(bandFrame)
})

function onBackgroundClick(event: MouseEvent) {
  if (swallowClick) return
  if (!(event.target as HTMLElement | null)?.closest('[data-path]')) {
    emit('clear')
  }
}

function onContextMenu(event: MouseEvent) {
  const row = (event.target as HTMLElement | null)?.closest('[data-path]')
  const path = row?.getAttribute('data-path')
  emit('context', props.entries.find((entry) => entry.path === path) ?? null)
}

function rowClass(entry: Entry): string[] {
  return [
    entry.hidden || props.dimmed.includes(entry.path) ? 'opacity-60' : '',
    props.selected.includes(entry.path) ? 'bg-primary/15' : '',
    dropOver.value === entry.path ? 'ring-2 ring-primary ring-inset' : '',
  ]
}

/** The folder a drag hovers over (a folder row, or the open folder). */
const dropOver = ref<string | null>(null)

function onDragStart(entry: Entry, event: DragEvent) {
  const paths = props.selected.includes(entry.path)
    ? props.selected
    : [entry.path]
  const items = props.entries
    .filter((candidate) => paths.includes(candidate.path))
    .map((candidate) => ({ path: candidate.path, kind: candidate.kind }))
  event.dataTransfer?.setData(
    ENTRIES_MIME,
    dragPayload({ source: props.source, folder: props.folder, items }),
  )
  if (event.dataTransfer) event.dataTransfer.effectAllowed = 'copyMove'
}

/** Accepts a drag of holzi's entries over a folder row (`entry`) or the open folder (`null`). */
function onDragOver(entry: Entry | null, event: DragEvent) {
  if (!event.dataTransfer?.types.includes(ENTRIES_MIME)) return
  if (entry && entry.kind !== 'dir') return
  event.preventDefault()
  event.stopPropagation()
  dropOver.value = entry?.path ?? props.folder
}

function onDrop(entry: Entry | null, event: DragEvent) {
  dropOver.value = null
  if (entry && entry.kind !== 'dir') return
  const payload = parseDragPayload(event.dataTransfer?.getData(ENTRIES_MIME))
  if (!payload) return
  event.preventDefault()
  event.stopPropagation()
  emit('drop', payload, entry?.path ?? props.folder, {
    ctrl: event.ctrlKey,
    alt: event.altKey,
  })
}

const errorText = computed(() => {
  if (!props.error) return ''
  const key = `files.error.${props.error.code}`
  return t(key) === key ? props.error.message : t(key)
})
</script>

<template>
  <div
    ref="container"
    class="relative h-full min-h-0 outline-none"
    :class="dropOver === folder ? 'ring-2 ring-primary ring-inset' : ''"
    tabindex="-1"
    data-files-list
    @pointerdown="onPointerdown"
    @focusin="onFocusin"
    @contextmenu="onContextMenu"
    @dragover="onDragOver(null, $event)"
    @dragleave="dropOver = null"
    @drop="onDrop(null, $event)"
  >
    <div
      v-if="error"
      class="flex h-full flex-col items-center justify-center gap-2 p-6 text-center text-sm text-muted-foreground"
      data-testid="files-error"
    >
      <Icon name="lucide:circle-alert" class="size-8" />
      <p>{{ errorText }}</p>
      <UiButton
        v-if="settingsLink"
        variant="outline"
        size="sm"
        data-testid="files-storage-settings"
        @click="emit('settings')"
      >
        {{ t('files.storage.settings') }}
      </UiButton>
    </div>
    <div
      v-else-if="!loading && entries.length === 0"
      class="flex h-full items-center justify-center p-6 text-sm text-muted-foreground"
      data-testid="files-empty"
    >
      {{ t('files.empty') }}
    </div>
    <div
      v-else
      v-bind="containerProps"
      class="h-full overflow-y-auto select-none"
      role="list"
      :aria-busy="loading"
      data-testid="files-entries"
      @click="onBackgroundClick"
      @pointerdown="onBandDown"
      @pointermove="onBandMove"
      @pointerup="onBandEnd"
      @pointercancel="onBandEnd"
    >
      <div v-bind="wrapperProps">
        <template v-if="view === 'list'">
          <button
            v-for="({ data: [entry] }, slot) in list"
            :key="slot"
            type="button"
            role="listitem"
            class="flex w-full items-center gap-3 px-3 text-left text-sm hover:bg-accent focus-visible:bg-accent focus-visible:outline-none"
            :class="entry ? rowClass(entry) : ''"
            :style="{ height: `${ROW_HEIGHT}px` }"
            :data-testid="`files-entry-${entry?.name}`"
            :data-path="entry?.path"
            :aria-selected="entry ? selected.includes(entry.path) : undefined"
            :draggable="!!entry"
            @click="entry && press(entry, $event)"
            @dblclick="entry && open(entry)"
            @dragstart="entry && onDragStart(entry, $event)"
            @dragover="entry && onDragOver(entry, $event)"
            @drop="entry && onDrop(entry, $event)"
          >
            <template v-if="entry">
              <span class="size-5 shrink-0">
                <FilesEntryIcon :source="source" :entry="entry" />
              </span>
              <span class="min-w-0 flex-1 truncate">{{
                shownName(entry)
              }}</span>
              <Icon
                v-if="entry.symlink"
                name="lucide:link"
                class="size-3.5 shrink-0 text-muted-foreground"
                :aria-label="t('files.symlink')"
              />
              <Icon
                v-if="entry.holziOwned"
                name="lucide:lock"
                class="size-3.5 shrink-0 text-muted-foreground"
                :aria-label="t('files.holziOwned')"
              />
              <span
                class="hidden w-24 shrink-0 text-right text-muted-foreground @md:block"
              >
                {{ size(entry) }}
              </span>
              <span
                class="hidden w-40 shrink-0 text-right text-muted-foreground @xl:block"
              >
                {{ modified(entry) }}
              </span>
            </template>
          </button>
        </template>
        <template v-else>
          <div
            v-for="({ data: row }, slot) in list"
            :key="slot"
            class="flex"
            :style="{ height: `${TILE_HEIGHT}px` }"
          >
            <button
              v-for="(entry, column) in row"
              :key="column"
              type="button"
              role="listitem"
              class="flex flex-col items-center gap-1 rounded p-2 text-xs hover:bg-accent focus-visible:bg-accent focus-visible:outline-none"
              :class="rowClass(entry)"
              :style="{ width: `${TILE_WIDTH}px` }"
              :data-testid="`files-entry-${entry.name}`"
              :data-path="entry.path"
              :aria-selected="selected.includes(entry.path)"
              draggable="true"
              @click="press(entry, $event)"
              @dblclick="open(entry)"
              @dragstart="onDragStart(entry, $event)"
              @dragover="onDragOver(entry, $event)"
              @drop="onDrop(entry, $event)"
            >
              <span class="size-24">
                <FilesEntryIcon :source="source" :entry="entry" thumbnail />
              </span>
              <span class="line-clamp-2 w-full text-center break-all">
                {{ shownName(entry) }}
              </span>
            </button>
          </div>
        </template>
      </div>
    </div>
    <div
      v-if="bandBox"
      class="pointer-events-none absolute inset-0 overflow-hidden"
      aria-hidden="true"
    >
      <div
        class="absolute border border-primary bg-primary/15"
        :style="{
          left: `${bandBox.left}px`,
          top: `${bandBox.top}px`,
          width: `${bandBox.width}px`,
          height: `${bandBox.height}px`,
        }"
        data-testid="files-band"
      />
    </div>
    <FilesLoader :loading="loading" />
  </div>
</template>
