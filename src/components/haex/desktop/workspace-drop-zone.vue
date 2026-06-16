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
import { makeDroppable } from '@vue-dnd-kit/core'
import type { IDragEvent } from '@vue-dnd-kit/core'

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
const elementRef = ref<HTMLElement | null>(null)

const { isDragOver } = makeDroppable(elementRef, {
  groups: ['launcher-item'],
  events: {
    onDrop: async (event: IDragEvent) => {
      const dragged = event.draggedItems[0]
      if (!dragged) return false

      const itemData = dragged.data as {
        id: string
        type: 'system' | 'extension'
        name: string
        icon: string
      } | undefined
      if (!itemData) return false

      const pointerPos = event.provider.pointer.value?.current
      if (!pointerPos) return false

      const desktopRect = elementRef.value?.getBoundingClientRect()
      if (!desktopRect) return false

      const rawX = Math.max(0, pointerPos.x - desktopRect.left - 32)
      const rawY = Math.max(0, pointerPos.y - desktopRect.top - 32)

      const snapped = desktopStore.snapToGrid(rawX, rawY)

      try {
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

const isOvered = computed(() => isDragOver.value !== undefined)

const handleMouseDown = (event: MouseEvent) => {
  if (event.target === elementRef.value) {
    emit('areaSelectStart', event)
  }
}
</script>
