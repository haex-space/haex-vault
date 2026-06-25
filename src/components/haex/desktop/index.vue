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
        <HaexDesktopWorkspaceSlide
          :workspace="workspace"
          :workspace-windows="getWorkspaceWindows(workspace.id)"
          :is-area-selecting="isAreaSelecting"
          :selection-box-style="selectionBoxStyle"
          :show-left-snap-zone="showLeftSnapZone"
          :show-right-snap-zone="showRightSnapZone"
          :drop-target-zone="dropTargetZone"
          @desktop-click="handleDesktopClick"
          @area-select-start="handleAreaSelectStart"
          @drag-over="handleDragOver"
          @drop="handleDrop"
          @position-changed="handlePositionChanged"
          @icon-drag-start="handleDragStart"
          @icon-dragging="handleDragging"
          @icon-drag-end="handleDragEnd"
          @request-uninstall="handleRequestUninstall"
          @window-drag-start="handleWindowDragStart"
          @window-drag-end="handleWindowDragEnd"
        />
      </SwiperSlide>
    </Swiper>

    <!-- Window Overview: Carousel -->
    <HaexDesktopOverviewCarousel
      :current-workspace-windows="currentWorkspaceWindows"
      :dragged-window-info="draggedWindowInfo"
      :drag-ghost-position="dragGhostPosition"
      @window-click="handleOverviewWindowClick"
      @window-mousedown="handleOverviewMouseDown"
    />

    <!-- Extension Remove Dialog -->
    <HaexDesktopExtensionUninstallHost ref="uninstallHostRef" />
  </div>
</template>

<script setup lang="ts">
import { Swiper, SwiperSlide } from 'swiper/vue'
import { Navigation } from 'swiper/modules'
import { invoke } from '@tauri-apps/api/core'
import type { Swiper as SwiperType } from 'swiper'
import 'swiper/css'
import 'swiper/css/navigation'

const SwiperNavigation = Navigation

const route = useRoute()
const desktopStore = useDesktopStore()
const windowManager = useWindowManagerStore()
const workspaceStore = useWorkspaceStore()
const vaultSettingsStore = useVaultSettingsStore()

// Check if this is a remote sync vault (initial connection)
const isRemoteSyncVault = computed(() => route.query.remoteSync === 'true')
const { desktopItems } = storeToRefs(desktopStore)
const {
  currentWorkspace,
  currentWorkspaceIndex,
  workspaces,
  swiperInstance,
  allowSwipe,
  isOverviewMode,
} = storeToRefs(workspaceStore)

const desktopEl = useTemplateRef('desktopEl')

// Extension uninstall dialog state (delegated to child)
const uninstallHostRef = useTemplateRef<{ requestUninstall: (id: string) => void }>('uninstallHostRef')

const handleRequestUninstall = (extensionId: string) => {
  uninstallHostRef.value?.requestUninstall(extensionId)
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
  } catch (error) {
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
  isWindowDragging.value = true
  windowManager.draggingWindowId = windowId // Set in store for workspace cards
  allowSwipe.value = false // Disable Swiper during window drag
}

const handleWindowDragEnd = async () => {
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
  const isComplete = await vaultSettingsStore.isInitialSyncCompleteAsync()
  if (isComplete) {
    return
  }

  return new Promise((resolve) => {
    let pollCount = 0
    const maxPolls = 120 // 60 seconds at 500ms intervals
    const { pause } = useIntervalFn(async () => {
      pollCount++
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
