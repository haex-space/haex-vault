<template>
  <div class="relative py-4">
    <div class="absolute inset-0 flex items-center">
      <div class="w-full border-t border-gray-200 dark:border-gray-700" />
    </div>
    <div
      v-if="label"
      class="relative flex justify-center text-sm"
    >
      <span class="bg-white dark:bg-gray-900 px-3 text-gray-600 dark:text-gray-400">
        {{ label }}
      </span>
    </div>
  </div>
</template>

<script setup lang="ts">
defineProps<{
  label?: string
}>()
</script>
