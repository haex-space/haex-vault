<template>
  <div class="flex items-center justify-between w-full">
    <div class="flex items-center gap-2">
      <h3 class="text-lg font-semibold">{{ title }}</h3>
      <slot name="suffix" />
    </div>
    <UiButton
      icon="i-heroicons-x-mark"
      color="neutral"
      variant="ghost"
      @click="emit('close')"
    />
  </div>
</template>

<script setup lang="ts">
defineProps<{
  title: string
}>()

const emit = defineEmits<{
  close: []
}>()
</script>
