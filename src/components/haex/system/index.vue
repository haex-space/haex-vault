<template>
  <div
    :class="[
      'w-full h-full flex flex-col transition-all duration-300 ease-in-out',
      isDragging ? 'bg-default/80 backdrop-blur-sm' : 'bg-default',
    ]"
  >
    <!-- Header Section -->
    <div
      v-if="title || $slots.header"
      class="shrink-0 p-6 border-b border-gray-200 dark:border-gray-800"
    >
      <slot name="header">
        <div class="space-y-1">
          <h1 class="text-2xl font-bold">
            {{ title }}
          </h1>
          <p
            v-if="description"
            class="text-sm text-gray-500 dark:text-gray-400"
          >
            {{ description }}
          </p>
        </div>
      </slot>
    </div>

    <!-- Main Content Area with optional Sidebar -->
    <div class="flex-1 overflow-hidden flex">
      <!-- Sidebar (optional) -->
      <div
        v-if="$slots.sidebar"
        class="w-16 @md:w-64 border-r border-gray-200 dark:border-gray-800 bg-elevated overflow-y-auto shrink-0"
      >
        <div class="p-2 @md:p-4">
          <slot name="sidebar" />
        </div>
      </div>

      <!-- Content Section -->
      <div class="flex-1 overflow-y-auto">
        <slot />
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
// Base wrapper component for all system windows
// Provides consistent background styling with transparency and blur
// Optional header with title and description
// Optional sidebar (VS Code style)

defineProps<{
  title?: string
  description?: string
  isDragging?: boolean // Whether the window is currently being dragged
}>()
</script>
