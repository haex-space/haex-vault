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
    <!-- Window Titlebar -->
    <div
      ref="titlebarEl"
      class="grid grid-cols-3 items-center px-3 py-1 bg-white/80 dark:bg-gray-800/80 border-b border-gray-200/50 dark:border-gray-700/50 cursor-move select-none touch-none"
      @mousedown="handleDragStart"
      @touchstart.passive="handleDragStart"
      @dblclick="handleMaximize"
    >
      <!-- Left: Icon -->
      <div class="flex items-center gap-2">
        <HaexIcon
          v-if="icon"
          :name="icon"
          :tooltip="title"
          class="w-5 h-5 object-contain shrink-0"
        />
      </div>

      <!-- Center: Title -->
      <div class="flex items-center justify-center">
        <span
          class="text-sm font-medium text-gray-900 dark:text-gray-100 truncate max-w-full"
        >
          {{ title }}
        </span>
      </div>

      <!-- Right: Window Controls -->
      <div class="flex items-center gap-1 justify-end">
        <HaexWindowButton
          variant="minimize"
          @click.stop="handleMinimize"
        />

        <HaexWindowButton
          v-if="!isMaximized || (!isSmallScreen && !viewportTooSmall)"
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

// Use defineModel for x, y, width, height
const x = defineModel<number>('x', { default: 100 })
const y = defineModel<number>('y', { default: 100 })
const width = defineModel<number>('width', { default: 800 })
const height = defineModel<number>('height', { default: 600 })

const windowEl = useTemplateRef('windowEl')
const titlebarEl = useTemplateRef('titlebarEl')

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
useEventListener(window, 'touchmove', (e: TouchEvent) => {
  if (!isDragging.value || !e.touches[0]) return

  const deltaX = e.touches[0].clientX - dragStartMouseX.value
  const deltaY = e.touches[0].clientY - dragStartMouseY.value

  const newX = dragStartWindowX.value + deltaX
  const newY = dragStartWindowY.value + deltaY

  const constrained = constrainToViewport(newX, newY)
  x.value = constrained.x
  y.value = constrained.y
}, { passive: true })

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
  if (props.isOpening && props.sourceX !== undefined && props.sourceY !== undefined) {
    baseStyle.left = `${props.sourceX}px`
    baseStyle.top = `${props.sourceY}px`
    baseStyle.width = `${props.sourceWidth || 100}px`
    baseStyle.height = `${props.sourceHeight || 100}px`
    baseStyle.opacity = '0'
    baseStyle.transform = 'scale(0.3)'
  }
  // Closing animation
  else if (props.isClosing && props.sourceX !== undefined && props.sourceY !== undefined) {
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
