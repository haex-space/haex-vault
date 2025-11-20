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
      <div class="p-6 flex flex-col h-full overflow-y-auto">
        <div class="w-full mx-auto space-y-4 flex-1">
          <h2 class="text-xl font-semibold">
            {{ title }}
          </h2>

          <slot name="content" />
        </div>

        <!-- Footer (optional) -->
        <div
          v-if="$slots.footer"
          class="mt-12 shrink-0"
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
  >
    <!-- Trigger Button -->
    <slot name="trigger" />

    <!-- Modal Body -->
    <template #body>
      <div class="space-y-4">
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
defineProps<{
  title: string
  description?: string
}>()

const open = defineModel<boolean>('open', { default: false })
const { isSmallScreen } = storeToRefs(useUiStore())
</script>
