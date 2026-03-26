<template>
  <div class="@container h-full flex flex-col">
    <!-- Header -->
    <div class="shrink-0 p-2 @xs:p-3 @sm:p-6 border-b border-default">
      <div class="flex items-center gap-3">
        <UiButton
          v-if="showBack"
          icon="i-heroicons-arrow-left"
          variant="ghost"
          @click="emit('back')"
        />
        <div class="flex-1 min-w-0">
          <h2 class="text-2xl font-bold">
            <slot name="title">{{ title }}</slot>
          </h2>
          <p
            v-if="description || $slots.description"
            class="text-sm text-muted mt-1"
          >
            <slot name="description">{{ description }}</slot>
          </p>
        </div>
      </div>
      <!-- Actions row below title -->
      <div
        v-if="$slots.actions"
        class="flex flex-wrap items-center gap-2 mt-3"
      >
        <slot name="actions" />
      </div>
    </div>

    <!-- Content -->
    <div class="p-2 @xs:p-3 @sm:p-6 flex-1 min-w-0 min-h-0 overflow-y-auto">
      <slot />
    </div>
  </div>
</template>

<script setup lang="ts">
withDefaults(
  defineProps<{
    title?: string
    description?: string
    showBack?: boolean
  }>(),
  {
    title: undefined,
    description: undefined,
    showBack: false,
  },
)

const emit = defineEmits<{
  back: []
}>()
</script>
