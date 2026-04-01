<template>
  <button
    class="w-full flex items-center gap-3 p-3 rounded-lg text-left hover:bg-muted transition-colors cursor-pointer"
    @click="$emit('click')"
  >
    <UIcon
      v-if="icon"
      :name="icon"
      class="w-5 h-5 shrink-0 text-primary"
    />
    <div class="flex-1 min-w-0">
      <p class="font-medium">{{ label }}</p>
      <p
        v-if="description"
        class="text-sm text-muted mt-0.5 line-clamp-1"
      >
        {{ description }}
      </p>
    </div>
    <slot name="badge" />
    <UIcon
      name="i-lucide-chevron-right"
      class="w-4 h-4 shrink-0 text-muted"
    />
  </button>
</template>

<script setup lang="ts">
defineProps<{
  label: string
  description?: string
  icon?: string
}>()

defineEmits<{
  click: []
}>()
</script>
