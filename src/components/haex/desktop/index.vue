<template>
  <div
    ref="desktopEl"
    class="absolute inset-0 overflow-hidden"
  >
    <Swiper
      :modules="[SwiperNavigation]"
      :slides-per-view="1"
      :space-between="0"
      :initial-slide="currentWorkspaceIndex"
      :speed="300"
      :touch-angle="45"
      :no-swiping="true"
      no-swiping-class="no-swipe"
      :allow-touch-move="false"
      class="h-full w-full"
      direction="vertical"
      @swiper="onSwiperInit"
      @slide-change="onSlideChange"
    >
      <SwiperSlide
        v-for="workspace in workspaces"
        :key="workspace.id"
        class="w-full h-full"
      >
        <UContextMenu :items="getWorkspaceContextMenuItems(workspace.id)">
          <HaexDesktopWorkspaceDropZone
            :workspace-id="workspace.id"
            :background-style="getWorkspaceBackgroundStyle(workspace)"
            @desktop-click="handleDesktopClick"
            @area-select-start="handleAreaSelectStart"
            @drag-over="handleDragOver"
            @drop="handleDrop($event, workspace.id)"
          >
            <!-- Drop Target Zone (visible during drag) -->
            <div
              v-if="dropTargetZone"
              class="absolute border-2 border-blue-500 bg-blue-500/10 rounded-lg pointer-events-none z-10 transition-all duration-75"
              :style="{
                left: `${dropTargetZone.x}px`,
                top: `${dropTargetZone.y}px`,
                width: `${dropTargetZone.width}px`,
                height: `${dropTargetZone.height}px`,
              }"
            />

            <!-- Snap Dropzones (only visible when window drag near edge) -->

            <div
              class="absolute left-0 top-0 bottom-0 border-blue-500 pointer-events-none backdrop-blur-sm z-50 transition-all duration-500 ease-in-out"
              :class="
                showLeftSnapZone ? 'w-1/2 bg-blue-500/20 border-2' : 'w-0'
              "
            />

            <div
              class="absolute right-0 top-0 bottom-0 border-blue-500 pointer-events-none backdrop-blur-sm z-50 transition-all duration-500 ease-in-out"
              :class="
                showRightSnapZone ? 'w-1/2 bg-blue-500/20 border-2' : 'w-0'
              "
            />

            <!-- Area Selection Box -->
            <div
              v-if="isAreaSelecting"
              class="absolute bg-blue-500/20 border-2 border-blue-500 pointer-events-none z-30"
              :style="selectionBoxStyle"
            />

            <!-- Icons for this workspace -->
            <HaexDesktopIcon
              v-for="item in getWorkspaceIcons(workspace.id)"
              :id="item.id"
              :key="item.id"
              :item-type="item.itemType"
              :reference-id="item.referenceId"
              :initial-x="item.positionX"
              :initial-y="item.positionY"
              :label="item.label"
              :icon="item.icon"
              class="no-swipe"
              @position-changed="handlePositionChanged"
              @drag-start="handleDragStart"
              @dragging="handleDragging"
              @drag-end="handleDragEnd"
              @request-uninstall="handleRequestUninstall"
            />

            <!-- Windows for this workspace - single instance, CSS-transformed in overview -->
            <HaexWindow
              v-for="window in getWorkspaceWindows(workspace.id)"
              v-show="windowManager.showWindowOverview || !window.isMinimized"
              :id="window.id"
              :key="window.id"
              v-model:x="window.x"
              v-model:y="window.y"
              v-model:width="window.width"
              v-model:height="window.height"
              :title="window.title"
              :icon="window.icon"
              :is-active="windowManager.isWindowActive(window.id)"
              :source-x="window.sourceX"
              :source-y="window.sourceY"
              :source-width="window.sourceWidth"
              :source-height="window.sourceHeight"
              :is-opening="window.isOpening"
              :is-closing="window.isClosing"
              :warning-level="
                window.type === 'extension' &&
                availableExtensions.find(
                  (ext) => ext.id === window.sourceId,
                )?.devServerUrl
                  ? 'warning'
                  : undefined
              "
              class="no-swipe"
              :class="{
                'transition-opacity duration-300': !window.isNativeWebview,
                'opacity-0 pointer-events-none': windowManager.showWindowOverview && !window.isNativeWebview,
                'invisible': windowManager.showWindowOverview && window.isNativeWebview,
              }"
              @close="windowManager.closeWindow(window.id)"
              @minimize="windowManager.minimizeWindow(window.id)"
              @activate="windowManager.activateWindow(window.id)"
              @position-changed="
                (x, y) => windowManager.updateWindowPosition(window.id, x, y)
              "
              @size-changed="
                (width, height) =>
                  windowManager.updateWindowSize(window.id, width, height)
              "
              @drag-start="handleWindowDragStart(window.id)"
              @drag-end="handleWindowDragEnd"
            >
              <!-- System Window: Render Vue Component -->
              <component
                :is="getSystemWindowComponent(window.sourceId)"
                v-if="window.type === 'system'"
                :is-dragging="windowManager.draggingWindowId === window.id"
                :window-params="window.params"
              />

              <!-- Native WebView: Show icon placeholder (actual content is in separate OS window) -->
              <div
                v-else-if="window.isNativeWebview"
                class="w-full h-full flex items-center justify-center bg-gray-50 dark:bg-gray-900"
              >
                <HaexIcon
                  :name="window.icon || 'i-lucide-app-window'"
                  class="size-20"
                />
              </div>

              <!-- Extension Window: Render iFrame -->
              <HaexDesktopExtensionFrame
                v-else
                :extension-id="window.sourceId"
                :window-id="window.id"
              />
            </HaexWindow>
          </HaexDesktopWorkspaceDropZone>
        </UContextMenu>
      </SwiperSlide>
    </Swiper>

    <!-- Window Overview: Carousel -->
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
        <div class="flex-1 flex items-center justify-center p-4 py-8 overflow-auto">
          <div
            v-if="currentWorkspaceWindows.length > 0"
            class="flex flex-row flex-wrap gap-6 justify-center items-center content-center"
          >
            <div
              v-for="window in currentWorkspaceWindows"
              :key="window.id"
              class="relative group cursor-pointer"
              @click="handleOverviewWindowClick(window.id)"
              @mousedown="handleOverviewMouseDown($event, window.id)"
            >
              <!-- Window Preview Card -->
              <div
                class="relative bg-gray-800/80 rounded-xl overflow-hidden border-2 border-gray-600 group-hover:border-primary-500 transition-all shadow-2xl flex flex-col items-center justify-center gap-3 p-4"
                :class="{
                  'opacity-50': windowManager.draggingWindowId === window.id,
                }"
                :style="getCarouselWindowStyle(window)"
              >
                <!-- Window Icon -->
                <HaexIcon
                  :name="window.icon || 'i-lucide-app-window'"
                  class="size-12 text-gray-300"
                />

                <!-- Window Title -->
                <span class="font-medium text-sm text-gray-200 truncate max-w-full text-center">{{ window.title }}</span>

                <!-- Badges (top right corner) -->
                <div class="absolute top-2 right-2 flex flex-col gap-1 items-end">
                  <!-- Native WebView Badge -->
                  <UBadge
                    v-if="window.isNativeWebview"
                    color="neutral"
                    size="xs"
                  >
                    Separates Fenster
                  </UBadge>

                  <!-- Minimized Badge -->
                  <UBadge
                    v-if="window.isMinimized"
                    color="info"
                    size="xs"
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

    <!-- Extension Remove Dialog -->
    <HaexExtensionDialogRemove
      v-model:open="showRemoveDialog"
      :extension="extensionToRemove"
      @confirm="handleRemoveExtension"
    />
  </div>
</template>

<script setup lang="ts">
import { Swiper, SwiperSlide } from 'swiper/vue'
import { Navigation } from 'swiper/modules'
import { invoke } from '@tauri-apps/api/core'
import type { Swiper as SwiperType } from 'swiper'
import type { IHaexSpaceExtension } from '~/types/haexspace'
import 'swiper/css'
import 'swiper/css/navigation'

const SwiperNavigation = Navigation

const route = useRoute()
const desktopStore = useDesktopStore()
const extensionsStore = useExtensionsStore()
const windowManager = useWindowManagerStore()
const workspaceStore = useWorkspaceStore()
const vaultSettingsStore = useVaultSettingsStore()
const uiStore = useUiStore()

// Check if this is a remote sync vault (initial connection)
const isRemoteSyncVault = computed(() => route.query.remoteSync === 'true')
const { desktopItems } = storeToRefs(desktopStore)
const { availableExtensions } = storeToRefs(extensionsStore)
const { isSmallScreen } = storeToRefs(uiStore)
const {
  currentWorkspace,
  currentWorkspaceIndex,
  workspaces,
  swiperInstance,
  allowSwipe,
  isOverviewMode,
} = storeToRefs(workspaceStore)
const { getWorkspaceBackgroundStyle, getWorkspaceContextMenuItems } =
  workspaceStore

const desktopEl = useTemplateRef('desktopEl')

// Extension uninstall dialog state
const showRemoveDialog = ref(false)
const extensionToRemove = ref<IHaexSpaceExtension | undefined>(undefined)

const handleRequestUninstall = (extensionId: string) => {
  const extension = extensionsStore.availableExtensions.find(
    (ext) => ext.id === extensionId,
  )

  if (extension) {
    extensionToRemove.value = extension
    showRemoveDialog.value = true
  }
}

const handleRemoveExtension = async (deleteMode: 'device' | 'complete') => {
  if (!extensionToRemove.value) return

  try {
    // Uninstall extension (handles dev/regular, removes desktop items, reloads list)
    await extensionsStore.uninstallExtensionAsync(extensionToRemove.value.id, deleteMode)
  } catch (error) {
    console.error('Failed to remove extension:', error)
  }
}

// Track desktop viewport size reactively
const { width: viewportWidth, height: viewportHeight } =
  useElementSize(desktopEl)

// Provide viewport size to child windows
provide('viewportSize', {
  width: viewportWidth,
  height: viewportHeight,
})

// Area selection state
const isAreaSelecting = ref(false)
const selectionStart = ref({ x: 0, y: 0 })
const selectionEnd = ref({ x: 0, y: 0 })

const selectionBoxStyle = computed(() => {
  const x1 = Math.min(selectionStart.value.x, selectionEnd.value.x)
  const y1 = Math.min(selectionStart.value.y, selectionEnd.value.y)
  const x2 = Math.max(selectionStart.value.x, selectionEnd.value.x)
  const y2 = Math.max(selectionStart.value.y, selectionEnd.value.y)

  return {
    left: `${x1}px`,
    top: `${y1}px`,
    width: `${x2 - x1}px`,
    height: `${y2 - y1}px`,
  }
})

// Drag state for desktop icons
const isDragging = ref(false)
const currentDraggedItem = reactive({
  id: '',
  itemType: '',
  referenceId: '',
  width: 0,
  height: 0,
  x: 0,
  y: 0,
})

// Track mouse position for showing drop target
const { x: mouseX, y: mouseY } = useMouse()

const dropTargetZone = computed(() => {
  if (!isDragging.value) return null

  // Use the actual icon position during drag
  const iconX = currentDraggedItem.x
  const iconY = currentDraggedItem.y

  // Use snapToGrid to get the exact position where the icon will land
  const snapped = desktopStore.snapToGrid(
    iconX,
    iconY,
    currentDraggedItem.width || undefined,
    currentDraggedItem.height || undefined,
  )

  // Show dropzone at snapped position with grid cell size
  const cellSize = desktopStore.gridCellSize

  return {
    x: snapped.x,
    y: snapped.y,
    width: currentDraggedItem.width || cellSize,
    height: currentDraggedItem.height || cellSize,
  }
})

// Window drag state for snap zones
const isWindowDragging = ref(false)
const snapEdgeThreshold = 50 // pixels from edge to show snap zone

// Computed visibility for snap zones (uses mouseX from above)
const showLeftSnapZone = computed(() => {
  return isWindowDragging.value && mouseX.value <= snapEdgeThreshold
})

const showRightSnapZone = computed(() => {
  if (!isWindowDragging.value) return false
  const viewportWidth = window.innerWidth
  return mouseX.value >= viewportWidth - snapEdgeThreshold
})

// Get icons for a specific workspace (uses cached computed from store)
const getWorkspaceIcons = (workspaceId: string) => {
  return desktopStore.getWorkspaceIcons(workspaceId)
}

// Get windows for a specific workspace (including minimized for teleport)
// Native webviews are included only during overview mode
const getWorkspaceWindows = (workspaceId: string) => {
  return windowManager.windows.filter((w) => {
    if (w.isNativeWebview) {
      // Only show native webviews during overview mode
      return windowManager.showWindowOverview
    }
    return w.workspaceId === workspaceId
  })
}

// Windows for current workspace (for overview) - includes native webviews
const currentWorkspaceWindows = computed(() => {
  return windowManager.windows.filter(
    (w) => w.workspaceId === currentWorkspace.value?.id || w.isNativeWebview,
  )
})

// Get Vue Component for system window
const getSystemWindowComponent = (sourceId: string) => {
  const systemWindow = windowManager.getSystemWindow(sourceId)
  return systemWindow?.component
}

const handlePositionChanged = async (id: string, x: number, y: number) => {
  try {
    await desktopStore.updateDesktopItemPositionAsync(id, x, y)
  } catch (error) {
    console.error('Fehler beim Speichern der Position:', error)
  }
}

const handleDragStart = (
  id: string,
  itemType: string,
  referenceId: string,
  width: number,
  height: number,
  x: number,
  y: number,
) => {
  isDragging.value = true
  currentDraggedItem.id = id
  currentDraggedItem.itemType = itemType
  currentDraggedItem.referenceId = referenceId
  currentDraggedItem.width = width
  currentDraggedItem.height = height
  currentDraggedItem.x = x
  currentDraggedItem.y = y
  allowSwipe.value = false // Disable Swiper during icon drag
}

const handleDragging = (id: string, x: number, y: number) => {
  if (currentDraggedItem.id === id) {
    currentDraggedItem.x = x
    currentDraggedItem.y = y
  }
}

const handleDragEnd = async () => {
  // Cleanup drag state
  isDragging.value = false
  currentDraggedItem.id = ''
  currentDraggedItem.itemType = ''
  currentDraggedItem.referenceId = ''
  currentDraggedItem.width = 0
  currentDraggedItem.height = 0
  currentDraggedItem.x = 0
  currentDraggedItem.y = 0
  allowSwipe.value = true // Re-enable Swiper after drag
}

// Handle drag over for launcher items
const handleDragOver = (event: DragEvent) => {
  if (!event.dataTransfer) return

  // Check if this is a launcher item
  if (event.dataTransfer.types.includes('application/haex-launcher-item')) {
    event.dataTransfer.dropEffect = 'copy'
  }
}

// Handle drop for launcher items
const handleDrop = async (event: DragEvent, workspaceId: string) => {
  if (!event.dataTransfer) return

  const launcherItemData = event.dataTransfer.getData(
    'application/haex-launcher-item',
  )
  if (!launcherItemData) return

  try {
    const item = JSON.parse(launcherItemData) as {
      id: string
      name: string
      icon: string
      type: 'system' | 'extension'
    }

    // Get drop position relative to desktop
    const desktopRect = (
      event.currentTarget as HTMLElement
    ).getBoundingClientRect()
    const rawX = Math.max(0, event.clientX - desktopRect.left - 32) // Center icon (64px / 2)
    const rawY = Math.max(0, event.clientY - desktopRect.top - 32)

    // Snap to grid
    const snapped = desktopStore.snapToGrid(rawX, rawY)

    // Create desktop icon on the specific workspace
    await desktopStore.addDesktopItemAsync(
      item.type as DesktopItemType,
      item.id,
      snapped.x,
      snapped.y,
      workspaceId,
    )
  } catch (error: any) {
    console.error('Failed to create desktop icon:', error)
  }
}

const handleDesktopClick = () => {
  // Only clear selection if it was a simple click, not an area selection
  // Check if we just finished an area selection (box size > threshold)
  const boxWidth = Math.abs(selectionEnd.value.x - selectionStart.value.x)
  const boxHeight = Math.abs(selectionEnd.value.y - selectionStart.value.y)

  // If box is larger than 5px in any direction, it was an area select, not a click
  if (boxWidth > 5 || boxHeight > 5) {
    return
  }

  desktopStore.clearSelection()
  isOverviewMode.value = false
}

const handleWindowDragStart = (windowId: string) => {
  console.log('[Desktop] handleWindowDragStart:', windowId)
  isWindowDragging.value = true
  windowManager.draggingWindowId = windowId // Set in store for workspace cards
  console.log(
    '[Desktop] draggingWindowId set to:',
    windowManager.draggingWindowId,
  )
  allowSwipe.value = false // Disable Swiper during window drag
}

const handleWindowDragEnd = async () => {
  console.log('[Desktop] handleWindowDragEnd')

  // Check if window should snap to left or right
  const draggingWindowId = windowManager.draggingWindowId

  if (draggingWindowId) {
    if (showLeftSnapZone.value) {
      // Snap to left half
      windowManager.updateWindowPosition(draggingWindowId, 0, 0)
      windowManager.updateWindowSize(
        draggingWindowId,
        viewportWidth.value / 2,
        viewportHeight.value,
      )
    } else if (showRightSnapZone.value) {
      // Snap to right half
      windowManager.updateWindowPosition(
        draggingWindowId,
        viewportWidth.value / 2,
        0,
      )
      windowManager.updateWindowSize(
        draggingWindowId,
        viewportWidth.value / 2,
        viewportHeight.value,
      )
    }
  }

  isWindowDragging.value = false
  windowManager.draggingWindowId = null // Clear from store
  allowSwipe.value = true // Re-enable Swiper after drag
}

// Area selection handlers
const handleAreaSelectStart = (e: MouseEvent) => {
  if (!desktopEl.value) return

  const rect = desktopEl.value.getBoundingClientRect()
  const x = e.clientX - rect.left
  const y = e.clientY - rect.top

  isAreaSelecting.value = true
  selectionStart.value = { x, y }
  selectionEnd.value = { x, y }

  // Disable Swiper during area selection
  allowSwipe.value = false

  // Clear current selection
  desktopStore.clearSelection()
}

// Overview window drag state
const overviewDragStartPos = ref<{ x: number; y: number } | null>(null)
const overviewDragWindowId = ref<string | null>(null)
const DRAG_THRESHOLD = 5 // pixels before considered a drag vs click

// Drag ghost position and info (uses mouseX/mouseY from above)
const dragGhostPosition = computed(() => ({
  x: mouseX.value,
  y: mouseY.value,
}))

const draggedWindowInfo = computed(() => {
  if (!windowManager.draggingWindowId) return null
  return windowManager.windows.find((w) => w.id === windowManager.draggingWindowId)
})

// Track mouse movement for area selection AND overview window dragging
useEventListener(window, 'mousemove', (e: MouseEvent) => {
  // Area selection handling
  if (isAreaSelecting.value && desktopEl.value) {
    const rect = desktopEl.value.getBoundingClientRect()
    const x = e.clientX - rect.left
    const y = e.clientY - rect.top

    selectionEnd.value = { x, y }

    // Find all items within selection box
    selectItemsInBox()
  }

  // Overview window drag handling
  if (overviewDragWindowId.value && overviewDragStartPos.value) {
    const dx = Math.abs(e.clientX - overviewDragStartPos.value.x)
    const dy = Math.abs(e.clientY - overviewDragStartPos.value.y)

    if ((dx > DRAG_THRESHOLD || dy > DRAG_THRESHOLD) && !windowManager.draggingWindowId) {
      // Start dragging - this triggers WorkspaceCard's drag detection via useMouse()
      windowManager.draggingWindowId = overviewDragWindowId.value
    }
  }
})

// End area selection AND overview window dragging
useEventListener(window, 'mouseup', () => {
  // Area selection handling
  if (isAreaSelecting.value) {
    isAreaSelecting.value = false

    // Re-enable Swiper after area selection
    allowSwipe.value = true

    // Reset selection coordinates after a short delay
    // This allows handleDesktopClick to still check the box size
    setTimeout(() => {
      selectionStart.value = { x: 0, y: 0 }
      selectionEnd.value = { x: 0, y: 0 }
    }, 100)
  }

  // Overview window drag handling
  if (overviewDragWindowId.value) {
    const wasDragging = windowManager.draggingWindowId !== null

    if (!wasDragging) {
      // Was a click, not a drag - activate the window
      const windowId = overviewDragWindowId.value
      const win = windowManager.windows.find((w) => w.id === windowId)
      if (win) {
        // Native webview windows need to be focused via Tauri command
        if (win.isNativeWebview) {
          invoke('focus_extension_webview_window', { windowId })
            .catch((error) => console.error('Failed to focus native window:', error))
        } else if (win.isMinimized) {
          windowManager.restoreWindow(windowId)
        } else {
          windowManager.activateWindow(windowId)
        }
        windowManager.showWindowOverview = false
      }
    }

    // Clean up drag state (with delay for WorkspaceCard to process)
    setTimeout(() => {
      windowManager.draggingWindowId = null
    }, 50)

    overviewDragStartPos.value = null
    overviewDragWindowId.value = null
  }
})

const selectItemsInBox = () => {
  const x1 = Math.min(selectionStart.value.x, selectionEnd.value.x)
  const y1 = Math.min(selectionStart.value.y, selectionEnd.value.y)
  const x2 = Math.max(selectionStart.value.x, selectionEnd.value.x)
  const y2 = Math.max(selectionStart.value.y, selectionEnd.value.y)

  desktopStore.clearSelection()

  desktopItems.value.forEach((item) => {
    // Check if item position is within selection box
    const itemX = item.positionX + 60 // Icon center (approx)
    const itemY = item.positionY + 60

    if (itemX >= x1 && itemX <= x2 && itemY >= y1 && itemY <= y2) {
      desktopStore.toggleSelection(item.id, true) // true = add to selection
    }
  })
}

// Swiper event handlers
const onSwiperInit = (swiper: SwiperType) => {
  swiperInstance.value = swiper
}

const onSlideChange = (swiper: SwiperType) => {
  workspaceStore.switchToWorkspace(
    workspaceStore.workspaces.at(swiper.activeIndex)?.id,
  )
}

// Disable Swiper in overview mode
// Sync isOverviewMode and showWindowOverview - they should always be in sync
watch(isOverviewMode, (newValue) => {
  allowSwipe.value = !newValue
  // Keep showWindowOverview in sync (avoid recursive updates by checking current value)
  if (windowManager.showWindowOverview !== newValue) {
    windowManager.showWindowOverview = newValue
  }
})

watch(
  () => windowManager.showWindowOverview,
  (isOpen) => {
    // Keep isOverviewMode in sync (avoid recursive updates by checking current value)
    if (isOverviewMode.value !== isOpen) {
      isOverviewMode.value = isOpen
    }
  },
)

// Handle mousedown on window in overview mode (starts potential drag)
const handleOverviewMouseDown = (event: MouseEvent, windowId: string) => {
  event.preventDefault()
  overviewDragStartPos.value = { x: event.clientX, y: event.clientY }
  overviewDragWindowId.value = windowId
}

// Handle click on window in overview carousel
const handleOverviewWindowClick = (windowId: string) => {
  // Only handle if not dragging
  if (windowManager.draggingWindowId) return

  const win = windowManager.windows.find((w) => w.id === windowId)
  if (!win) return

  // Native webview windows need to be focused via Tauri command
  if (win.isNativeWebview) {
    invoke('focus_extension_webview_window', { windowId })
      .catch((error) => console.error('Failed to focus native window:', error))
  } else if (win.isMinimized) {
    windowManager.restoreWindow(windowId)
  } else {
    windowManager.activateWindow(windowId)
  }

  // Switch to workspace if needed
  if (win.workspaceId && win.workspaceId !== currentWorkspace.value?.id) {
    workspaceStore.slideToWorkspace(win.workspaceId)
  }

  windowManager.showWindowOverview = false
}

// Calculate window preview size for carousel
const getCarouselWindowStyle = (_window: (typeof windowManager.windows)[0]) => {
  // Fixed card sizes for consistent appearance
  const width = isSmallScreen.value ? 200 : 240
  const height = isSmallScreen.value ? 160 : 180

  return {
    width: `${width}px`,
    height: `${height}px`,
  }
}

// Watch for workspace changes to reload desktop items
watch(currentWorkspace, async () => {
  if (currentWorkspace.value) {
    await desktopStore.loadDesktopItemsAsync()
  }
})

// Reset drag state when mouse leaves the document (fixes stuck dropzone)
useEventListener(document, 'mouseleave', () => {
  if (isDragging.value) {
    isDragging.value = false
    currentDraggedItem.id = ''
    currentDraggedItem.itemType = ''
    currentDraggedItem.referenceId = ''
    currentDraggedItem.width = 0
    currentDraggedItem.height = 0
    currentDraggedItem.x = 0
    currentDraggedItem.y = 0
    allowSwipe.value = true
  }
})

// Keyboard shortcuts
useEventListener(window, 'keydown', async (e: KeyboardEvent) => {
  // Only handle if no input/textarea is focused
  const activeElement = document.activeElement
  if (
    activeElement instanceof HTMLInputElement ||
    activeElement instanceof HTMLTextAreaElement ||
    (activeElement as HTMLElement)?.isContentEditable
  ) {
    return
  }

  // Ctrl/Cmd + A: Select all icons on current workspace
  if ((e.ctrlKey || e.metaKey) && e.key === 'a') {
    e.preventDefault()
    desktopStore.selectAll()
  }

  // Delete/Backspace: Remove selected icons from desktop
  if (e.key === 'Delete' || e.key === 'Backspace') {
    const selectedIds = Array.from(desktopStore.selectedItemIds)
    if (selectedIds.length > 0) {
      e.preventDefault()

      // Remove all selected items from desktop
      for (const itemId of selectedIds) {
        await desktopStore.removeDesktopItemAsync(itemId)
      }

      // Clear selection after removal
      desktopStore.clearSelection()
    }
  }
})

// Poll for initial sync completion (used for remote vault connections)
const waitForInitialSyncAsync = async (): Promise<void> => {
  console.log('[DESKTOP] waitForInitialSyncAsync: Checking if already complete...')
  const isComplete = await vaultSettingsStore.isInitialSyncCompleteAsync()
  if (isComplete) {
    console.log('[DESKTOP] waitForInitialSyncAsync: Already complete, returning immediately')
    return
  }

  console.log('[DESKTOP] waitForInitialSyncAsync: Not complete, starting poll (every 500ms, max 60s timeout)...')

  return new Promise((resolve) => {
    let pollCount = 0
    const maxPolls = 120 // 60 seconds at 500ms intervals
    const { pause } = useIntervalFn(async () => {
      pollCount++
      // Only log every 10 polls to reduce noise
      if (pollCount % 10 === 0) {
        console.log(`[DESKTOP] waitForInitialSyncAsync: Poll #${pollCount}/${maxPolls}, still waiting...`)
      }

      // Timeout after max polls
      if (pollCount >= maxPolls) {
        pause()
        console.warn(`[DESKTOP] waitForInitialSyncAsync: TIMEOUT after ${pollCount} polls (${maxPolls * 500 / 1000}s). Continuing anyway.`)
        resolve()
        return
      }

      const complete = await vaultSettingsStore.isInitialSyncCompleteAsync()
      if (complete) {
        pause()
        console.log(`[DESKTOP] waitForInitialSyncAsync: Complete after ${pollCount} polls!`)
        resolve()
      }
    }, 500) // 500ms interval
  })
}

onMounted(async () => {
  // For remote sync vaults, wait for initial sync to complete before loading
  // This prevents creating a default workspace before synced data arrives
  if (isRemoteSyncVault.value) {
    await waitForInitialSyncAsync()
  }

  // Load workspaces first
  await workspaceStore.loadWorkspacesAsync()

  // Then load desktop items for current workspace
  await desktopStore.loadDesktopItemsAsync()
})
</script>

<style scoped>
.slide-down-enter-active,
.slide-down-leave-active {
  transition: all 0.3s ease;
}

.slide-down-enter-from {
  opacity: 0;
  transform: translateY(-100%);
}

.slide-down-leave-to {
  opacity: 0;
  transform: translateY(-100%);
}

.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.3s ease;
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}
</style>
