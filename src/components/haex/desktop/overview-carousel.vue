<template>
  <Transition name="fade">
    <div
      v-if="windowManager.showWindowOverview"
      class="absolute inset-0 z-9997 flex flex-col"
      :style="{ paddingLeft: isOverviewMode && !isSmallScreen ? '400px' : '0' }"
    >
      <!-- Backdrop to close overview on click -->
      <div
        class="absolute inset-0 -z-10 bg-black/30 backdrop-blur-sm transition-all duration-300"
        @click="windowManager.showWindowOverview = false"
      />

      <!-- Window Overview Grid -->
      <div class="flex-1 flex items-start justify-center px-4 py-16 overflow-auto">
        <div
          v-if="currentWorkspaceWindows.length > 0"
          class="flex flex-row flex-wrap gap-6 justify-center items-center content-center"
        >
          <div
            v-for="window in currentWorkspaceWindows"
            :key="window.id"
            class="relative group cursor-pointer"
            @click="emit('window-click', window.id)"
            @mousedown="(event) => emit('window-mousedown', event, window.id)"
          >
            <!-- Window Preview Card -->
            <div
              class="relative bg-gray-800/80 rounded-xl overflow-hidden border-2 border-gray-600 group-hover:border-primary-500 transition-all shadow-2xl flex flex-col items-center justify-center gap-3 p-4"
              :class="{
                'opacity-50': windowManager.draggingWindowId === window.id,
              }"
              :style="getCarouselWindowStyle()"
            >
              <!-- Window Icon -->
              <HaexIcon
                :name="window.icon || 'i-lucide-app-window'"
                class="size-12 text-gray-300"
              />

              <!-- Window Title -->
              <span class="font-medium text-sm text-gray-200 truncate max-w-full text-center">{{ window.type === 'system' ? windowManager.getLocalizedSystemWindowName(window.sourceId) : window.title }}</span>

              <!-- Badges (top right corner) -->
              <div class="absolute top-2 right-2 flex flex-col gap-1 items-end">
                <!-- Native WebView Badge -->
                <UBadge
                  v-if="window.isNativeWebview"
                  color="neutral"
                >
                  Separates Fenster
                </UBadge>

                <!-- Minimized Badge -->
                <UBadge
                  v-if="window.isMinimized"
                  color="info"
                >
                  Minimiert
                </UBadge>
              </div>

              <!-- Hover Overlay -->
              <div class="absolute inset-0 bg-primary-500/0 group-hover:bg-primary-500/10 transition-colors" />
            </div>
          </div>
        </div>

        <!-- Empty State -->
        <div
          v-else
          class="flex flex-col items-center justify-center text-white/70"
        >
          <UIcon
            name="i-heroicons-window"
            class="size-16 mb-4"
          />
          <p class="text-lg font-medium">Keine Fenster geöffnet</p>
          <p class="text-sm opacity-70">
            Öffne eine Erweiterung, um sie hier zu sehen
          </p>
        </div>
      </div>

      <!-- Drag ghost (follows mouse while dragging) -->
      <div
        v-if="windowManager.draggingWindowId && draggedWindowInfo"
        class="fixed z-10000 pointer-events-none"
        :style="{
          left: `${dragGhostPosition.x}px`,
          top: `${dragGhostPosition.y}px`,
          transform: 'translate(-50%, -50%)',
        }"
      >
        <div class="bg-elevated/90 backdrop-blur-sm rounded-lg shadow-2xl border border-primary-500 px-4 py-3 flex items-center gap-3">
          <UIcon
            v-if="draggedWindowInfo.icon"
            :name="draggedWindowInfo.icon"
            class="size-6 shrink-0"
          />
          <span class="font-medium text-sm">{{ draggedWindowInfo.title }}</span>
        </div>
      </div>
    </div>
  </Transition>
</template>

<script setup lang="ts">
type WindowInfo = ReturnType<typeof useWindowManagerStore>['windows'][number]

defineProps<{
  currentWorkspaceWindows: WindowInfo[]
  draggedWindowInfo: WindowInfo | null | undefined
  dragGhostPosition: { x: number; y: number }
}>()

const emit = defineEmits<{
  'window-click': [windowId: string]
  'window-mousedown': [event: MouseEvent, windowId: string]
}>()

const windowManager = useWindowManagerStore()
const workspaceStore = useWorkspaceStore()
const uiStore = useUiStore()

const { isOverviewMode } = storeToRefs(workspaceStore)
const { isSmallScreen } = storeToRefs(uiStore)

// Calculate window preview size for carousel
const getCarouselWindowStyle = () => {
  // Fixed card sizes for consistent appearance
  const width = isSmallScreen.value ? 200 : 240
  const height = isSmallScreen.value ? 160 : 180

  return {
    width: `${width}px`,
    height: `${height}px`,
  }
}
</script>

<style scoped>
.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.3s ease;
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}
</style>
