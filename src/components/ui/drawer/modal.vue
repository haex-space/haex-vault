<template>
  <UiDrawer
    v-model:open="open"
    :title="title"
    :description="$slots.description ? undefined : description"
    v-bind="$attrs"
    :ui="{
      content: 'md:w-full md:max-w-4xl md:left-1/2 md:right-auto md:-translate-x-1/2 md:rounded-xl',
    }"
  >
    <!-- Trigger -->
    <slot name="trigger" />

    <!-- Content -->
    <template #content>
      <div class="p-6 pb-[calc(2rem+env(safe-area-inset-bottom))] flex flex-col max-h-[95vh]">
        <!-- Header -->
        <div class="shrink-0 mb-4">
          <slot name="header">
            <h2
              v-if="title"
              class="text-xl font-semibold"
            >
              {{ title }}
            </h2>
            <p
              v-if="$slots.description || description"
              class="mt-1 text-muted text-sm"
            >
              <slot name="description">{{ description }}</slot>
            </p>
          </slot>
        </div>

        <!-- Scrollable Content -->
        <div class="flex-1 overflow-y-auto space-y-4 min-h-0 pt-3 pb-4">
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
</template>

<script setup lang="ts">
defineProps<{
  title?: string
  description?: string
}>()

const open = defineModel<boolean>('open', { default: false })
</script>
