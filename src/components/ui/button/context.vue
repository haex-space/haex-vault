<template>
  <div class="w-full [&>span]:flex [&>span]:w-full">
    <UContextMenu :items="contextMenuItems">
      <UiButton
        v-bind="$attrs"
        @click="$emit('click', $event)"
      >
        <template
          v-for="(_, slotName) in $slots"
          #[slotName]="slotProps"
        >
          <slot
            :name="slotName"
            v-bind="slotProps"
          />
        </template>
      </UiButton>
    </UContextMenu>
  </div>
</template>

<script setup lang="ts">
import type { ContextMenuItem } from '@nuxt/ui'

defineProps<{
  contextMenuItems: ContextMenuItem[]
}>()

defineEmits<{ click: [Event] }>()
</script>
