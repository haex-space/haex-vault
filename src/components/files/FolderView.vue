<script setup lang="ts">
/**
 * The open folder as list or grid (spec 044 FR-003, FR-008): both scroll virtually, so a folder
 * with 50 000 entries mounts only the rows on screen. Loading, empty and error states replace the
 * list; a folder the system denies shows why. A press reports the entry with the keys held (the
 * frame decides between opening and selecting, `lib/files/clipboard.ts`); a double click, Enter or
 * Space opens it, a click on the empty area clears the selection. Entries drag onto a folder row or
 * the open folder (FR-018).
 */
import type { Entry } from '@bindings/Entry'
import type { FilesError } from '@bindings/FilesError'
import type { SourceRef } from '@bindings/SourceRef'
import type { FilesView } from '~/composables/useFilesPrefs'
import {
  type ClickKeys,
  type DragPayload,
  dragPayload,
  ENTRIES_MIME,
  parseDragPayload,
} from '~/lib/files/clipboard'
import { shownNameKey } from '~/lib/files/encrypted'
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

const { t, d } = useI18n()

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

// A new folder starts at the top. Not on every new list: the watch and the focus load the same
// folder again, and that keeps where the user scrolled to.
watch(
  () => `${JSON.stringify(props.source)}|${props.folder}`,
  () => {
    listVirtual.scrollTo(0)
    gridVirtual.scrollTo(0)
  },
)

function modified(entry: Entry): string {
  return entry.modifiedMs === null
    ? ''
    : d(new Date(entry.modifiedMs), { dateStyle: 'medium', timeStyle: 'short' })
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

function onBackgroundClick(event: MouseEvent) {
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
    class="h-full min-h-0"
    :class="dropOver === folder ? 'ring-2 ring-primary ring-inset' : ''"
    @pointerdown="onPointerdown"
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
      class="h-full overflow-y-auto"
      role="list"
      :aria-busy="loading"
      data-testid="files-entries"
      @click="onBackgroundClick"
    >
      <div v-bind="wrapperProps">
        <template v-if="view === 'list'">
          <button
            v-for="{ data: [entry], index } in list"
            :key="entry?.path ?? index"
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
            v-for="{ data: row, index } in list"
            :key="index"
            class="flex"
            :style="{ height: `${TILE_HEIGHT}px` }"
          >
            <button
              v-for="entry in row"
              :key="entry.path"
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
  </div>
</template>
