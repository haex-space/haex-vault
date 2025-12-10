<template>
  <UCard>
    <div class="flex flex-col gap-3">
      <!-- Header row: Info and button -->
      <div class="flex flex-col @sm:flex-row @sm:items-center justify-between gap-3">
        <div class="flex-1 min-w-0">
          <p class="font-medium">{{ backend.name }}</p>
          <p class="text-sm text-gray-500 dark:text-gray-400 truncate">
            {{ backend.serverUrl }}
          </p>
          <!-- Badges slot - allows custom badges to be injected -->
          <div
            v-if="$slots.badges"
            class="flex flex-wrap gap-2 mt-2"
          >
            <slot name="badges" />
          </div>
        </div>
        <!-- Actions slot - for buttons/actions in the header area -->
        <div
          v-if="$slots.actions"
          class="shrink-0"
        >
          <slot name="actions" />
        </div>
      </div>
    </div>

    <!-- Optional content slot (e.g., for vaults list) -->
    <div
      v-if="$slots.default"
      class="mt-4 pt-4 border-t border-gray-200 dark:border-gray-700"
    >
      <slot />
    </div>
  </UCard>
</template>

<script setup lang="ts">
import type { SelectHaexSyncBackends } from '~/database/schemas'

defineProps<{
  backend: SelectHaexSyncBackends
}>()
</script>
