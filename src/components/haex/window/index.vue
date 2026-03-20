<template>
  <div
    ref="windowEl"
    :style="windowStyle"
    :class="[
      'absolute bg-default/80 backdrop-blur-xl rounded-lg shadow-xl overflow-hidden',
      'flex flex-col @container',
      // Only apply transition when NOT dragging/resizing for smooth animations
      isResizingOrDragging ? '' : 'transition-all ease-out duration-300',
      { 'select-none': isResizingOrDragging },
      isActive ? 'z-20' : 'z-10',
      // Border colors based on warning level
      warningLevel === 'warning'
        ? 'border-2 border-warning-500'
        : warningLevel === 'danger'
          ? 'border-2 border-danger-500'
          : 'border border-gray-200 dark:border-gray-700',
    ]"
    @mousedown="handleActivate"
    @contextmenu.stop.prevent
  >
    <!-- Window Titlebar — taller on compact windows via container query -->
    <div
      ref="titlebarEl"
      class="flex items-stretch h-10 @max-sm:h-14 bg-white/80 dark:bg-gray-800/80 border-b border-gray-200/50 dark:border-gray-700/50 select-none touch-none"
    >
      <!-- Left: Tabs (or single title if only 1 tab) -->

      <!-- Compact window: USelectMenu for tab switching (when multiple tabs) -->
      <template v-if="isCompactWindow && windowData && windowData.tabs.length > 1">
        <div
          class="flex-1 flex items-center min-w-0 px-2 gap-2"
          @mousedown="handleDragStart"
          @touchstart.passive="handleDragStart"
        >
          <USelectMenu
            :model-value="activeTabSelectItem"
            :items="tabSelectItems"
            class="flex-1 min-w-0"
            @update:model-value="(item: any) => windowManager.switchTab(props.id, item.value)"
            @mousedown.stop
            @touchstart.stop
          />
        </div>
      </template>

      <!-- Standard: tab bar with scroll buttons -->
      <template v-else>
        <!-- Scroll left button (only visible when tabs overflow) -->
        <button
          v-if="tabsOverflowLeft"
          class="px-3 self-stretch flex items-center bg-gray-200/50 dark:bg-gray-700/50 text-highlighted hover:bg-gray-300/50 dark:hover:bg-gray-600/50 transition-colors shrink-0 z-10 min-w-10"
          @mousedown.stop
          @click.stop="scrollTabs('left')"
        >
          <UIcon
            name="i-lucide-chevron-left"
            class="w-4 h-4"
          />
        </button>
        <div
          ref="tabContainerEl"
          class="flex-1 flex items-center min-w-0 overflow-hidden cursor-move"
          @mousedown="handleDragStart"
          @touchstart.passive="handleDragStart"
          @dblclick="handleMaximize"
        >
          <!-- Single tab → just show icon + title (no tab chrome) -->
          <template v-if="windowData?.tabs.length === 1">
            <div class="flex items-center gap-2 px-3">
              <HaexIcon
                v-if="icon"
                :name="icon"
                class="w-4 h-4 object-contain shrink-0"
              />
              <span class="text-sm font-medium truncate">{{
                windowData?.tabs[0] ? resolveTabTitle(windowData.tabs[0]) : title
              }}</span>
            </div>
          </template>

          <!-- Multiple tabs → tab bar -->
          <template v-else-if="windowData?.tabs.length">
            <div
              v-for="tab in windowData.tabs"
              :key="tab.id"
              :class="[
                'flex items-center gap-1.5 px-3 text-sm cursor-pointer self-stretch border-r border-gray-200/30 dark:border-gray-700/30 max-w-48 group transition-colors',
                tab.id === windowData.activeTabId
                  ? 'bg-default/60 font-medium'
                  : 'text-muted hover:bg-default/30',
              ]"
              @mousedown.stop="windowManager.switchTab(props.id, tab.id)"
            >
              <HaexIcon
                v-if="tab.icon"
                :name="tab.icon"
                class="w-3.5 h-3.5 shrink-0"
              />
              <span class="truncate">{{ resolveTabTitle(tab) }}</span>
              <HaexWindowButton
                variant="close"
                @mousedown.stop
                @click.stop="windowManager.closeTab(props.id, tab.id)"
              />
            </div>
          </template>
        </div>
        <!-- Scroll right button (only visible when tabs overflow) -->
        <button
          v-if="tabsOverflowRight"
          class="px-3 self-stretch flex items-center bg-gray-200/50 dark:bg-gray-700/50 text-highlighted hover:bg-gray-300/50 dark:hover:bg-gray-600/50 transition-colors shrink-0 z-10 min-w-10"
          @mousedown.stop
          @click.stop="scrollTabs('right')"
        >
          <UIcon
            name="i-lucide-chevron-right"
            class="w-4 h-4"
          />
        </button>
      </template>

      <!-- "+" button with dropdown for new tab -->
      <div class="flex items-center shrink-0">
        <UDropdownMenu
          :items="newTabMenuItems"
          :ui="{
            content: 'min-w-48 max-h-80 overflow-y-auto',
            item: 'py-2 px-2.5 text-base gap-2.5',
          }"
        >
          <template #item-leading="{ item }">
            <span
              v-if="(item as any).extensionIcon"
              class="flex items-center self-center size-6 shrink-0"
            >
              <HaexIcon
                :name="(item as any).extensionIcon"
                class="size-6"
              />
            </span>
          </template>
          <HaexWindowButton
            variant="add"
            @mousedown.stop
          />
        </UDropdownMenu>
      </div>

      <!-- Right: Window Controls -->
      <div class="flex items-center self-stretch gap-1 px-2 shrink-0">
        <HaexWindowButton
          variant="minimize"
          @click.stop="handleMinimize"
        />

        <HaexWindowButton
          v-if="isMaximized || (!isSmallScreen && !viewportTooSmall)"
          :is-maximized
          variant="maximize"
          @click.stop="handleMaximize"
        />

        <HaexWindowButton
          variant="close"
          @click.stop="handleClose"
        />
      </div>
    </div>

    <!-- Window Content -->
    <div
      :class="[
        'flex-1 overflow-auto relative ',
        isResizingOrDragging ? 'pointer-events-none' : '',
      ]"
    >
      <slot :window-id="props.id" />
    </div>

    <!-- Resize Handles -->
    <HaexWindowResizeHandles
      :disabled="isMaximized || isSmallScreen || viewportTooSmall"
      @resize-start="handleResizeStart"
    />
  </div>
</template>

<script setup lang="ts">
import { getAvailableContentHeight } from '~/utils/viewport'
import type { IWindowTab } from '~/stores/desktop/windowManager'

const windowManager = useWindowManagerStore()

const props = defineProps<{
  id: string
  title: string
  icon?: string | null
  isActive?: boolean
  sourceX?: number
  sourceY?: number
  sourceWidth?: number
  sourceHeight?: number
  isOpening?: boolean
  isClosing?: boolean
  warningLevel?: 'warning' | 'danger'
}>()

const emit = defineEmits<{
  close: []
  minimize: []
  activate: []
  positionChanged: [x: number, y: number]
  sizeChanged: [width: number, height: number]
  dragStart: []
  dragEnd: []
}>()

// Reactive window data (for tabs)
const windowData = computed(() =>
  windowManager.windows.find((w) => w.id === props.id),
)

// Resolve localized tab titles
const { localizedName } = useExtensionI18n()

const resolveTabTitle = (tab: IWindowTab): string => {
  if (tab.type === 'system') {
    return windowManager.getLocalizedSystemWindowName(tab.sourceId)
  }
  // Extensions: use extension i18n map
  const extensionsStore = useExtensionsStore()
  const ext = extensionsStore.availableExtensions.find(
    (e) => e.id === tab.sourceId,
  )
  if (ext?.i18n) return localizedName(tab.title, ext.i18n)
  return tab.title
}

// New tab dropdown menu items
const newTabMenuItems = computed(() => {
  const items: { label: string; icon?: string; onSelect: () => void }[][] = []

  // System windows (non-singleton or not yet open in this window)
  const systemItems = windowManager
    .getAllSystemWindows()
    .filter((window) => {
      if (window.singleton) {
        return !windowData.value?.tabs.some((t) => t.sourceId === window.id)
      }
      return true
    })
    .map((window) => ({
      label: windowManager.getLocalizedSystemWindowName(window.id),
      extensionIcon: window.icon,
      onSelect: () => {
        windowManager.addTab(props.id, {
          type: 'system' as const,
          sourceId: window.id,
          title: window.name,
          icon: window.icon,
        })
      },
    }))

  if (systemItems.length) items.push(systemItems)

  // Extensions (non-singleInstance or not yet open in this window)
  const extensionsStore = useExtensionsStore()
  const extensionItems = extensionsStore.availableExtensions
    .filter((ext) => {
      if (ext.singleInstance) {
        return !windowData.value?.tabs.some((t) => t.sourceId === ext.id)
      }
      return true
    })
    .map((ext) => ({
      label: localizedName(ext.name, ext.i18n),
      extensionIcon:
        ext.iconUrl || ext.icon || 'i-heroicons-puzzle-piece-solid',
      onSelect: () => {
        windowManager.addTab(props.id, {
          type: 'extension' as const,
          sourceId: ext.id,
          title: ext.name,
          icon: ext.iconUrl || ext.icon,
        })
      },
    }))

  if (extensionItems.length) items.push(extensionItems)

  return items
})

// Window-level responsive breakpoints (syncs with @container queries)
const { isCompact: isCompactWindow } = useWindowSize(props.id)

// Tab select items for compact mode (USelectMenu)
const tabSelectItems = computed(() =>
  (windowData.value?.tabs ?? []).map((tab) => ({
    label: resolveTabTitle(tab),
    value: tab.id,
  })),
)
const activeTabSelectItem = computed(() =>
  tabSelectItems.value.find((item) => item.value === windowData.value?.activeTabId),
)

// Tab scrolling
const tabContainerEl = ref<HTMLElement | null>(null)
const tabsOverflowLeft = ref(false)
const tabsOverflowRight = ref(false)

const checkTabOverflow = () => {
  const el = tabContainerEl.value
  if (!el) {
    tabsOverflowLeft.value = false
    tabsOverflowRight.value = false
    return
  }
  tabsOverflowLeft.value = el.scrollLeft > 0
  tabsOverflowRight.value = el.scrollLeft + el.clientWidth < el.scrollWidth - 1
}

const scrollTabs = (direction: 'left' | 'right') => {
  const el = tabContainerEl.value
  if (!el) return
  el.scrollBy({ left: direction === 'left' ? -150 : 150, behavior: 'smooth' })
  setTimeout(checkTabOverflow, 200)
}

// Watch for tab changes and container resize to re-check overflow
watch(
  () => windowData.value?.tabs.length,
  () => nextTick(checkTabOverflow),
)
useResizeObserver(tabContainerEl, () => requestAnimationFrame(checkTabOverflow))
onMounted(() => nextTick(checkTabOverflow))

// Use defineModel for x, y, width, height
const x = defineModel<number>('x', { default: 100 })
const y = defineModel<number>('y', { default: 100 })
const width = defineModel<number>('width', { default: 800 })
const height = defineModel<number>('height', { default: 600 })

const windowEl = useTemplateRef('windowEl')

const uiStore = useUiStore()
const { isSmallScreen } = storeToRefs(uiStore)

// Inject viewport size from parent desktop
const viewportSize = inject<{
  width: Ref<number>
  height: Ref<number>
}>('viewportSize')

// Minimum dimensions for windowed mode
const MIN_WINDOW_WIDTH = 800
const MIN_WINDOW_HEIGHT = 600

// Check if viewport is too small for the requested window size
const viewportTooSmall = computed(() => {
  if (!viewportSize) return isSmallScreen.value
  return (
    viewportSize.width.value < MIN_WINDOW_WIDTH ||
    viewportSize.height.value < MIN_WINDOW_HEIGHT
  )
})

// Start maximized on small screens or when viewport is too small
const isMaximized = ref(isSmallScreen.value || viewportTooSmall.value)

// Store initial position/size for restore
const preMaximizeState = ref({
  x: x.value,
  y: y.value,
  width: width.value,
  height: height.value,
})

// Keep maximized window in sync with viewport size
watch(
  () => [viewportSize?.width.value, viewportSize?.height.value],
  () => {
    if (isMaximized.value && viewportSize) {
      x.value = 0
      y.value = 0
      width.value = viewportSize.width.value
      height.value = getAvailableContentHeight()
    }
  },
)

// Dragging state
const isDragging = ref(false)
const dragStartMouseX = ref(0)
const dragStartMouseY = ref(0)
const dragStartWindowX = ref(0)
const dragStartWindowY = ref(0)

// Resizing state
const isResizing = ref(false)
const resizeDirection = ref<string>('')
const resizeStartX = ref(0)
const resizeStartY = ref(0)
const resizeStartWidth = ref(0)
const resizeStartHeight = ref(0)
const resizeStartPosX = ref(0)
const resizeStartPosY = ref(0)

const isResizingOrDragging = computed(
  () => isResizing.value || isDragging.value,
)

// Drag start handler
const handleDragStart = (e: MouseEvent | TouchEvent) => {
  // Disable dragging when maximized or on small screens
  if (isMaximized.value || isSmallScreen.value) return

  // Don't start drag on button clicks
  if ((e.target as HTMLElement).closest('button')) return

  // Only prevent default for mouse events (touch needs passive)
  if (!('touches' in e)) {
    e.preventDefault()
  }

  const clientX = 'touches' in e ? (e.touches[0]?.clientX ?? 0) : e.clientX
  const clientY = 'touches' in e ? (e.touches[0]?.clientY ?? 0) : e.clientY

  isDragging.value = true
  dragStartMouseX.value = clientX
  dragStartMouseY.value = clientY
  dragStartWindowX.value = x.value
  dragStartWindowY.value = y.value

  emit('dragStart')
}

// Global mouse move for dragging
useEventListener(window, 'mousemove', (e: MouseEvent) => {
  if (!isDragging.value) return

  const deltaX = e.clientX - dragStartMouseX.value
  const deltaY = e.clientY - dragStartMouseY.value

  const newX = dragStartWindowX.value + deltaX
  const newY = dragStartWindowY.value + deltaY

  const constrained = constrainToViewport(newX, newY)
  x.value = constrained.x
  y.value = constrained.y
})

// Global touch move for dragging
useEventListener(
  window,
  'touchmove',
  (e: TouchEvent) => {
    if (!isDragging.value || !e.touches[0]) return

    const deltaX = e.touches[0].clientX - dragStartMouseX.value
    const deltaY = e.touches[0].clientY - dragStartMouseY.value

    const newX = dragStartWindowX.value + deltaX
    const newY = dragStartWindowY.value + deltaY

    const constrained = constrainToViewport(newX, newY)
    x.value = constrained.x
    y.value = constrained.y
  },
  { passive: true },
)

// Global mouse up for drag end
useEventListener(window, 'mouseup', () => {
  if (isDragging.value) {
    isDragging.value = false
    globalThis.getSelection()?.removeAllRanges()
    emit('positionChanged', x.value, y.value)
    emit('dragEnd')
  }

  if (isResizing.value) {
    isResizing.value = false
    globalThis.getSelection()?.removeAllRanges()
    emit('positionChanged', x.value, y.value)
    emit('sizeChanged', width.value, height.value)
  }
})

// Global touch end for drag end
useEventListener(window, 'touchend', () => {
  if (isDragging.value) {
    isDragging.value = false
    emit('positionChanged', x.value, y.value)
    emit('dragEnd')
  }
})

const windowStyle = computed(() => {
  const baseStyle: Record<string, string> = {}

  // Opening animation
  if (
    props.isOpening &&
    props.sourceX !== undefined &&
    props.sourceY !== undefined
  ) {
    baseStyle.left = `${props.sourceX}px`
    baseStyle.top = `${props.sourceY}px`
    baseStyle.width = `${props.sourceWidth || 100}px`
    baseStyle.height = `${props.sourceHeight || 100}px`
    baseStyle.opacity = '0'
    baseStyle.transform = 'scale(0.3)'
  }
  // Closing animation
  else if (
    props.isClosing &&
    props.sourceX !== undefined &&
    props.sourceY !== undefined
  ) {
    baseStyle.left = `${props.sourceX}px`
    baseStyle.top = `${props.sourceY}px`
    baseStyle.width = `${props.sourceWidth || 100}px`
    baseStyle.height = `${props.sourceHeight || 100}px`
    baseStyle.opacity = '0'
    baseStyle.transform = 'scale(0.3)'
  }
  // Closing fallback
  else if (props.isClosing) {
    const centerX = x.value + width.value / 2 - 50
    const centerY = y.value + height.value / 2 - 50
    baseStyle.left = `${centerX}px`
    baseStyle.top = `${centerY}px`
    baseStyle.width = '100px'
    baseStyle.height = '100px'
    baseStyle.opacity = '0'
    baseStyle.transform = 'scale(0.3)'
  }
  // Normal state
  else {
    baseStyle.left = `${x.value}px`
    baseStyle.top = `${y.value}px`
    baseStyle.width = `${width.value}px`
    baseStyle.height = `${height.value}px`
    baseStyle.opacity = '1'

    if (isMaximized.value) {
      baseStyle.borderRadius = '0'
    }
  }

  return baseStyle
})

const getViewportBounds = () => {
  if (viewportSize) {
    return {
      width: viewportSize.width.value,
      height: viewportSize.height.value,
    }
  }

  if (!windowEl.value?.parentElement) return null

  const parent = windowEl.value.parentElement
  return {
    width: parent.clientWidth,
    height: parent.clientHeight,
  }
}

const constrainToViewport = (newX: number, newY: number) => {
  const bounds = getViewportBounds()
  if (!bounds) return { x: newX, y: newY }

  const windowWidth = width.value
  const windowHeight = height.value

  const maxOffscreenX = windowWidth / 3
  const maxOffscreenBottom = windowHeight / 3

  const maxX = bounds.width - windowWidth + maxOffscreenX
  const minX = -maxOffscreenX
  const minY = 0
  const maxY = bounds.height - windowHeight + maxOffscreenBottom

  return {
    x: Math.max(minX, Math.min(maxX, newX)),
    y: Math.max(minY, Math.min(maxY, newY)),
  }
}

const handleActivate = () => {
  emit('activate')
}

const handleClose = () => {
  emit('close')
}

const handleMinimize = () => {
  emit('minimize')
}

const handleMaximize = () => {
  if (isMaximized.value) {
    // On small screens or when viewport is too small, don't allow restore
    if (isSmallScreen.value || viewportTooSmall.value) return

    x.value = preMaximizeState.value.x
    y.value = preMaximizeState.value.y
    width.value = preMaximizeState.value.width
    height.value = preMaximizeState.value.height
    isMaximized.value = false
  } else {
    preMaximizeState.value = {
      x: x.value,
      y: y.value,
      width: width.value,
      height: height.value,
    }

    const bounds = getViewportBounds()

    if (bounds && bounds.width > 0 && bounds.height > 0) {
      x.value = 0
      y.value = 0
      width.value = bounds.width
      height.value = getAvailableContentHeight()
      isMaximized.value = true
    }
  }
}

// Window resizing
const handleResizeStart = (direction: string, e: MouseEvent | TouchEvent) => {
  isResizing.value = true
  resizeDirection.value = direction

  const clientX = 'touches' in e ? (e.touches[0]?.clientX ?? 0) : e.clientX
  const clientY = 'touches' in e ? (e.touches[0]?.clientY ?? 0) : e.clientY

  if ('touches' in e && !e.touches[0]) {
    isResizing.value = false
    return
  }

  resizeStartX.value = clientX
  resizeStartY.value = clientY
  resizeStartWidth.value = width.value
  resizeStartHeight.value = height.value
  resizeStartPosX.value = x.value
  resizeStartPosY.value = y.value
}

// Global handler for resizing
useEventListener(window, 'mousemove', (e: MouseEvent) => {
  if (!isResizing.value) return

  const deltaX = e.clientX - resizeStartX.value
  const deltaY = e.clientY - resizeStartY.value
  const dir = resizeDirection.value

  if (dir.includes('e')) {
    width.value = Math.max(300, resizeStartWidth.value + deltaX)
  } else if (dir.includes('w')) {
    const newWidth = Math.max(300, resizeStartWidth.value - deltaX)
    const widthDiff = resizeStartWidth.value - newWidth
    x.value = resizeStartPosX.value + widthDiff
    width.value = newWidth
  }

  if (dir.includes('s')) {
    height.value = Math.max(200, resizeStartHeight.value + deltaY)
  } else if (dir.includes('n')) {
    const newHeight = Math.max(200, resizeStartHeight.value - deltaY)
    const heightDiff = resizeStartHeight.value - newHeight
    y.value = resizeStartPosY.value + heightDiff
    height.value = newHeight
  }
})
</script>

