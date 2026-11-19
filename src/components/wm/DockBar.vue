<script setup lang="ts">
/**
 * The dock as a bar (spec 045): its entries in a row, or in a column at the left and right edge
 * (FR-026), scrolling along its axis when they do not fit; running apps that are not pinned
 * follow after a separator (FR-005). A toolbar with one tab stop; the arrow
 * keys move between the entries, Enter and Space activate (FR-041). Pinned entries and controls
 * are dragged to a new place (FR-019).
 */
import { computed, ref, useTemplateRef, watch } from 'vue'
import { dockItemKey, type DockEntry } from '~/lib/wm/dock'

const props = defineProps<{
  entries: DockEntry[]
  orientation: 'horizontal' | 'vertical'
}>()

const { t } = useI18n()
const root = useTemplateRef<HTMLElement>('root')
const focusIndex = ref(0)

watch(
  () => props.entries.length,
  (length) => {
    if (focusIndex.value >= length) focusIndex.value = Math.max(0, length - 1)
  },
)

const keys = computed(() =>
  props.orientation === 'horizontal'
    ? { previous: 'ArrowLeft', next: 'ArrowRight' }
    : { previous: 'ArrowUp', next: 'ArrowDown' },
)

function buttons(): HTMLElement[] {
  return [
    ...(root.value?.querySelectorAll<HTMLElement>('[data-dock-item]') ?? []),
  ]
}

function onFocusin(event: FocusEvent) {
  const index = buttons().indexOf(event.target as HTMLElement)
  if (index >= 0) focusIndex.value = index
}

function onKeydown(event: KeyboardEvent) {
  const all = buttons()
  if (all.length === 0) return
  let next: number
  if (event.key === keys.value.next) next = (focusIndex.value + 1) % all.length
  else if (event.key === keys.value.previous)
    next = (focusIndex.value - 1 + all.length) % all.length
  else if (event.key === 'Home') next = 0
  else if (event.key === 'End') next = all.length - 1
  else return
  event.preventDefault()
  focusIndex.value = next
  all[next]?.focus()
}

/** Running apps that are not pinned come after a separator (FR-005); -1 without any. */
const firstRunning = computed(() =>
  props.entries.findIndex((entry) => entry.kind === 'app' && !entry.pinned),
)

// ponytail: native drag and drop works with a mouse only; on touch a long press opens the menu,
// so entries are sorted in the settings there (research R8). Upgrade path: pointer-event sorting.
const DRAG_MIME = 'application/x-haex-dock-item'
const dock = useDock()

/** Only stored entries move; a running app that is not pinned has no place to move to. */
function draggable(entry: DockEntry): boolean {
  return entry.kind === 'control' || entry.pinned
}

/** Its index in the stored entries, which also hold apps hidden on this device. */
function storedIndex(key: string): number {
  return dock.items.value.findIndex((item) => dockItemKey(item) === key)
}

function onDragstart(event: DragEvent, entry: DockEntry) {
  event.dataTransfer?.setData(DRAG_MIME, dockItemKey(entry))
  if (event.dataTransfer) event.dataTransfer.effectAllowed = 'move'
}

function onDragover(event: DragEvent, entry: DockEntry) {
  if (!draggable(entry)) return
  if (!event.dataTransfer?.types.includes(DRAG_MIME)) return
  event.preventDefault()
}

function onDrop(event: DragEvent, entry: DockEntry) {
  const key = event.dataTransfer?.getData(DRAG_MIME)
  if (!key || !draggable(entry)) return
  event.preventDefault()
  const from = storedIndex(key)
  const to = storedIndex(dockItemKey(entry))
  if (from >= 0 && to >= 0) void dock.moveAsync(from, to)
}
</script>

<template>
  <div
    ref="root"
    role="toolbar"
    :aria-label="t('wm.dock.label')"
    :aria-orientation="orientation"
    class="flex max-h-full max-w-full gap-1 rounded-full bg-background p-1 shadow-lg ring-1 ring-border"
    :class="
      orientation === 'horizontal'
        ? 'flex-row overflow-x-auto'
        : 'flex-col overflow-y-auto'
    "
    @focusin="onFocusin"
    @keydown="onKeydown"
  >
    <template v-for="(entry, index) in entries" :key="dockItemKey(entry)">
      <span
        v-if="index === firstRunning"
        class="shrink-0 self-stretch bg-border"
        :class="orientation === 'horizontal' ? 'mx-0.5 w-px' : 'my-0.5 h-px'"
        aria-hidden="true"
      />
      <div
        class="shrink-0"
        :draggable="draggable(entry)"
        @dragstart="onDragstart($event, entry)"
        @dragover="onDragover($event, entry)"
        @drop="onDrop($event, entry)"
      >
        <WmDockItem :entry="entry" :tabbable="index === focusIndex" />
      </div>
    </template>
  </div>
</template>
