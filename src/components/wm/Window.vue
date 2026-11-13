<script setup lang="ts">
/**
 * One window manager window: frame, header (`WmHeader`: the controls of the active tab's app, the
 * window buttons; spec 090 replaced the tab strip), drag-to-move, eight-way resize, and its tabs'
 * content (spec 015-workspace-shell, T023 + T029 + T031 + T034).
 *
 * Each tab renders through its own `WmTabPanel` (T034) rather than this
 * component providing `useWmTab()` itself, so Vue's provide/inject stays
 * correctly scoped per tab even with several mounted at once (research R8).
 */
import { computed, provide, ref } from 'vue'
import { getAppDefinition } from '~/lib/wm/apps'
import { windowDisplayRect } from '~/lib/wm/geometry'
import { WM_OVERFLOW_OPEN, WM_OVERLAY } from '~/lib/wm/overlay'
import {
  useWindowPointerGesture,
  type ResizeDirection,
} from '~/composables/useWindowPointerGesture'
import type { WmWindow } from '~/lib/wm/types'
import type { WmHeaderEvent } from '~/types/bindings/WmHeaderEvent'

const props = defineProps<{
  window: WmWindow
  active: boolean
}>()

/** The header's overflow menu is drawn over this window's content, not over the screen. */
const overlay = ref<HTMLElement | null>(null)
provide(WM_OVERLAY, overlay)
/** Whether the overflow panel is open; a sidebar over the content closes it and is closed by it. */
const overflowOpen = ref(false)
provide(WM_OVERFLOW_OPEN, overflowOpen)

const wm = useWindowManagerStore()
const { windowHint } = useAppearance()
const { t } = useI18n()
const runBack = useAction('wm.tab.back')
const runForward = useAction('wm.tab.forward')

/** Mouse side buttons (DOM `button` 3 = back, 4 = forward) act on this window's active tab without
 * focusing it (spec 020 FR-018); their default webview navigation is suppressed. Whether a
 * platform delivers them to the DOM at all is research R6's spike. */
function isSideButton(event: MouseEvent | PointerEvent): boolean {
  return event.button === 3 || event.button === 4
}

// Listened to in the capture phase: the header's controls stop `pointerdown` themselves, so using
// one in a window behind the others must still bring that window forward.
// action-exempt: implicit focus on pointer down is part of the pointer gesture, not a control.
function onRootPointerDown(event: PointerEvent) {
  if (!isSideButton(event)) wm.focusWindow(props.window.id)
}

function onSideButton(event: MouseEvent) {
  if (!isSideButton(event)) return
  event.preventDefault()
  if (event.type !== 'mouseup') return
  const run = event.button === 3 ? runBack : runForward
  void run({ tabId: props.window.activeTabId })
}

const info = computed(() => wm.windowDisplayInfo(props.window))
const title = computed(() => {
  const displayInfo = info.value
  if (!displayInfo) return ''
  return (
    displayInfo.titleOverride ??
    (displayInfo.titleKey
      ? t(displayInfo.titleKey, displayInfo.titleParams)
      : '')
  )
})

// The largest minimum size across all of the window's tabs' apps, so it never shrinks below what
// any of them needs — not just the currently active one.
const minSize = computed(() => {
  let width = 0
  let height = 0
  for (const tab of props.window.tabs) {
    const app = getAppDefinition(tab.appId, wm.apps())
    if (app) {
      width = Math.max(width, app.minSize.width)
      height = Math.max(height, app.minSize.height)
    }
  }
  return { width, height }
})

const displayRect = computed(() =>
  windowDisplayRect(props.window, wm.compact, wm.area),
)
// Neither compact nor a maximized window offers moving/resizing (FR-028 for compact; a maximized
// window has nowhere to move or grow to until it is restored).
const interactive = computed(() => !wm.compact && !props.window.maximized)

const { startMove, startResize } = useWindowPointerGesture(
  () => ({
    x: props.window.x,
    y: props.window.y,
    width: props.window.width,
    height: props.window.height,
  }),
  () => minSize.value,
  () => wm.area,
  // action-exempt: continuous drag/resize gesture; agents use wm.window.setGeometry.
  (geometry) => wm.updateWindowGeometry(props.window.id, geometry),
)

/** Starts a move gesture only while the window can be dragged. */
function onTitleBarPointerDown(event: PointerEvent) {
  if (interactive.value) startMove(event)
}

/** Starts resizing in the handle's direction when geometry is editable. */
function onResizeHandlePointerDown(
  event: PointerEvent,
  direction: ResizeDirection,
) {
  if (interactive.value) startResize(event, direction)
}

/** What a double-click must leave alone: the header's controls and the window buttons. */
const HEADER_CONTROL_SELECTOR =
  'button, input, a, [role="switch"], [data-header-control]'

/** Toggles maximization from the empty part of the title bar, outside compact mode. */
function onTitleBarDoubleClick(event: MouseEvent) {
  const target = event.target as Element | null
  if (target?.closest(HEADER_CONTROL_SELECTOR)) return
  if (!wm.compact) void toggleMaximize()
}

// Spec 020 FR-024: the title-bar controls trigger catalog actions for this window/tab; closing
// still runs every close guard (the actions use the same confirmation helpers).
const windowTarget = () => ({ windowId: props.window.id })
const runMinimize = useAction('wm.window.minimize')
const runToggleMaximize = useAction('wm.window.toggleMaximize')
const runCloseWindow = useAction('wm.window.close')
const minimize = () => runMinimize(windowTarget())
const toggleMaximize = () => runToggleMaximize(windowTarget())
const requestClose = () => void runCloseWindow(windowTarget())

/** What the user does to a control of the header goes to the app of the active tab (spec 090). */
const sendHeaderEvent = (
  event: WmHeaderEvent,
  tabId: string = props.window.activeTabId,
) => wm.sendHeaderEvent(tabId, event)

const RESIZE_HANDLES: { direction: ResizeDirection; class: string }[] = [
  { direction: 'n', class: 'inset-x-2 top-0 h-1 cursor-ns-resize' },
  { direction: 's', class: 'inset-x-2 bottom-0 h-1 cursor-ns-resize' },
  { direction: 'e', class: 'inset-y-2 right-0 w-1 cursor-ew-resize' },
  { direction: 'w', class: 'inset-y-2 left-0 w-1 cursor-ew-resize' },
  { direction: 'ne', class: 'right-0 top-0 h-2 w-2 cursor-nesw-resize' },
  { direction: 'nw', class: 'left-0 top-0 h-2 w-2 cursor-nwse-resize' },
  { direction: 'se', class: 'right-0 bottom-0 h-2 w-2 cursor-nwse-resize' },
  { direction: 'sw', class: 'left-0 bottom-0 h-2 w-2 cursor-nesw-resize' },
]
</script>

<template>
  <div
    v-show="!window.minimized"
    class="absolute flex flex-col overflow-hidden rounded-lg border bg-background shadow-lg"
    :class="
      active
        ? windowHint
          ? 'border-primary'
          : 'border-foreground/40'
        : 'border-border'
    "
    :style="{
      left: `${displayRect.x}px`,
      top: `${displayRect.y}px`,
      width: `${displayRect.width}px`,
      height: `${displayRect.height}px`,
      zIndex: window.stack,
    }"
    role="group"
    :aria-label="title"
    :data-wm-window-id="window.id"
    @pointerdown.capture="onRootPointerDown"
    @mousedown="onSideButton"
    @auxclick="onSideButton"
    @mouseup="onSideButton"
  >
    <WmHeader
      :title="title"
      :description="wm.headerOf(window.activeTabId)"
      :owner="window.activeTabId"
      :dev="info?.dev"
      :maximized="window.maximized"
      :compact="wm.compact"
      :movable="interactive"
      @pointerdown="onTitleBarPointerDown"
      @dblclick="onTitleBarDoubleClick"
      @event="sendHeaderEvent"
      @minimize="minimize"
      @toggle-maximize="toggleMaximize"
      @close="requestClose"
    />
    <div class="relative min-h-0 flex-1 overflow-hidden">
      <WmTabPanel
        v-for="tab in window.tabs"
        :key="tab.id"
        :tab="tab"
        :window-id="window.id"
        :active="tab.id === window.activeTabId"
      />
      <div
        ref="overlay"
        class="@container pointer-events-none absolute inset-0 z-30 overflow-hidden"
        data-testid="wm-window-overlay"
      />
    </div>
    <div
      v-for="handle in RESIZE_HANDLES"
      v-show="interactive"
      :key="handle.direction"
      class="absolute"
      :class="handle.class"
      :style="{ touchAction: 'none' }"
      @pointerdown="onResizeHandlePointerDown($event, handle.direction)"
    />
  </div>
</template>
