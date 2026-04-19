<template>
  <HaexSystem
    :is-dragging="isDragging"
    disable-content-scroll
  >
    <div class="h-full flex flex-col overflow-hidden">
      <HaexSystemPasswordsHeader
        v-if="viewMode === 'list'"
        class="flex-none"
      />
      <div class="flex-1 min-h-0 flex">
        <aside
          class="hidden @3xl:flex @3xl:flex-col shrink-0 overflow-y-auto border-e border-default"
          :style="{ width: `${sidebarWidth}px` }"
        >
          <HaexSystemPasswordsSidebar />
        </aside>
        <div
          class="hidden @3xl:block w-1 shrink-0 cursor-col-resize hover:bg-primary/40 active:bg-primary/60 transition-colors"
          :class="{ 'bg-primary/60': isResizing }"
          @mousedown="startResize"
          @dblclick="sidebarWidth = DEFAULT_SIDEBAR_WIDTH"
        />
        <main class="flex-1 min-w-0 overflow-hidden flex flex-col">
          <HaexSystemPasswordsEditor
            v-if="viewMode === 'item'"
            :key="selectedItemId ?? 'new'"
          />
          <template v-else>
            <HaexSystemPasswordsBreadcrumb class="@3xl:hidden shrink-0" />
            <HaexSystemPasswordsList class="flex-1 min-h-0" />
          </template>
        </main>
      </div>
    </div>
  </HaexSystem>
</template>

<script setup lang="ts">
const props = defineProps<{
  tabId?: string
  isDragging?: boolean
}>()

// Provide the tab ID so per-tab back/forward stacks stay isolated
// (same pattern the settings view uses).
provide('haex-tab-id', props.tabId ?? '')

const passwordsStore = usePasswordsStore()
const groupsStore = usePasswordsGroupsStore()
const { viewMode, selectedItemId } = storeToRefs(passwordsStore)
const toast = useToast()
const { t } = useI18n()

const { armWindowCloseBoundary } = usePasswordsNavigation(props.tabId ?? '')

onMounted(async () => {
  // Self-rearming sentinel — a back press from the list view is absorbed
  // instead of reaching the global close-window action.
  armWindowCloseBoundary()

  try {
    await Promise.all([
      passwordsStore.loadItemsAsync(),
      groupsStore.loadGroupsAsync(),
    ])
  } catch (error) {
    console.error('[Passwords] Failed to load items/groups:', error)
    toast.add({
      title: t('loadError'),
      color: 'error',
      icon: 'i-lucide-alert-triangle',
    })
  }
})

const DEFAULT_SIDEBAR_WIDTH = 256
const MIN_SIDEBAR_WIDTH = 180
const MAX_SIDEBAR_WIDTH = 480

const sidebarWidth = ref(DEFAULT_SIDEBAR_WIDTH)
const isResizing = ref(false)

const startResize = (event: MouseEvent) => {
  event.preventDefault()
  const startX = event.clientX
  const startWidth = sidebarWidth.value
  isResizing.value = true

  const onMove = (e: MouseEvent) => {
    const next = startWidth + (e.clientX - startX)
    sidebarWidth.value = Math.min(
      MAX_SIDEBAR_WIDTH,
      Math.max(MIN_SIDEBAR_WIDTH, next),
    )
  }

  const onUp = () => {
    isResizing.value = false
    document.removeEventListener('mousemove', onMove)
    document.removeEventListener('mouseup', onUp)
  }

  document.addEventListener('mousemove', onMove)
  document.addEventListener('mouseup', onUp)
}
</script>

<i18n lang="yaml">
de:
  loadError: Passwörter konnten nicht geladen werden
en:
  loadError: Failed to load passwords
</i18n>

