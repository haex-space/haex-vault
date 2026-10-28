<script setup lang="ts">
/**
 * The Firefox-style tab strip (spec 015-workspace-shell, T034/T035/T037,
 * plan research R18, FR-031/032/033/035/036/038): one tab shows icon+title
 * with no tab frame (FR-031); two or more show a full ARIA tab-list with a
 * close button per tab — unless compact (FR-036), which always collapses
 * to the active tab's title regardless of count, dropping the tablist and
 * its scroll arrows entirely. With more than one tab, the collapsed title
 * keeps its own close button so a tab can be closed without closing the
 * whole window (only the window's close remains for a single tab, same as
 * the non-collapsed FR-031 case). The "+" sits immediately after the last
 * tab (FR-032) either way, wrapped in `wm/NewTabMenu.vue`'s dropdown.
 * Activating a tab scrolls it into view (FR-035). Like Firefox, tabs take up
 * to 14rem each and shrink evenly to 6rem before the strip scrolls.
 * The active tab (spec 040-active-tab-emphasis) gets a raised surface, a
 * heavier title and an accent bar at its lower edge, so it is not told
 * apart by colour alone. Its focus ring is inset: the scrolling tablist
 * clips anything drawn outside a tab, which hid the outline before.
 */
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import type { WmTab } from '~/lib/wm/types'

const props = defineProps<{
  windowId: string
  tabs: WmTab[]
  activeTabId: string
  compact: boolean
}>()

const emit = defineEmits<{
  selectTab: [tabId: string]
  closeTab: [tabId: string]
}>()

const wm = useWindowManagerStore()
const { t } = useI18n()

const tabRefs = new Map<string, HTMLElement>()
/** Tracks mounted tab elements for keyboard focus and scroll positioning. */
function setTabRef(tabId: string, el: Element | null) {
  if (el instanceof HTMLElement) tabRefs.set(tabId, el)
  else tabRefs.delete(tabId)
}

type TabInfo = ReturnType<typeof wm.tabDisplayInfo>

/** Prefers a tab's runtime title over its translated app title. */
function titleFrom(info: TabInfo): string {
  return (
    info.titleOverride ??
    (info.titleKey ? t(info.titleKey, info.titleParams) : '')
  )
}

// Collapses to a plain title (no tab frame) when there is exactly one tab (FR-031) or, regardless
// of count, whenever the window manager is compact (FR-036) — using the *active* tab in that case, since a
// compact window has no visible tablist to pick a tab from otherwise.
const collapsedTab = computed(() => {
  const tab = props.compact
    ? (props.tabs.find((t) => t.id === props.activeTabId) ?? props.tabs[0])
    : props.tabs.length === 1
      ? props.tabs[0]
      : undefined
  return tab ? { tab, info: wm.tabDisplayInfo(tab) } : undefined
})

const tabRows = computed(() =>
  props.tabs.map((tab) => ({ tab, info: wm.tabDisplayInfo(tab) })),
)

/** Sends the selected tab id to the window that owns the tab state. */
function select(tabId: string) {
  emit('selectTab', tabId)
}

/** Roving-tabindex ARIA tab-list navigation: arrows move focus/selection, Home/End jump to the
 * ends, Enter/Space activates (FR-038) — this element is a `role="tab"` `<div>`, not a native
 * `<button>`, so activation keys need explicit handling. */
function onKeydown(event: KeyboardEvent, index: number) {
  if (event.target !== event.currentTarget) return
  const count = props.tabs.length
  if (event.key === 'Enter' || event.key === ' ') {
    event.preventDefault()
    const tab = props.tabs[index]
    if (tab) select(tab.id)
    return
  }
  let targetIndex: number | null = null
  if (event.key === 'ArrowRight') targetIndex = (index + 1) % count
  else if (event.key === 'ArrowLeft') targetIndex = (index - 1 + count) % count
  else if (event.key === 'Home') targetIndex = 0
  else if (event.key === 'End') targetIndex = count - 1
  else return
  event.preventDefault()
  const target = props.tabs[targetIndex]
  if (target) {
    select(target.id)
    tabRefs.get(target.id)?.focus()
  }
}

watch(
  () => props.activeTabId,
  (tabId) => {
    void nextTick(() => {
      tabRefs
        .get(tabId)
        ?.scrollIntoView({ inline: 'nearest', block: 'nearest' })
    })
  },
  { immediate: true },
)

const barRef = ref<HTMLElement | null>(null)
const overflowing = ref(false)
let observer: ResizeObserver | null = null

/** Shows scroll controls only when the tab strip exceeds its visible width. */
function updateOverflow() {
  const el = barRef.value
  overflowing.value = !!el && el.scrollWidth > el.clientWidth + 1
}

/** Scrolls the tab strip without changing the active tab. */
function scrollBy(amount: number) {
  barRef.value?.scrollBy({ left: amount, behavior: 'smooth' })
}

onMounted(() => {
  updateOverflow()
  observer = new ResizeObserver(updateOverflow)
  if (barRef.value) observer.observe(barRef.value)
})

onBeforeUnmount(() => {
  observer?.disconnect()
})

watch(
  () => props.tabs.length,
  () => void nextTick(updateOverflow),
)
</script>

<template>
  <div class="flex min-w-0 flex-1 items-center self-stretch">
    <button
      v-if="!collapsedTab && overflowing"
      type="button"
      class="shrink-0 rounded p-1 text-muted-foreground hover:bg-accent hover:text-foreground"
      :aria-label="t('wm.tabs.scrollLeft')"
      @pointerdown.stop
      @click.stop="scrollBy(-120)"
    >
      <Icon
        name="lucide:chevron-left"
        class="h-5 w-5"
        :aria-hidden="true"
      />
    </button>

    <div
      v-if="collapsedTab"
      class="flex min-w-0 items-center gap-2 px-1.5 py-1.5 text-base font-medium"
    >
      <WmTabIcon
        :icon="collapsedTab.info.icon"
        :icon-url="collapsedTab.info.iconUrl"
        class="h-5 w-5 shrink-0"
      />
      <span class="min-w-0 truncate">{{ titleFrom(collapsedTab.info) }}</span>
      <WmDevBadge v-if="collapsedTab.info.dev" />
      <span
        v-if="collapsedTab.info.hasAttention"
        class="h-1.5 w-1.5 shrink-0 rounded-full bg-warning"
        :aria-label="t('wm.attention')"
      />
      <button
        v-if="tabs.length > 1"
        type="button"
        data-testid="tab-close"
        class="shrink-0 rounded p-0.5 hover:bg-foreground/10"
        :aria-label="t('wm.tabs.close')"
        @pointerdown.stop
        @click.stop="emit('closeTab', collapsedTab.tab.id)"
      >
        <Icon name="lucide:x" class="h-4 w-4" :aria-hidden="true" />
      </button>
    </div>

    <div
      v-else
      ref="barRef"
      role="tablist"
      class="flex min-w-0 items-stretch gap-0.5 self-stretch overflow-x-auto"
      :aria-label="t('wm.tabs.tablist')"
    >
      <div
        v-for="(row, index) in tabRows"
        :key="row.tab.id"
        :ref="(el) => setTabRef(row.tab.id, el as Element | null)"
        role="tab"
        :aria-selected="row.tab.id === activeTabId"
        :tabindex="row.tab.id === activeTabId ? 0 : -1"
        class="flex min-w-48 basis-56 cursor-pointer items-center gap-2 rounded-t-md border-b-[3px] border-transparent px-2.5 py-1.5 text-base focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset"
        :class="
          row.tab.id === activeTabId
            ? 'border-primary bg-background font-semibold text-foreground shadow-sm'
            : 'text-muted-foreground hover:bg-accent'
        "
        @pointerdown.stop
        @click="select(row.tab.id)"
        @keydown="onKeydown($event, index)"
      >
        <WmTabIcon
          :icon="row.info.icon"
          :icon-url="row.info.iconUrl"
          class="h-5 w-5 shrink-0"
        />
        <span class="min-w-0 flex-1 truncate">{{ titleFrom(row.info) }}</span>
        <WmDevBadge v-if="row.info.dev" />
        <span
          v-if="row.info.hasAttention"
          class="h-1.5 w-1.5 shrink-0 rounded-full bg-warning"
          :aria-label="t('wm.attention')"
        />
        <button
          type="button"
          class="shrink-0 rounded p-0.5 hover:bg-foreground/10"
          :aria-label="t('wm.tabs.close')"
          @click.stop="emit('closeTab', row.tab.id)"
        >
          <Icon name="lucide:x" class="h-4 w-4" :aria-hidden="true" />
        </button>
      </div>
    </div>

    <button
      v-if="!collapsedTab && overflowing"
      type="button"
      class="shrink-0 rounded p-1 text-muted-foreground hover:bg-accent hover:text-foreground"
      :aria-label="t('wm.tabs.scrollRight')"
      @pointerdown.stop
      @click.stop="scrollBy(120)"
    >
      <Icon
        name="lucide:chevron-right"
        class="h-5 w-5"
        :aria-hidden="true"
      />
    </button>

    <WmNewTabMenu :window-id="windowId">
      <button
        type="button"
        data-testid="wm-new-tab"
        class="shrink-0 rounded p-1 text-muted-foreground hover:bg-accent hover:text-foreground"
        :aria-label="t('wm.tabs.newTab')"
        @pointerdown.stop
      >
        <Icon name="lucide:plus" class="h-5 w-5" :aria-hidden="true" />
      </button>
    </WmNewTabMenu>
  </div>
</template>
