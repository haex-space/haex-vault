<template>
  <div
    ref="elementRef"
    class="size-24 flex flex-wrap text-sm items-center justify-center overflow-visible select-none rounded-lg transition-colors"
    :class="isDragging ? 'opacity-50' : 'cursor-grab hover:bg-gray-100 dark:hover:bg-gray-800'"
    :style="{ touchAction: 'none' }"
    @pointerdown="onPointerDown"
    @pointerup="onPointerUp"
    @pointercancel="onPointerCancel"
    @pointermove="onPointerMove"
    @click="onClick"
    @selectstart.prevent
    @contextmenu.prevent
  >
    <HaexIcon
      :name="icon"
      class="size-10 pointer-events-none"
    />
    <span class="w-full text-center truncate pointer-events-none">
      {{ name }}
    </span>
  </div>
</template>

<script setup lang="ts">
import { useDraggable } from '@vue-dnd-kit/core'

const props = defineProps<{
  id: string
  type: 'system' | 'extension'
  name: string
  icon: string
}>()

const emit = defineEmits<{
  click: []
  dragStart: []
  dragMove: []
}>()

const hasDragged = ref(false)
const pointerDownPosition = ref<{ x: number; y: number } | null>(null)
const isLongPressActive = ref(false)
const lastPointerEvent = ref<PointerEvent | null>(null)
const hasMoved = ref(false)

const { elementRef, handleDragStart, isDragging } = useDraggable({
  groups: ['launcher-item'],
  data: {
    id: props.id,
    type: props.type,
    name: props.name,
    icon: props.icon,
  },
})

// Long press to start drag (for touch devices)
onLongPress(
  elementRef,
  () => {
    if (lastPointerEvent.value && !isDragging.value) {
      isLongPressActive.value = true
      // Trigger haptic feedback if available
      if (navigator.vibrate) {
        navigator.vibrate(50)
      }
      handleDragStart(lastPointerEvent.value)
    }
  },
  {
    delay: 500,
    modifiers: {
      stop: true,
    },
  },
)

// Watch for drag state changes to detect actual dragging
watch(isDragging, (dragging) => {
  if (dragging) {
    hasDragged.value = true
    hasMoved.value = false // Reset move tracking when drag starts
    emit('dragStart')
  } else if (hasDragged.value) {
    // Reset after drag ends
    setTimeout(() => {
      hasDragged.value = false
      pointerDownPosition.value = null
      isLongPressActive.value = false
      lastPointerEvent.value = null
      hasMoved.value = false
    }, 50)
  }
})

const onPointerDown = (event: PointerEvent) => {
  // Store initial position and event for threshold check and long press
  pointerDownPosition.value = { x: event.clientX, y: event.clientY }
  lastPointerEvent.value = event
  isLongPressActive.value = false
}

const onPointerUp = () => {
  pointerDownPosition.value = null
  lastPointerEvent.value = null
}

const onPointerCancel = () => {
  // Handle touch cancel (important for mobile)
  pointerDownPosition.value = null
  lastPointerEvent.value = null
  hasDragged.value = false
  isLongPressActive.value = false
}

const onPointerMove = (event: PointerEvent) => {
  // Update last pointer event for long press
  lastPointerEvent.value = event

  // Only process further if we have a tracked pointer down position
  if (!pointerDownPosition.value) {
    // If dragging but no pointerDownPosition, still track movement for dragMove event
    if (isDragging.value && !hasMoved.value) {
      hasMoved.value = true
      emit('dragMove')
    }
    return
  }

  const dx = Math.abs(event.clientX - pointerDownPosition.value.x)
  const dy = Math.abs(event.clientY - pointerDownPosition.value.y)

  // For desktop: Start drag on pointer move with threshold (no long press needed)
  const isPressed = event.buttons > 0
  const isTouch = event.pointerType === 'touch'

  // Desktop: immediate drag after threshold
  // Touch: drag handled by long press
  if (!isDragging.value && !hasDragged.value && isPressed && !isTouch) {
    if (dx > 5 || dy > 5) {
      handleDragStart(event)
    }
  }

  // Emit dragMove when actual movement is detected during drag
  if (isDragging.value && !hasMoved.value && (dx > 5 || dy > 5)) {
    hasMoved.value = true
    emit('dragMove')
  }
}

const onClick = () => {
  // Only emit click if no drag occurred and no long press was active
  if (!hasDragged.value && !isDragging.value && !isLongPressActive.value) {
    emit('click')
  }
}
</script>
