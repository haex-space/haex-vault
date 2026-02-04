<template>
  <div
    ref="elementRef"
    class="w-full h-full relative select-none"
    :class="isOvered ? 'ring-2 ring-blue-500 ring-inset' : ''"
    :style="backgroundStyle"
    @click.self.stop="$emit('desktopClick')"
    @mousedown.left="handleMouseDown"
    @dragover.prevent="$emit('dragOver', $event)"
    @drop.prevent="$emit('drop', $event)"
    @selectstart.prevent
  >
    <slot />
  </div>
</template>

<script setup lang="ts">
import { useDroppable } from '@vue-dnd-kit/core'
import type { IDnDStore } from '@vue-dnd-kit/core'

const props = defineProps<{
  workspaceId: string
  backgroundStyle: Record<string, string | undefined>
}>()

const emit = defineEmits<{
  desktopClick: []
  areaSelectStart: [event: MouseEvent]
  dragOver: [event: DragEvent]
  drop: [event: DragEvent]
  dndDrop: [workspaceId: string, data: unknown, pointerPosition: { x: number; y: number }]
}>()

const desktopStore = useDesktopStore()

const { elementRef, isOvered } = useDroppable({
  groups: ['launcher-item'],
  events: {
    onDrop: async (store: IDnDStore) => {
      // Get the dragged item data
      const draggingElement = store.draggingElements.value.values().next().value
      if (!draggingElement) return false

      const itemData = draggingElement.data as {
        id: string
        type: 'system' | 'extension'
        name: string
        icon: string
      }

      // Get drop position from pointer
      const pointerPos = store.pointerPosition.current.value
      if (!pointerPos) return false

      // Calculate position relative to desktop
      const desktopRect = elementRef.value?.getBoundingClientRect()
      if (!desktopRect) return false

      const rawX = Math.max(0, pointerPos.x - desktopRect.left - 32) // Center icon
      const rawY = Math.max(0, pointerPos.y - desktopRect.top - 32)

      // Snap to grid
      const snapped = desktopStore.snapToGrid(rawX, rawY)

      try {
        // Add desktop item
        await desktopStore.addDesktopItemAsync(
          itemData.type,
          itemData.id,
          snapped.x,
          snapped.y,
          props.workspaceId,
        )

        return true
      } catch (error) {
        console.error('Failed to create desktop icon:', error)
        return false
      }
    },
  },
})

const handleMouseDown = (event: MouseEvent) => {
  // Only emit if clicking directly on the drop zone (not on children)
  if (event.target === elementRef.value) {
    emit('areaSelectStart', event)
  }
}
</script>
