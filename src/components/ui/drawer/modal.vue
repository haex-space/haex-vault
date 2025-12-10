<template>
  <!-- Small Screen: Drawer -->
  <UiDrawer
    v-if="isSmallScreen"
    v-model:open="open"
    :title="title"
    :description="description"
  >
    <!-- Trigger Button -->
    <slot name="trigger" />

    <!-- Drawer Content -->
    <template #content>
      <div class="p-6 pb-[calc(2rem+env(safe-area-inset-bottom))] flex flex-col h-full max-h-[95vh]">
        <!-- Custom Header or default title -->
        <div class="shrink-0 mb-4">
          <slot name="header">
            <h2 class="text-xl font-semibold">
              {{ title }}
            </h2>
          </slot>
        </div>

        <!-- Scrollable Content -->
        <div class="flex-1 overflow-y-auto space-y-4 min-h-0 px-1 pb-4">
          <slot name="content" />
        </div>

        <!-- Footer (optional) -->
        <div
          v-if="$slots.footer"
          class="mt-6 shrink-0"
        >
          <slot name="footer" />
        </div>
      </div>
    </template>
  </UiDrawer>

  <!-- Large Screen: Modal -->
  <UModal
    v-else
    v-model:open="open"
    :title="title"
    :description="description"
    :ui="ui"
  >
    <!-- Trigger Button -->
    <slot name="trigger" />

    <!-- Custom Header (optional) -->
    <template
      v-if="$slots.header"
      #header
    >
      <slot name="header" />
    </template>

    <!-- Modal Body -->
    <template #body>
      <div class="space-y-4 px-4">
        <slot name="content" />
      </div>
    </template>

    <!-- Modal Footer (optional) -->
    <template
      v-if="$slots.footer"
      #footer
    >
      <slot name="footer" />
    </template>
  </UModal>
</template>

<script setup lang="ts">
import type { ModalProps } from '@nuxt/ui'

defineProps<{
  title?: string
  description?: string
  ui?: ModalProps['ui']
}>()

const open = defineModel<boolean>('open', { default: false })
const { isSmallScreen } = storeToRefs(useUiStore())
</script>
