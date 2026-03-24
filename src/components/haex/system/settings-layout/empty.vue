<template>
  <div class="text-center py-6 text-muted">
    <UIcon
      v-if="icon"
      :name="icon"
      class="w-10 h-10 mx-auto mb-2 opacity-50"
    />
    <p class="text-sm">
      <slot>{{ message }}</slot>
    </p>
    <div v-if="$slots.action" class="mt-3">
      <slot name="action" />
    </div>
  </div>
</template>

<script setup lang="ts">
withDefaults(
  defineProps<{
    message?: string
    icon?: string
  }>(),
  {
    message: undefined,
    icon: undefined,
  },
)
</script>
