<script setup lang="ts">
/**
 * The apps as a scrolling grid of tiles (spec 091-tab-switcher, FR-003, FR-003a, FR-022,
 * contracts/app-tiles.md), shared by the launcher and the "+" of a window. The grid scrolls
 * vertically as COSMIC's app library does, so 30 to 50 installed apps stay reachable without
 * pages or a swipe: its own element is the scroll container (bounded by whatever hosts it) and a
 * touch that moves scrolls and never picks a tile.
 *
 * The column count follows the grid's own width (`gridColumns`), so a window made smaller or
 * larger re-flows live; tiles are never shrunk below a touch target. The arrow keys move between
 * tiles by one or by a row (`moveFocus`), one tile is in the tab order at a time (roving tabindex)
 * and the focused tile is scrolled into view. `pinMenu` adds the launcher's right click / long
 * press menu that pins an app to the dock (spec 045 FR-017); `markOpen` says of a single-instance
 * app that it already has a tab somewhere (the "+" activates it instead of opening a second one,
 * spec 020).
 */
import type { AppDefinition } from '~/lib/wm/apps'
import { GAP, gridColumns, moveFocus } from '~/lib/wm/tileGrid'

const props = defineProps<{
  apps: readonly AppDefinition[]
  markOpen?: boolean
  pinMenu?: boolean
}>()

const emit = defineEmits<{ pick: [appId: string] }>()

const wm = useWindowManagerStore()
const dock = useDock()
const { t } = useI18n()

const root = useTemplateRef<HTMLElement>('root')
const { width } = useElementSize(root)
const columns = computed(() =>
  gridColumns(width.value, { compact: wm.compact }),
)

/** A single-instance app with a tab in any window: picking it shows that tab. */
function isOpenElsewhere(app: AppDefinition): boolean {
  return (
    !app.multiInstance &&
    wm.windows.some((w) => w.tabs.some((tab) => tab.appId === app.id))
  )
}

function togglePin(appId: string) {
  void (dock.isPinned(appId) ? dock.unpinAsync(appId) : dock.pinAsync(appId))
}

// --- Keyboard: one tile in the tab order, the arrows move it ---------------------------------

/** The tile that is in the tab order and gets the arrow keys' focus. */
const active = ref(0)

watch(
  () => props.apps.length,
  (count) => {
    if (active.value >= count) active.value = Math.max(0, count - 1)
  },
)

function tileAt(index: number): HTMLElement | null {
  return (
    root.value?.querySelector<HTMLElement>(`[data-tile-index="${index}"]`) ??
    null
  )
}

async function focusTile(index: number) {
  active.value = index
  await nextTick()
  const tile = tileAt(index)
  tile?.focus()
  tile?.scrollIntoView({ block: 'nearest' })
}

function onKeydown(event: KeyboardEvent) {
  if (event.altKey || event.ctrlKey || event.metaKey) return
  const rtl = root.value
    ? getComputedStyle(root.value).direction === 'rtl'
    : false
  const next = moveFocus(
    active.value,
    event.key,
    columns.value,
    props.apps.length,
    rtl,
  )
  if (next === null) return
  event.preventDefault()
  void focusTile(next)
}

/** A tile focused by Tab or a click becomes the one in the tab order. */
function onFocusin(event: FocusEvent) {
  const tile = (event.target as HTMLElement | null)?.closest<HTMLElement>(
    '[data-tile-index]',
  )
  if (tile) active.value = Number(tile.dataset.tileIndex)
}
</script>

<template>
  <div
    ref="root"
    class="min-h-0 overflow-y-auto overscroll-contain p-4"
    data-testid="app-tiles"
    @keydown="onKeydown"
    @focusin="onFocusin"
  >
    <div
      class="grid"
      role="group"
      :aria-label="t('wm.launcher.apps')"
      :style="{
        gridTemplateColumns: `repeat(${columns}, minmax(0, 1fr))`,
        gap: `${GAP}px`,
      }"
    >
      <template v-for="(app, index) in apps" :key="app.id">
        <ShadcnContextMenu v-if="pinMenu">
          <ShadcnContextMenuTrigger as-child>
            <WmAppTile
              :app="app"
              :open="markOpen && isOpenElsewhere(app)"
              :data-tile-index="index"
              :tabindex="index === active ? 0 : -1"
              @pick="emit('pick', $event)"
            />
          </ShadcnContextMenuTrigger>
          <ShadcnContextMenuContent class="min-w-48">
            <ShadcnContextMenuItem
              data-testid="launcher-menu-pin"
              @select="togglePin(app.id)"
            >
              {{ t(dock.isPinned(app.id) ? 'wm.dock.unpin' : 'wm.dock.pin') }}
            </ShadcnContextMenuItem>
          </ShadcnContextMenuContent>
        </ShadcnContextMenu>
        <WmAppTile
          v-else
          :app="app"
          :open="markOpen && isOpenElsewhere(app)"
          :data-tile-index="index"
          :tabindex="index === active ? 0 : -1"
          @pick="emit('pick', $event)"
        />
      </template>
    </div>
  </div>
</template>
