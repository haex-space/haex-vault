<script setup lang="ts">
/**
 * What the dock's one context menu shows (spec 045): for an app, pin or unpin, a new window and close
 * all (FR-015, FR-016); for the workspaces, windows and assistant controls, removing them (US4,
 * spec 066 FR-001), or keeping the assistant's toggle that shows only while it is on; for the free
 * area and the launcher, which has nothing of its own, edge, alignment, style and mode (FR-018). The
 * bar and the wheel each hold a single menu and pass the entry it was opened on, so only one menu
 * is ever open. Goes inside a `ShadcnContextMenuContent`.
 */
import { computed } from 'vue'
import { dockItemKey, type DockEntry } from '~/lib/wm/dock'

const props = defineProps<{
  /** The entry the menu was opened on; `null` for the free area. */
  entry: DockEntry | null
}>()

const wm = useWindowManagerStore()
const dock = useDock()
const { t } = useI18n()
const openApp = useAction('wm.app.open')
const closeTab = useAction('wm.tab.close')

const app = computed(() => {
  const entry = props.entry
  if (entry?.kind !== 'app') return undefined
  return wm.apps().find((candidate) => candidate.id === entry.appId)
})

// The launcher cannot leave the dock; it shows the placement menu instead.
const ownMenu = computed(
  () =>
    props.entry !== null &&
    !(props.entry.kind === 'control' && props.entry.id === 'launcher'),
)

/** Whether the control the menu was opened on is stored; the assistant's toggle also shows while it
 * is on without being stored. */
const stored = computed(() => {
  const entry = props.entry
  if (entry === null) return false
  const key = dockItemKey(entry)
  return dock.items.value.some((item) => dockItemKey(item) === key)
})

function keepControl() {
  if (props.entry?.kind === 'control') void dock.addControlAsync(props.entry.id)
}

function togglePin() {
  if (props.entry?.kind !== 'app') return
  const appId = props.entry.appId
  void (props.entry.pinned ? dock.unpinAsync(appId) : dock.pinAsync(appId))
}

function newInstance() {
  if (props.entry?.kind === 'app') void openApp({ appId: props.entry.appId })
}

/** Workspaces, windows and the assistant can leave the dock; the launcher cannot (FR-006, US4). */
function removeControl() {
  if (props.entry?.kind !== 'control' || props.entry.id === 'launcher') return
  const key = dockItemKey(props.entry)
  const index = dock.items.value.findIndex((item) => dockItemKey(item) === key)
  if (index >= 0) void dock.removeAsync(index)
}

/** Closes every instance as a click on its tab's close button would, guards included (FR-016);
 * stops at the first one the user keeps open. */
async function closeAll() {
  if (props.entry?.kind !== 'app') return
  for (const instance of [...props.entry.instances]) {
    const outcome = await closeTab({ tabId: instance.tabId })
    if (!outcome.ok) return
  }
}
</script>

<template>
  <WmDockPlacementMenu v-if="!ownMenu" />
  <template v-else-if="entry?.kind === 'control'">
    <ShadcnContextMenuItem
      v-if="stored"
      data-testid="dock-menu-remove"
      @select="removeControl"
    >
      {{ t('wm.dock.remove') }}
    </ShadcnContextMenuItem>
    <ShadcnContextMenuItem
      v-else
      data-testid="dock-menu-pin"
      @select="keepControl"
    >
      {{ t('wm.dock.pin') }}
    </ShadcnContextMenuItem>
  </template>
  <template v-else-if="entry?.kind === 'app'">
    <ShadcnContextMenuItem data-testid="dock-menu-pin" @select="togglePin">
      {{ entry.pinned ? t('wm.dock.unpin') : t('wm.dock.pin') }}
    </ShadcnContextMenuItem>
    <ShadcnContextMenuItem
      v-if="app?.multiInstance"
      data-testid="dock-menu-new"
      @select="newInstance"
    >
      {{ t('wm.dock.newWindow') }}
    </ShadcnContextMenuItem>
    <ShadcnContextMenuItem
      v-if="entry.instances.length > 0"
      data-testid="dock-menu-close-all"
      @select="closeAll"
    >
      {{ t('wm.dock.closeAll') }}
    </ShadcnContextMenuItem>
  </template>
</template>
