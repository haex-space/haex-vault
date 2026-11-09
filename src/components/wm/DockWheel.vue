<script setup lang="ts">
/**
 * The dock as a wheel (spec 045, US5): one round button that fans the entries out over a quarter
 * circle at a corner or a half circle at the middle of an edge, on further rings when they do not
 * fit (`wheelLayout`, FR-027, FR-028). It folds when an entry is activated, on Escape and on a
 * click outside (FR-029), but stays open while an entry's menu or chooser is (they live outside
 * it). On a small screen the entries are bigger and further apart. Its one context menu offers the actions of the entry it was opened on, or edge,
 * alignment and style on the button (`DockMenu`). A menu of entries (FR-042): Enter or Space opens it, the arrow keys follow the entries,
 * Enter activates. The button carries a dot while any app wants attention.
 */
import {
  computed,
  inject,
  nextTick,
  onBeforeUnmount,
  provide,
  reactive,
  ref,
  useTemplateRef,
  watch,
} from 'vue'
import { useWindowSize } from '@vueuse/core'
import { DOCK_HOLD, dockEntryAt, useDockHold } from '~/composables/useDock'
import {
  dockItemKey,
  wheelLayout,
  WHEEL_METRICS,
  WHEEL_METRICS_COMPACT,
  type DockAlign,
  type DockEdge,
  type DockEntry,
} from '~/lib/wm/dock'

const props = defineProps<{
  entries: DockEntry[]
  edge: DockEdge
  align: DockAlign
}>()

const wm = useWindowManagerStore()
const { t } = useI18n()

const root = useTemplateRef<HTMLElement>('root')
const toggle = useTemplateRef<HTMLButtonElement>('toggle')
const overflowListRoot = useTemplateRef<HTMLDivElement>('overflowListRoot')
const { width: viewportWidth, height: viewportHeight } = useWindowSize()
const open = ref(false)
const focusIndex = ref(0)

/** The wheel's menu and the entries' choosers open right now; they live outside the wheel, so
 * while one is open a click there is no "click outside". Passed on to the dock as well. */
const holders = reactive(new Set<symbol>())
const holdDock = inject(DOCK_HOLD, () => {})
function hold(holder: symbol, isOpen: boolean) {
  holdDock(holder, isOpen)
  if (isOpen) {
    holders.add(holder)
    return
  }
  // A chooser closed after a choice, a menu after its action: the wheel's job is done too.
  if (holders.delete(holder) && holders.size === 0) close()
}
provide(DOCK_HOLD, hold)
onBeforeUnmount(() => {
  for (const holder of holders) holdDock(holder, false)
})

// The wheel's one context menu (only one menu open at a time): the entry it was opened on, or the
// button, which offers edge, alignment and style.
const menuEntry = ref<DockEntry | null>(null)
function pickMenuEntry(event: Event) {
  menuEntry.value = dockEntryAt(event.target, props.entries)
}
// A fanned-out wheel keeps a hiding dock visible (FR-025).
const holdFanned = useDockHold()
watch(open, (isOpen) => holdFanned(isOpen))
const wheelMenu = Symbol('dockWheelMenu')
function holdWheelMenu(isOpen: boolean) {
  hold(wheelMenu, isOpen)
}

// Switching between compact and normal mode moves the wheel to another corner (FR-034).
watch(
  () => wm.compact,
  () => close(),
)

const offsets = computed(() =>
  wheelLayout(
    props.entries.length,
    props.edge,
    props.align,
    wm.compact ? WHEEL_METRICS_COMPACT : WHEEL_METRICS,
  ),
)

/** Use a scrollable list when a compact wheel would put a target beyond the viewport. */
const overflowList = computed(() => {
  void viewportWidth.value
  void viewportHeight.value
  if (!wm.compact || !root.value) return false
  const bounds = root.value.getBoundingClientRect()
  const halfItem = WHEEL_METRICS_COMPACT.item / 2
  return offsets.value.some(({ x, y }) => {
    const centerX = bounds.left + bounds.width / 2 + x
    const centerY = bounds.top + bounds.height / 2 + y
    return (
      centerX - halfItem < 0 ||
      centerX + halfItem > window.innerWidth ||
      centerY - halfItem < 0 ||
      centerY + halfItem > window.innerHeight
    )
  })
})

const attention = computed(() =>
  props.entries.some(
    (entry) => entry.kind === 'app' && wm.appHasAttention(entry.appId),
  ),
)

function items(): HTMLElement[] {
  return [
    ...(root.value?.querySelectorAll<HTMLElement>('[data-dock-item]') ?? []),
  ]
}

async function show() {
  open.value = true
  focusIndex.value = 0
  await nextTick()
  if (!overflowList.value) items()[0]?.focus()
}

function close() {
  open.value = false
}

function onToggle() {
  if (open.value) close()
  else void show()
}

/** An activated entry folds the wheel, unless it opened its chooser, which then holds it. */
function onItemClick() {
  void nextTick(() => {
    if (holders.size === 0) close()
  })
}

onClickOutside(root, (event) => {
  if (
    overflowList.value &&
    event.target instanceof Node &&
    overflowListRoot.value?.contains(event.target)
  )
    return
  if (holders.size === 0) close()
})

function entryLabel(entry: DockEntry): string {
  if (entry.kind === 'control') return t(`wm.dock.controls.${entry.id}`)
  const app = wm.apps().find((candidate) => candidate.id === entry.appId)
  return app ? (app.title ?? t(app.titleKey)) : ''
}

function onKeydown(event: KeyboardEvent) {
  if (!open.value) return
  if (event.key === 'Escape') {
    event.preventDefault()
    close()
    toggle.value?.focus()
    return
  }
  const all = items()
  if (all.length === 0) return
  let next: number
  if (event.key === 'ArrowRight' || event.key === 'ArrowDown')
    next = (focusIndex.value + 1) % all.length
  else if (event.key === 'ArrowLeft' || event.key === 'ArrowUp')
    next = (focusIndex.value - 1 + all.length) % all.length
  else return
  event.preventDefault()
  focusIndex.value = next
  all[next]?.focus()
}
</script>

<template>
  <ShadcnContextMenu @update:open="holdWheelMenu">
    <ShadcnContextMenuTrigger as-child>
      <div
        ref="root"
        class="relative h-14 w-14"
        @keydown="onKeydown"
        @pointerdown.capture="pickMenuEntry"
        @contextmenu.capture="pickMenuEntry"
      >
        <div
          v-if="!overflowList"
          role="menu"
          :aria-label="t('wm.dock.label')"
          :aria-hidden="!open"
          class="absolute inset-0"
        >
          <div
            v-for="(entry, index) in entries"
            :key="dockItemKey(entry)"
            role="none"
            class="absolute left-1/2 top-1/2 rounded-full bg-background shadow-lg ring-1 ring-border transition-[transform,opacity] duration-200 motion-reduce:transition-none"
            :class="open ? 'opacity-100' : 'pointer-events-none opacity-0'"
            :style="{
              transform: open
                ? `translate(-50%, -50%) translate(${offsets[index]?.x ?? 0}px, ${offsets[index]?.y ?? 0}px)`
                : 'translate(-50%, -50%) scale(0.5)',
            }"
            @click="onItemClick"
          >
            <WmDockItem
              :entry="entry"
              :tabbable="open && index === focusIndex"
              :large="wm.compact"
            />
          </div>
        </div>
        <button
          ref="toggle"
          type="button"
          class="relative flex h-14 w-14 items-center justify-center rounded-full bg-foreground text-background shadow-lg hover:opacity-90"
          data-testid="dock-wheel-toggle"
          :aria-haspopup="overflowList ? 'dialog' : 'menu'"
          :aria-expanded="open"
          :aria-label="
            open ? t('wm.dock.wheel.close') : t('wm.dock.wheel.open')
          "
          :title="open ? t('wm.dock.wheel.close') : t('wm.dock.wheel.open')"
          @click="onToggle"
        >
          <Icon
            :name="open ? 'lucide:x' : 'lucide:circle-dot'"
            class="h-6 w-6"
            :aria-hidden="true"
          />
          <span
            v-if="attention && !open"
            class="absolute right-1 top-1 h-2 w-2 rounded-full bg-warning"
            :aria-label="t('wm.attention')"
          />
        </button>
        <UiDrawerModal
          v-if="overflowList"
          :open="open"
          :title="t('wm.dock.label')"
          @update:open="open = $event"
        >
          <template #content>
            <div
              ref="overflowListRoot"
              class="max-h-[70dvh] overflow-y-auto p-3"
              role="list"
              data-testid="dock-wheel-overflow-list"
            >
              <div
                v-for="(entry, index) in entries"
                :key="dockItemKey(entry)"
                role="listitem"
                class="flex items-center gap-3 border-b border-border py-2 last:border-0"
                @click="onItemClick"
              >
                <WmDockItem
                  :entry="entry"
                  :tabbable="open && index === focusIndex"
                  :large="true"
                />
                <span class="min-w-0 truncate text-sm">
                  {{ entryLabel(entry) }}
                </span>
              </div>
            </div>
          </template>
        </UiDrawerModal>
      </div>
    </ShadcnContextMenuTrigger>
    <ShadcnContextMenuContent class="min-w-48" data-testid="dock-menu">
      <WmDockMenu :entry="menuEntry" />
    </ShadcnContextMenuContent>
  </ShadcnContextMenu>
</template>
