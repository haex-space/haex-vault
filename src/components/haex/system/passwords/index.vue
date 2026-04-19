<template>
  <HaexSystem
    :is-dragging="isDragging"
    disable-content-scroll
  >
    <div class="h-full flex flex-col overflow-hidden">
      <HaexSystemPasswordsHeader class="flex-none" />
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
        <main class="flex-1 min-w-0 overflow-hidden">
          <component :is="currentView" />
        </main>
      </div>
    </div>
  </HaexSystem>
</template>

<script setup lang="ts">
defineProps<{
  tabId?: string
  isDragging?: boolean
}>()

const passwordsStore = usePasswordsStore()
const { viewMode } = storeToRefs(passwordsStore)
const toast = useToast()
const { t } = useI18n()

const currentView = computed(() =>
  viewMode.value === 'itemDetail'
    ? 'HaexSystemPasswordsDetails'
    : 'HaexSystemPasswordsList',
)

onMounted(async () => {
  try {
    await passwordsStore.loadItemsAsync()
  } catch (error) {
    console.error('[Passwords] Failed to load items:', error)
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

