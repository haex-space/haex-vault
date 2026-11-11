<script setup lang="ts">
/**
 * The dock of the desktop (spec 045, FR-001): the stored entries plus the running apps
 * (`resolveDockEntries`) at the effective placement (`effectivePlacement`: the user's choice, or
 * the bottom in compact mode, FR-031). Reserving space it is a flex sibling of the window area, so
 * `wm/Desktop.vue` measures what is left (FR-024); floating and hiding it lies over the windows.
 * Hiding, it comes in when the pointer reaches its edge and leaves 400 ms after the pointer, unless
 * focus is inside or one of its menus is open (FR-025). Right click on its free area sets edge,
 * alignment, style and mode (FR-018). The mode applies to the wheel too: reserving space it keeps a
 * strip the size of its button, its fanned entries always lie over the windows (FR-030).
 */
import { computed, provide, reactive, ref, useTemplateRef, watch } from 'vue'
import { DOCK_HOLD, dockEntryAt } from '~/composables/useDock'
import {
  resolveDockEntries,
  withAssistant,
  type DockEntry,
} from '~/lib/wm/dock'

const wm = useWindowManagerStore()
const dock = useDock()

const assistant = useAssistantStore()
void assistant.start()
// The assistant's toggle is a stored entry (spec 066 FR-001); while the assistant is on it shows even
// when the user took it out of the dock, so it can be switched off.
const entries = computed<DockEntry[]>(() =>
  withAssistant(
    resolveDockEntries(dock.items.value, wm.windows),
    assistant.state.enabled,
    assistant.available,
  ),
)
const placement = dock.effective
const vertical = computed(
  () => placement.value.edge === 'left' || placement.value.edge === 'right',
)
const reserved = computed(() => placement.value.mode === 'reserved')
const autohide = computed(() => placement.value.mode === 'autohide')

const JUSTIFY = {
  start: 'justify-start',
  center: 'justify-center',
  end: 'justify-end',
} as const
const ITEMS = {
  top: 'items-start',
  bottom: 'items-end',
  left: 'items-start',
  right: 'items-end',
} as const
const HIDDEN = {
  top: '-translate-y-[calc(100%+1rem)]',
  bottom: 'translate-y-[calc(100%+1rem)]',
  left: '-translate-x-[calc(100%+1rem)]',
  right: 'translate-x-[calc(100%+1rem)]',
} as const
const STRIP = {
  top: 'inset-x-0 top-0 h-1',
  bottom: 'inset-x-0 bottom-0 h-1',
  left: 'inset-y-0 left-0 w-1',
  right: 'inset-y-0 right-0 w-1',
} as const

const frameClass = computed(() => [
  'flex p-2',
  vertical.value ? 'flex-col' : 'flex-row',
  JUSTIFY[placement.value.align],
  reserved.value
    ? 'shrink-0'
    : `pointer-events-none absolute inset-0 z-10 ${ITEMS[placement.value.edge]}`,
])

// Auto-hide (FR-024, FR-025).
const bar = useTemplateRef<HTMLElement>('bar')
const { focused } = useFocusWithin(bar)
const pointerInside = ref(false)
/** The menus and choosers open right now (`useDockHold`); a set, so a holder that reports
 * "closed" twice, or only on unmount, cannot unbalance it. */
const holders = reactive(new Set<symbol>())
const revealed = ref(false)
let hideTimer: ReturnType<typeof setTimeout> | undefined

function hold(holder: symbol, open: boolean) {
  if (open) holders.add(holder)
  else if (holders.delete(holder)) scheduleHide()
}
provide(DOCK_HOLD, hold)

// The bar's one context menu (only one menu open at a time): its content follows the entry it was
// opened on, taken when it opens, so it does not change under the pointer while the entry changes.
// Switching to the wheel from it removes the bar before reka closes the menu.
const menuEntry = ref<DockEntry | null>(null)
function pickMenuEntry(event: Event) {
  menuEntry.value = dockEntryAt(event.target, entries.value)
}
const barMenu = Symbol('dockBarMenu')
function holdBarMenu(open: boolean) {
  hold(barMenu, open)
}
watch(
  () => placement.value.style,
  () => hold(barMenu, false),
)

const shown = computed(
  () => !autohide.value || revealed.value || focused.value || holders.size > 0,
)

function reveal() {
  clearTimeout(hideTimer)
  revealed.value = true
}

function scheduleHide() {
  clearTimeout(hideTimer)
  hideTimer = setTimeout(() => {
    if (!pointerInside.value && holders.size === 0) revealed.value = false
  }, 400)
}

function onPointerenter() {
  pointerInside.value = true
  reveal()
}

function onPointerleave() {
  pointerInside.value = false
  scheduleHide()
}
</script>

<template>
  <div
    :class="frameClass"
    data-testid="dock"
    :data-style="placement.style"
    :data-edge="placement.edge"
    :data-align="placement.align"
    :data-mode="placement.mode"
  >
    <div
      v-if="autohide"
      class="pointer-events-auto absolute"
      :class="STRIP[placement.edge]"
      aria-hidden="true"
      @pointerenter="reveal"
      @pointerleave="scheduleHide"
    />
    <div
      v-if="placement.style === 'wheel'"
      ref="bar"
      class="pointer-events-auto transition-transform duration-200 motion-reduce:transition-none"
      :class="shown ? '' : HIDDEN[placement.edge]"
      @pointerenter="onPointerenter"
      @pointerleave="onPointerleave"
    >
      <WmDockWheel
        :entries="entries"
        :edge="placement.edge"
        :align="placement.align"
      />
    </div>
    <ShadcnContextMenu v-else @update:open="holdBarMenu">
      <ShadcnContextMenuTrigger as-child>
        <div
          ref="bar"
          class="pointer-events-auto max-h-full max-w-full transition-transform duration-200 motion-reduce:transition-none"
          :class="shown ? '' : HIDDEN[placement.edge]"
          @pointerenter="onPointerenter"
          @pointerleave="onPointerleave"
          @pointerdown.capture="pickMenuEntry"
          @contextmenu.capture="pickMenuEntry"
        >
          <WmDockBar
            :entries="entries"
            :orientation="vertical ? 'vertical' : 'horizontal'"
          />
        </div>
      </ShadcnContextMenuTrigger>
      <ShadcnContextMenuContent class="min-w-48" data-testid="dock-menu">
        <WmDockMenu :entry="menuEntry" />
      </ShadcnContextMenuContent>
    </ShadcnContextMenu>
  </div>
</template>
