<script setup lang="ts">
/**
 * One entry of the dock (spec 045): a control (Launcher, window overview, workspace overview,
 * FR-014) or an app (FR-004, FR-009). An app shows whether it runs, how many instances it has and
 * whether it wants attention (FR-007, FR-008). A click opens it, brings its one instance to the
 * front, or offers its instances grouped by workspace (FR-010–FR-012); a middle click opens a new
 * one (FR-013). Its context menu is the bar's or the wheel's single one (`DockMenu`), which finds
 * the entry by `data-dock-key`.
 */
import { computed, ref, useTemplateRef, watch } from 'vue'
import { useDockHold } from '~/composables/useDock'
import {
  dockActivation,
  dockItemKey,
  type DockControlId,
  type DockEntry,
  type DockInstance,
} from '~/lib/wm/dock'

const props = defineProps<{
  entry: DockEntry
  /** Roving tab stop of the dock's toolbar (FR-041): only one entry is reached with Tab. */
  tabbable: boolean
}>()

const wm = useWindowManagerStore()
const { t } = useI18n()

const CONTROL_ICONS: Record<DockControlId, string> = {
  launcher: 'lucide:layout-grid',
  workspaces: 'lucide:monitor',
  windows: 'lucide:copy',
}
const CONTROL_ACTIONS = {
  launcher: useAction('wm.launcher.open'),
  workspaces: useAction('wm.workspaces.overview'),
  windows: useAction('wm.windows.overview'),
} satisfies Record<DockControlId, unknown>
const openApp = useAction('wm.app.open')
const activateTab = useAction('wm.tab.activate')

const app = computed(() => {
  const entry = props.entry
  if (entry.kind !== 'app') return undefined
  return wm.apps().find((candidate) => candidate.id === entry.appId)
})

const instances = computed<readonly DockInstance[]>(() =>
  props.entry.kind === 'app' ? props.entry.instances : [],
)

const label = computed(() => {
  if (props.entry.kind === 'control')
    return t(`wm.dock.controls.${props.entry.id}`)
  return app.value ? (app.value.title ?? t(app.value.titleKey)) : ''
})

const attention = computed(
  () => props.entry.kind === 'app' && wm.appHasAttention(props.entry.appId),
)

const testId = computed(() => {
  if (props.entry.kind === 'app') return `dock-item-${props.entry.appId}`
  return props.entry.id === 'launcher'
    ? 'open-launcher'
    : `dock-control-${props.entry.id}`
})

const button = useTemplateRef<HTMLButtonElement>('button')
const chooserOpen = ref(false)
// A hiding dock stays while this entry's chooser is open (FR-025).
const holdChooser = useDockHold()
watch(chooserOpen, (open) => holdChooser(open))
// Switching between compact and normal mode moves the dock away from the chooser (FR-034).
watch(
  () => wm.compact,
  () => {
    chooserOpen.value = false
  },
)
// An instance closed elsewhere leaves the chooser; with fewer than two there is nothing to choose.
watch(
  () => instances.value.length,
  (count) => {
    if (count < 2) chooserOpen.value = false
  },
)

function tabTitle(instance: DockInstance): string {
  const tab = wm.windows
    .find((window) => window.id === instance.windowId)
    ?.tabs.find((candidate) => candidate.id === instance.tabId)
  if (!tab) return label.value
  const info = wm.tabDisplayInfo(tab)
  return (
    info.titleOverride ??
    (info.titleKey ? t(info.titleKey, info.titleParams) : label.value)
  )
}

/** The instances by workspace, in workspace order, each titled like its tab. */
const groups = computed(() =>
  [...wm.workspaces]
    .sort((a, b) => a.position - b.position)
    .map((workspace) => ({
      id: workspace.id,
      label: t('wm.workspaces.numbered', { number: workspace.position + 1 }),
      rows: instances.value
        .filter((instance) => instance.workspaceId === workspace.id)
        .map((instance) => ({
          tabId: instance.tabId,
          title: tabTitle(instance),
        })),
    }))
    .filter((group) => group.rows.length > 0),
)

function newInstance() {
  if (props.entry.kind === 'app') void openApp({ appId: props.entry.appId })
}

function activate() {
  const entry = props.entry
  if (entry.kind === 'control') {
    void CONTROL_ACTIONS[entry.id]({})
    return
  }
  const activation = dockActivation(entry.instances)
  if (activation.kind === 'open') newInstance()
  else if (activation.kind === 'focus')
    void activateTab({ tabId: activation.tabId })
  else chooserOpen.value = true
}

function choose(tabId: string) {
  chooserOpen.value = false
  void activateTab({ tabId })
}

function chooseNew() {
  chooserOpen.value = false
  newInstance()
}

function onAuxclick(event: MouseEvent) {
  if (event.button !== 1) return
  event.preventDefault()
  if (app.value?.multiInstance) newInstance()
  else activate()
}
</script>

<template>
  <ShadcnPopover v-model:open="chooserOpen">
    <button
      ref="button"
      type="button"
      class="relative flex h-14 w-14 shrink-0 items-center justify-center rounded-2xl"
      :class="[
        entry.kind === 'control' && entry.id === 'launcher'
          ? 'bg-foreground text-background hover:opacity-90'
          : 'text-foreground hover:bg-accent',
        attention ? 'ring-2 ring-warning' : '',
      ]"
      :data-testid="testId"
      data-dock-item
      :data-dock-key="dockItemKey(entry)"
      :data-running="instances.length > 0 || undefined"
      :data-count="instances.length || undefined"
      :tabindex="tabbable ? 0 : -1"
      :aria-label="label"
      :title="label"
      @click="activate"
      @auxclick="onAuxclick"
      @mousedown.middle.prevent
    >
      <img
        v-if="app?.iconUrl"
        :src="app.iconUrl"
        alt=""
        class="h-11 w-11 object-contain"
        :class="app.iconUrl.startsWith('data:') ? '' : 'drop-shadow-sm'"
        draggable="false"
      />
      <Icon
        v-else
        :name="
          entry.kind === 'control' ? CONTROL_ICONS[entry.id] : (app?.icon ?? '')
        "
        class="h-6 w-6"
        :aria-hidden="true"
      />
      <span
        v-if="instances.length > 0"
        class="absolute bottom-0.5 left-1/2 h-1.5 w-1.5 -translate-x-1/2 rounded-full bg-foreground"
      />
      <span class="sr-only">
        <template v-if="instances.length > 1">
          {{ t('wm.dock.count', { count: instances.length }) }}
        </template>
        <template v-else-if="instances.length > 0">
          {{ t('wm.dock.running') }}
        </template>
        <template v-if="attention">{{ t('wm.attention') }}</template>
      </span>
      <span
        v-if="instances.length > 1"
        class="absolute right-0.5 top-0.5 min-w-4 rounded-full bg-primary px-1 text-[10px] leading-4 text-primary-foreground"
        aria-hidden="true"
      >
        {{ instances.length }}
      </span>
    </button>
    <!-- Anchored by `reference` to the entry's button. -->
    <ShadcnPopoverContent
      :reference="button ?? undefined"
      class="flex w-64 flex-col gap-2 p-2"
      data-testid="dock-instances"
      :aria-label="t('wm.dock.instances')"
    >
      <div v-for="group in groups" :key="group.id" class="flex flex-col">
        <span class="px-2 py-1 text-xs text-muted-foreground">
          {{ group.label }}
        </span>
        <button
          v-for="row in group.rows"
          :key="row.tabId"
          type="button"
          class="truncate rounded-md px-2 py-1.5 text-left text-sm hover:bg-accent"
          :data-testid="`dock-instance-${row.tabId}`"
          @click="choose(row.tabId)"
        >
          {{ row.title }}
        </button>
      </div>
      <button
        type="button"
        class="flex items-center gap-2 rounded-md border-t px-2 py-1.5 text-left text-sm hover:bg-accent"
        data-testid="dock-instances-new"
        @click="chooseNew"
      >
        <Icon name="lucide:plus" class="h-4 w-4" :aria-hidden="true" />
        {{ t('wm.dock.newWindow') }}
      </button>
    </ShadcnPopoverContent>
  </ShadcnPopover>
</template>
