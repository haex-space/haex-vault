<template>
  <div
    ref="elementRef"
    class="size-24 flex flex-col text-sm items-center justify-center overflow-visible select-none rounded-lg transition-colors"
    :class="isDragging ? 'opacity-50' : 'cursor-grab hover:bg-gray-100 dark:hover:bg-gray-800'"
    :style="{ touchAction: 'none' }"
    :data-testid="`launcher-item-${type}-${id}`"
    @click="onClick"
    @selectstart.prevent
    @contextmenu.prevent
  >
    <HaexIcon
      :name="icon"
      class="size-14 pointer-events-none"
    />
    <span class="w-full text-center truncate pointer-events-none">
      {{ name }}
    </span>
  </div>
</template>

<script setup lang="ts">
import { makeDraggable } from '@vue-dnd-kit/core'
import type { IDragActivationOptions } from '@vue-dnd-kit/core'

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

const elementRef = ref<HTMLElement | null>(null)
const hasDragged = ref(false)

const { isDragging } = makeDraggable(elementRef, {
  groups: ['launcher-item'],
  // Native activation rule replaces the v1 long-press + 5px pointer-threshold
  // dance we had to script ourselves. The root-level `condition: 'any'` means
  // EITHER 5px of movement OR a 500ms hold starts the drag — mouse flicks
  // anywhere past 5px, touch needs the long-press. (The library defaults to
  // `'both'` when distance + delay are both set, which would force the user
  // to satisfy both rules; we don't want that.)
  //
  // The cast is because `IDragActivationOptions` (the input type) omits the
  // root-level `condition`, even though the runtime activation-gate reads it
  // there. `IDragActivation` (the normalized internal shape) has it. We
  // model the wider shape locally so the compiler stops complaining.
  activation: {
    distance: 5,
    delay: 500,
    condition: 'any',
  } as IDragActivationOptions & { condition: 'any' | 'both' },
  data: () => ({
    id: props.id,
    type: props.type,
    name: props.name,
    icon: props.icon,
  }),
  events: {
    onSelfDragStart: () => {
      hasDragged.value = true
      emit('dragStart')
    },
    onSelfDragMove: () => {
      emit('dragMove')
    },
    onSelfDragEnd: () => {
      setTimeout(() => {
        hasDragged.value = false
      }, 50)
    },
    onSelfDragCancel: () => {
      hasDragged.value = false
    },
  },
})

const onClick = () => {
  // Suppress click that follows a drag — drag-end fires before the synthesized
  // click on touch devices, so the 50ms gate above is what keeps the desktop
  // icon from "opening" the moment you drop it.
  if (!hasDragged.value && !isDragging.value) {
    emit('click')
  }
}
</script>
